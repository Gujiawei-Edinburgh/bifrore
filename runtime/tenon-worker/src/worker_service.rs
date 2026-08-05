use crate::{ActivePipeline, WorkerConfig, WorkerError, WorkerResult};
use std::fs;
use std::path::Path;
use std::time::Duration;
use tenon_message::daemon::v1::{
    worker_envelope, worker_response_envelope, GetWorkerStatsResponse, ReloadWorkerRequest,
    ReloadWorkerResponse, StartWorkerRequest, StartWorkerResponse, StopWorkerRequest,
    StopWorkerResponse, WorkerEnvelope, WorkerHelloResponse, WorkerResponseEnvelope, WorkerState,
    WorkerStats,
};
use tenon_transport::{Transport, UdsConnection, UdsTransportConfig, UdsTransportProvider};

#[derive(Debug)]
pub struct WorkerService {
    config: WorkerConfig,
}

impl WorkerService {
    pub fn new(config: WorkerConfig) -> Self {
        Self { config }
    }

    pub fn run_uds(self, socket_path: &Path, worker_id: &str) -> WorkerResult<()> {
        tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .map_err(|error| WorkerError::control(format!("failed to create worker runtime: {error}")))?
            .block_on(self.run_uds_async(socket_path, worker_id))
    }

    async fn run_uds_async(self, socket_path: &Path, worker_id: &str) -> WorkerResult<()> {
        if let Some(parent) = socket_path.parent() {
            fs::create_dir_all(parent).map_err(|error| {
                WorkerError::control(format!(
                    "failed to create worker socket directory {}: {error}",
                    parent.display()
                ))
            })?;
        }
        let _ = fs::remove_file(socket_path);
        let provider = Transport::provide::<UdsTransportProvider>(UdsTransportConfig::default());
        let listener = provider.bind(socket_path).map_err(|error| {
            WorkerError::control(format!(
                "failed to bind worker socket {}: {error}",
                socket_path.display()
            ))
        })?;
        let mut active = None;
        let mut stream: Option<UdsConnection> = None;

        loop {
            if stream.is_none() {
                let candidate = listener.accept().await.map_err(|error| {
                    WorkerError::control(format!("failed to accept daemon worker connection: {error}"))
                })?;
                let mut candidate = candidate;
                if let Err(error) = accept_handshake(&mut candidate, worker_id, &active).await {
                    log::warn!("rejecting worker control connection: {error}");
                    continue;
                }
                stream = Some(candidate);
                continue;
            }

            let current = stream.as_mut().expect("active worker stream");
            tokio::select! {
                accepted = listener.accept() => {
                    match accepted {
                        Ok(_extra) => log::warn!("rejecting additional worker control connection"),
                        Err(error) => log::warn!("failed to accept additional worker connection: {error}"),
                    }
                }
                result = current.receive::<WorkerEnvelope>() => {
                    match result {
                        Ok(envelope) => match handle_worker_envelope(
                            current,
                            envelope,
                            &mut active,
                            &self.config,
                        ).await {
                            Ok(ConnectionResult::Stop) => break,
                            Ok(ConnectionResult::Continue) => {}
                            Err(error) => {
                                if let Some(failure) = active.as_ref().and_then(ActivePipeline::failure) {
                                    let fatal = WorkerError::pipeline(format!(
                                        "worker failure tracker reported {:?} failure: {}",
                                        failure.component, failure.message
                                    ));
                                    if let Some(pipeline) = active.take() {
                                        let _ = pipeline.stop();
                                    }
                                    return Err(fatal);
                                }
                                log::warn!("worker control connection closed: {error}");
                                stream = None;
                            }
                        },
                        Err(error) => {
                            log::warn!("worker control connection closed: {error}");
                            stream = None;
                        }
                    }
                }
            }
        }

        let _ = fs::remove_file(socket_path);
        Ok(())
    }
}

enum ConnectionResult {
    Continue,
    Stop,
}

