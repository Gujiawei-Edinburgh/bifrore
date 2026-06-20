use crate::DaemonResult;
use std::sync::atomic::{AtomicU64, Ordering};
use tenon_message::plan::DeploymentPlan;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerHandle {
    pub id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerStatus {
    Init,
    Starting,
    Running,
    Stopping,
    Stopped,
    Error,
}

pub trait WorkerLauncher {
    fn start(&mut self, plan: DeploymentPlan) -> DaemonResult<WorkerHandle>;

    fn reload(&mut self, worker: &WorkerHandle, plan: DeploymentPlan) -> DaemonResult<()>;

    fn stop(&mut self, worker: WorkerHandle) -> DaemonResult<()>;

    fn status(&mut self, worker: &WorkerHandle) -> DaemonResult<WorkerStatus>;
}

#[derive(Debug, Default)]
pub struct NoopWorkerLauncher {
    next_id: AtomicU64,
}

impl WorkerLauncher for NoopWorkerLauncher {
    fn start(&mut self, _plan: DeploymentPlan) -> DaemonResult<WorkerHandle> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        Ok(WorkerHandle {
            id: format!("worker-{id}"),
        })
    }

    fn reload(&mut self, _worker: &WorkerHandle, _plan: DeploymentPlan) -> DaemonResult<()> {
        Ok(())
    }

    fn stop(&mut self, _worker: WorkerHandle) -> DaemonResult<()> {
        Ok(())
    }

    fn status(&mut self, _worker: &WorkerHandle) -> DaemonResult<WorkerStatus> {
        Ok(WorkerStatus::Running)
    }
}
