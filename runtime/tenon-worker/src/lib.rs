mod control;
mod mqtt;
mod pipeline;

pub use control::WorkerControl;
pub use pipeline::{ActivePipeline, WorkerConfig};

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
}

impl std::fmt::Display for WorkerError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for WorkerError {}

#[derive(Debug)]
pub struct TenonWorker {
    control: WorkerControl,
}

impl TenonWorker {
    pub fn new(config: WorkerConfig) -> Self {
        Self {
            control: WorkerControl::new(config),
        }
    }

    pub fn run_uds(self, socket_path: impl AsRef<std::path::Path>) -> WorkerResult<()> {
        self.control.run_uds(socket_path.as_ref())
    }
}