async fn accept_handshake(
    stream: &mut UdsConnection,
    worker_id: &str,
    active: &Option<ActivePipeline>,
) -> WorkerResult<()> {
    let envelope = tokio::time::timeout(Duration::from_secs(10), stream.receive::<WorkerEnvelope>())
        .await
        .map_err(|_| WorkerError::control("timed out waiting for worker hello"))?
        .map_err(|error| WorkerError::control(format!("failed to read worker hello: {error}")))?;
    let Some(worker_envelope::Payload::WorkerHello(request)) = envelope.payload else {
        return Err(WorkerError::control("worker hello is required before control requests"));
    };
    let hello_error = if request.worker_id != worker_id {
        Some(format!("worker identity mismatch: expected={worker_id} actual={}", request.worker_id))
    } else if request.protocol_version != 1 {
        Some("worker protocol version mismatch".to_string())
    } else {
        None
    };
    stream.send(&WorkerResponseEnvelope {
            payload: Some(worker_response_envelope::Payload::WorkerHello(
                WorkerHelloResponse {
                    worker_id: worker_id.to_string(),
                    protocol_version: 1,
                    state: if hello_error.is_some() {
                        WorkerState::Error as i32
                    } else if active.is_some() {
                        WorkerState::Running as i32
                    } else {
                        WorkerState::Init as i32
                    },
                    error: hello_error.clone().unwrap_or_default(),
                },
            )),
        }).await.map_err(|error| WorkerError::control(format!("failed to write worker response: {error}")))?;
    if hello_error.is_some() {
        return Err(WorkerError::control("worker hello was rejected"));
    }
    Ok(())
}

async fn handle_worker_envelope(
    stream: &mut UdsConnection,
    envelope: WorkerEnvelope,
    active: &mut Option<ActivePipeline>,
    config: &WorkerConfig,
) -> WorkerResult<ConnectionResult> {
    if let Some(failure) = active.as_ref().and_then(ActivePipeline::failure) {
        return Err(WorkerError::pipeline(format!(
            "worker failure tracker reported {:?} failure: {}",
            failure.component, failure.message
        )));
    }
    match envelope.payload {
                Some(worker_envelope::Payload::StartWorker(request)) => {
                    let (state, error) = match replace_pipeline(active, request, config) {
                        Ok(()) => (WorkerState::Running, String::new()),
                        Err(error) => (WorkerState::Error, error.message),
                    };
                    stream.send(&WorkerResponseEnvelope {
                            payload: Some(worker_response_envelope::Payload::StartWorker(
                                StartWorkerResponse {
                                    state: state as i32,
                                    error,
                                },
                            )),
                        }).await.map_err(|error| WorkerError::control(format!("failed to write worker response: {error}")))?;
                    return Ok(ConnectionResult::Continue);
                }
                Some(worker_envelope::Payload::ReloadWorker(request)) => {
                    let (state, error) = match reload_pipeline(active, request) {
                        Ok(()) => (WorkerState::Running, String::new()),
                        Err(error) => (WorkerState::Error, error.message),
                    };
                    stream.send(&WorkerResponseEnvelope {
                            payload: Some(worker_response_envelope::Payload::ReloadWorker(
                                ReloadWorkerResponse {
                                    state: state as i32,
                                    error,
                                },
                            )),
                        }).await.map_err(|error| WorkerError::control(format!("failed to write worker response: {error}")))?;
                    return Ok(ConnectionResult::Continue);
                }
                Some(worker_envelope::Payload::StopWorker(request)) => {
                    let (state, error) = match stop_pipeline(active, request) {
                        Ok(()) => (WorkerState::Stopped, String::new()),
                        Err(error) => (WorkerState::Error, error.message),
                    };
                    stream.send(&WorkerResponseEnvelope {
                            payload: Some(worker_response_envelope::Payload::StopWorker(
                                StopWorkerResponse {
                                    state: state as i32,
                                    error,
                                },
                            )),
                        }).await.map_err(|error| WorkerError::control(format!("failed to write worker response: {error}")))?;
                    return Ok(ConnectionResult::Stop);
                }
                Some(worker_envelope::Payload::GetWorkerStats(_)) => {
                    let response = active
                        .as_ref()
                        .map(|pipeline| worker_stats(pipeline))
                        .unwrap_or_else(|| GetWorkerStatsResponse {
                            stats: Some(WorkerStats::default()),
                            error: "worker has no active pipeline".to_string(),
                        });
                    stream.send(&WorkerResponseEnvelope {
                            payload: Some(
                                tenon_message::daemon::v1::worker_response_envelope::Payload::WorkerStats(response),
                            ),
                        }).await.map_err(|error| WorkerError::control(format!("failed to write worker response: {error}")))?;
                    return Ok(ConnectionResult::Continue);
                }
                Some(worker_envelope::Payload::Heartbeat(_)) => {
                    return Err(WorkerError::control(
                        "daemon must not send heartbeat to worker",
                    ));
                }
                Some(worker_envelope::Payload::WorkerHello(_)) => {
                    return Err(WorkerError::control("worker hello was already completed"));
                }
                None => return Err(WorkerError::control("worker envelope payload is missing")),
    }
}

