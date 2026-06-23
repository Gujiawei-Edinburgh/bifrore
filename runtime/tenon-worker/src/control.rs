use crate::{ActivePipeline, WorkerConfig, WorkerError, WorkerResult};
use prost::Message;
use std::io::Read;
use std::os::unix::net::UnixStream;
use std::path::Path;
use tenon_message::daemon::v1::{
    worker_envelope, ReloadWorkerRequest, StartWorkerRequest, StopWorkerRequest, WorkerEnvelope,
};

#[derive(Debug)]
pub struct WorkerControl {
    config: WorkerConfig,
}

impl WorkerControl {
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
        let mut active = None;

        loop {
            let envelope = read_worker_envelope(&mut stream)?;
            match envelope.payload {
                Some(worker_envelope::Payload::StartWorker(request)) => {
                    replace_pipeline(&mut active, request, &self.config)?;
                }
                Some(worker_envelope::Payload::ReloadWorker(request)) => {
                    reload_pipeline(&mut active, request)?;
                }
                Some(worker_envelope::Payload::StopWorker(request)) => {
                    stop_pipeline(&mut active, request)?;
                    break;
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

fn read_worker_envelope(stream: &mut UnixStream) -> WorkerResult<WorkerEnvelope> {
    let mut header = [0u8; 4];
    stream
        .read_exact(&mut header)
        .map_err(|error| WorkerError::control(format!("failed to read worker frame: {error}")))?;
    let len = u32::from_le_bytes(header) as usize;
    let mut payload = vec![0u8; len];
    stream
        .read_exact(&mut payload)
        .map_err(|error| WorkerError::control(format!("failed to read worker frame: {error}")))?;
    WorkerEnvelope::decode(payload.as_slice())
        .map_err(|error| WorkerError::control(format!("failed to decode worker frame: {error}")))
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
