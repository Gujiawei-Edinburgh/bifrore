mod state;
mod worker;

pub use state::{MemoryStateStore, StateStore};
pub use worker::{NoopWorkerLauncher, WorkerHandle, WorkerLauncher, WorkerStatus};

use tenon_message::plan::{DeploymentPlan, ResourceId};
use tenon_message::state::{StateMutation, StateSnapshot};

pub type DaemonResult<T> = Result<T, DaemonError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonError {
    pub kind: DaemonErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonErrorKind {
    Worker,
    State,
    InvalidState,
}

impl DaemonError {
    pub fn new(kind: DaemonErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }

    pub fn worker(message: impl Into<String>) -> Self {
        Self::new(DaemonErrorKind::Worker, message)
    }

    pub fn state(message: impl Into<String>) -> Self {
        Self::new(DaemonErrorKind::State, message)
    }

    pub fn invalid_state(message: impl Into<String>) -> Self {
        Self::new(DaemonErrorKind::InvalidState, message)
    }
}

impl std::fmt::Display for DaemonError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(formatter, "{:?}: {}", self.kind, self.message)
    }
}

impl std::error::Error for DaemonError {}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ActiveDeployment {
    pub id: ResourceId,
    pub worker: WorkerHandle,
}

pub struct TenonDaemon<L, S> {
    worker_launcher: L,
    state_store: S,
    active: Option<ActiveDeployment>,
}

impl TenonDaemon<NoopWorkerLauncher, MemoryStateStore> {
    pub fn new() -> Self {
        Self::with_components(NoopWorkerLauncher::default(), MemoryStateStore::default())
    }
}

impl Default for TenonDaemon<NoopWorkerLauncher, MemoryStateStore> {
    fn default() -> Self {
        Self::new()
    }
}

impl<L, S> TenonDaemon<L, S>
where
    L: WorkerLauncher,
    S: StateStore,
{
    pub fn with_components(worker_launcher: L, state_store: S) -> Self {
        Self {
            worker_launcher,
            state_store,
            active: None,
        }
    }

    pub fn active_deployment(&self) -> Option<&ActiveDeployment> {
        self.active.as_ref()
    }

    pub async fn apply_plan(&mut self, plan: DeploymentPlan) -> DaemonResult<&ActiveDeployment> {
        self.stop_active_worker()?;
        self.state_store.save_plan(plan.clone()).await?;

        let id = plan
            .id
            .clone()
            .ok_or_else(|| DaemonError::invalid_state("deployment plan id is missing"))?;
        let worker = self.worker_launcher.start(plan)?;
        self.active = Some(ActiveDeployment { id, worker });
        self.active
            .as_ref()
            .ok_or_else(|| DaemonError::invalid_state("active deployment missing after apply"))
    }

    pub fn stop(&mut self) -> DaemonResult<()> {
        self.stop_active_worker()
    }

    pub async fn load_plan(&self) -> DaemonResult<Option<DeploymentPlan>> {
        self.state_store.load_plan().await
    }

    pub async fn load_state(&self, scope: &str, keys: &[String]) -> DaemonResult<StateSnapshot> {
        self.state_store.load(scope, keys).await
    }

    pub async fn commit_state(
        &mut self,
        scope: &str,
        mutations: Vec<StateMutation>,
    ) -> DaemonResult<()> {
        self.state_store.commit(scope, mutations).await
    }

    pub fn worker_status(&mut self) -> DaemonResult<Option<WorkerStatus>> {
        self.active
            .as_ref()
            .map(|active| self.worker_launcher.status(&active.worker))
            .transpose()
    }

    fn stop_active_worker(&mut self) -> DaemonResult<()> {
        if let Some(active) = self.active.take() {
            self.worker_launcher.stop(active.worker)?;
        }
        Ok(())
    }
}