fn worker_stats(pipeline: &ActivePipeline) -> GetWorkerStatsResponse {
    let snapshot = pipeline.metrics().snapshot();
    GetWorkerStatsResponse {
        stats: Some(WorkerStats {
            processed_messages: snapshot.processed_messages,
            processor_errors: snapshot.processor_errors,
            emitted_records: snapshot.emitted_records,
            egress_enqueued_records: snapshot.egress_enqueued_records,
            egress_delivered_records: snapshot.egress_delivered_records,
            egress_dropped_records: snapshot.egress_dropped_records,
            egress_dropped_queue_full_records: snapshot.egress_dropped_queue_full_records,
            egress_dropped_stopped_records: snapshot.egress_dropped_stopped_records,
            egress_dropped_no_consumer_records: snapshot.egress_dropped_no_consumer_records,
            egress_dropped_slow_consumer_records: snapshot.egress_dropped_slow_consumer_records,
            egress_dropped_incomplete_frame_records: snapshot.egress_dropped_incomplete_frame_records,
            egress_dropped_oversized_records: snapshot.egress_dropped_oversized_records,
            egress_dropped_encode_error_records: snapshot.egress_dropped_encode_error_records,
        }),
        error: String::new(),
    }
}

fn replace_pipeline(
    active: &mut Option<ActivePipeline>,
    request: StartWorkerRequest,
    config: &WorkerConfig,
) -> WorkerResult<()> {
    if let Some(pipeline) = active.take() {
        pipeline.stop()?;
    }
    let plan = request
        .plan
        .ok_or_else(|| WorkerError::control("start worker plan is missing"))?;
    *active = Some(ActivePipeline::start(
        plan,
        request.source_client_ids,
        config.clone(),
    )?);
    Ok(())
}

fn reload_pipeline(
    active: &mut Option<ActivePipeline>,
    request: ReloadWorkerRequest,
) -> WorkerResult<()> {
    let plan = request
        .plan
        .ok_or_else(|| WorkerError::control("reload worker plan is missing"))?;
    let pipeline = active
        .as_mut()
        .ok_or_else(|| WorkerError::control("reload requires an active pipeline"))?;
    pipeline.reload_process(plan)?;
    Ok(())
}

fn stop_pipeline(
    active: &mut Option<ActivePipeline>,
    _request: StopWorkerRequest,
) -> WorkerResult<()> {
    if let Some(pipeline) = active.take() {
        pipeline.stop()?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::UnixStream;
    use tenon_transport::{FramedTransport, TransportConfig};
    use tenon_message::daemon::v1::{
        worker_envelope, worker_response_envelope, StopWorkerRequest, WorkerEnvelope,
        WorkerHelloRequest, WorkerResponseEnvelope,
    };

    #[test]
    fn stop_without_active_pipeline_is_ok() {
        let mut active = None;

        stop_pipeline(&mut active, StopWorkerRequest {}).expect("stop");

        assert!(active.is_none());
    }

    #[test]
    fn worker_requires_and_answers_control_handshake() {
        tokio::runtime::Builder::new_current_thread()
            .enable_io()
            .enable_time()
            .build()
            .expect("runtime")
            .block_on(async {
                let (daemon_stream, worker_stream) =
                    UnixStream::pair().expect("socket pair");
                let mut daemon_stream = FramedTransport::new(daemon_stream, TransportConfig::default());
                let mut worker_stream = FramedTransport::new(worker_stream, TransportConfig::default());
                let request = WorkerEnvelope {
                    payload: Some(worker_envelope::Payload::WorkerHello(WorkerHelloRequest {
                        worker_id: "worker-1".to_string(),
                        protocol_version: 1,
                    })),
                };
                daemon_stream.send(&request).await.expect("write hello");
                accept_handshake(&mut worker_stream, "worker-1", &None)
                    .await
                    .expect("accept handshake");
                let response = daemon_stream
                    .receive::<WorkerResponseEnvelope>()
                    .await
                    .expect("read response");
                assert!(matches!(
                    response.payload,
                    Some(worker_response_envelope::Payload::WorkerHello(_))
                ));
            });
    }
}
