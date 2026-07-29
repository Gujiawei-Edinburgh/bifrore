use crate::{ActivePipeline, WorkerConfig, WorkerError, WorkerResult};
use prost::Message;
use std::io::{Read, Write};
use std::os::unix::net::UnixStream;
use std::path::Path;
use std::time::Duration;
use tenon_message::daemon::v1::{
    worker_envelope, worker_response_envelope, GetWorkerStatsResponse, ReloadWorkerRequest,
    ReloadWorkerResponse, StartWorkerRequest, StartWorkerResponse, StopWorkerRequest,
    StopWorkerResponse, WorkerEnvelope, WorkerResponseEnvelope, WorkerState, WorkerStats,
};

#[derive(Debug)]
pub struct WorkerService {
    config: WorkerConfig,
}

impl WorkerService {
    pub fn new(config: WorkerConfig) -> Self {
        Self { config }
    }

    pub fn run_uds(self, socket_path: &Path) -> WorkerResult<()> {
        let mut stream = UnixStream::connect(socket_path).map_err(|error| {
            WorkerError::control(format!(
                "failed to connect daemon worker socket {}: {error}",
                socket_path.display()
            ))
        })?;
        stream
            .set_read_timeout(Some(Duration::from_millis(100)))
            .map_err(|error| {
                WorkerError::control(format!("failed to configure worker control timeout: {error}"))
            })?;
        let mut active = None;

        loop {
            if let Some(failure) = active.as_ref().and_then(ActivePipeline::failure) {
                let error = WorkerError::pipeline(format!(
                    "worker supervisor reported {:?} failure: {}",
                    failure.component, failure.message
                ));
                if let Some(pipeline) = active.take() {
                    let _ = pipeline.stop();
                }
                return Err(error);
            }

            let Some(envelope) = read_worker_envelope(&mut stream)? else {
                continue;
            };
            match envelope.payload {
                Some(worker_envelope::Payload::StartWorker(request)) => {
                    let (state, error) = match replace_pipeline(&mut active, request, &self.config) {
                        Ok(()) => (WorkerState::Running, String::new()),
                        Err(error) => (WorkerState::Error, error.message),
                    };
                    write_worker_response(
                        &mut stream,
                        WorkerResponseEnvelope {
                            payload: Some(worker_response_envelope::Payload::StartWorker(
                                StartWorkerResponse {
                                    state: state as i32,
                                    error,
                                },
                            )),
                        },
                    )?;
                }
                Some(worker_envelope::Payload::ReloadWorker(request)) => {
                    let (state, error) = match reload_pipeline(&mut active, request) {
                        Ok(()) => (WorkerState::Running, String::new()),
                        Err(error) => (WorkerState::Error, error.message),
                    };
                    write_worker_response(
                        &mut stream,
                        WorkerResponseEnvelope {
                            payload: Some(worker_response_envelope::Payload::ReloadWorker(
                                ReloadWorkerResponse {
                                    state: state as i32,
                                    error,
                                },
                            )),
                        },
                    )?;
                }
                Some(worker_envelope::Payload::StopWorker(request)) => {
                    let (state, error) = match stop_pipeline(&mut active, request) {
                        Ok(()) => (WorkerState::Stopped, String::new()),
                        Err(error) => (WorkerState::Error, error.message),
                    };
                    write_worker_response(
                        &mut stream,
                        WorkerResponseEnvelope {
                            payload: Some(worker_response_envelope::Payload::StopWorker(
                                StopWorkerResponse {
                                    state: state as i32,
                                    error,
                                },
                            )),
                        },
                    )?;
                    break;
                }
                Some(worker_envelope::Payload::GetWorkerStats(_)) => {
                    let response = active
                        .as_ref()
                        .map(|pipeline| worker_stats(pipeline))
                        .unwrap_or_else(|| GetWorkerStatsResponse {
                            stats: Some(WorkerStats::default()),
                            error: "worker has no active pipeline".to_string(),
                        });
                    write_worker_response(
                        &mut stream,
                        WorkerResponseEnvelope {
                            payload: Some(
                                tenon_message::daemon::v1::worker_response_envelope::Payload::WorkerStats(response),
                            ),
                        },
                    )?;
                }
                Some(worker_envelope::Payload::Heartbeat(_)) => {
                    return Err(WorkerError::control(
                        "daemon must not send heartbeat to worker",
                    ));
                }
                None => return Err(WorkerError::control("worker envelope payload is missing")),
            }
        }

        Ok(())
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

fn read_worker_envelope(stream: &mut UnixStream) -> WorkerResult<Option<WorkerEnvelope>> {
    let mut header = [0u8; 4];
    match stream.read_exact(&mut header) {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::TimedOut => return Ok(None),
        Err(error) => {
            return Err(WorkerError::control(format!(
                "failed to read worker frame: {error}"
            )))
        }
    }
    let len = u32::from_le_bytes(header) as usize;
    let mut payload = vec![0u8; len];
    stream
        .read_exact(&mut payload)
        .map_err(|error| WorkerError::control(format!("failed to read worker frame: {error}")))?;
    WorkerEnvelope::decode(payload.as_slice())
        .map(Some)
        .map_err(|error| WorkerError::control(format!("failed to decode worker frame: {error}")))
}

fn write_worker_response(
    stream: &mut UnixStream,
    response: WorkerResponseEnvelope,
) -> WorkerResult<()> {
    let mut payload = Vec::new();
    response
        .encode(&mut payload)
        .map_err(|error| WorkerError::control(format!("failed to encode worker response: {error}")))?;
    let length = u32::try_from(payload.len())
        .map_err(|_| WorkerError::control("worker response is too large"))?;
    stream
        .write_all(&length.to_le_bytes())
        .and_then(|_| stream.write_all(&payload))
        .and_then(|_| stream.flush())
        .map_err(|error| WorkerError::control(format!("failed to write worker response: {error}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use tenon_message::daemon::v1::StopWorkerRequest;

    #[test]
    fn stop_without_active_pipeline_is_ok() {
        let mut active = None;

        stop_pipeline(&mut active, StopWorkerRequest {}).expect("stop");

        assert!(active.is_none());
    }
}
