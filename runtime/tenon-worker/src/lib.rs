mod egress;
mod auth;
mod metrics;
mod mqtt;
mod pipeline;
mod processor;
mod failure_tracker;
mod worker_service;

pub use egress::{Egress, EgressConfig, EgressRuntime};
pub use metrics::{EgressDropReason, WorkerMetrics, WorkerMetricsSnapshot};
pub use pipeline::{ActivePipeline, WorkerConfig};
pub use processor::{LuaProcessor, Processor};
pub use failure_tracker::{WorkerComponent, WorkerFailure, WorkerFailureTracker};
pub use worker_service::WorkerService;

pub type WorkerResult<T> = Result<T, WorkerError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerError {
    pub kind: WorkerErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerErrorKind {
    Control,
    Mqtt,
    Pipeline,
    Processor,
}

impl WorkerError {
    pub fn new(kind: WorkerErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn control(message: impl Into<String>) -> Self {
        Self::new(WorkerErrorKind::Control, message)
    }

    pub fn mqtt(message: impl Into<String>) -> Self {
        Self::new(WorkerErrorKind::Mqtt, message)
    }

    pub fn pipeline(message: impl Into<String>) -> Self {
        Self::new(WorkerErrorKind::Pipeline, message)
    }

    pub fn processor(message: impl Into<String>) -> Self {
        Self::new(WorkerErrorKind::Processor, message)
    }
}

impl std::fmt::Display for WorkerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for WorkerError {}

#[derive(Debug)]
pub struct TenonWorker {
    service: WorkerService,
}

impl TenonWorker {
    pub fn new(config: WorkerConfig) -> Self {
        Self {
            service: WorkerService::new(config),
        }
    }

    pub fn run_uds(self, socket_path: impl AsRef<std::path::Path>) -> WorkerResult<()> {
        self.service.run_uds(socket_path.as_ref())
    }
}
