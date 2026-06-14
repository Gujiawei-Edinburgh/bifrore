mod state;
mod worker;

pub use state::{MemoryStateStore, StateStore};
pub use worker::{NoopWorkerLauncher, WorkerHandle, WorkerLauncher, WorkerStatus};

use std::collections::HashMap;
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
    NotFound,
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

    pub fn not_found(message: impl Into<String>) -> Self {
        Self::new(DaemonErrorKind::NotFound, message)
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DeploymentKey {
    pub kind: i32,
    pub name: String,
    pub version: String,
}

impl DeploymentKey {
    pub fn from_id(id: &ResourceId) -> Self {
        Self {
            kind: id.kind,
            name: id.name.clone(),
            version: id.version.clone(),
        }
    }

    pub fn as_store_key(&self) -> String {
        format!("{}/{}/{}", self.kind, self.name, self.version)
    }
}

pub struct TenonDaemon<L, S> {
    worker_launcher: L,
    state_store: S,
    deployments: HashMap<DeploymentKey, ActiveDeployment>,
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
            deployments: HashMap::new(),
        }
    }

    pub fn deployments(&self) -> impl Iterator<Item = &ActiveDeployment> {
        self.deployments.values()
    }

    pub async fn apply_plan(&mut self, plan: DeploymentPlan) -> DaemonResult<&ActiveDeployment> {
        let id = plan
            .id
            .clone()
            .ok_or_else(|| DaemonError::invalid_state("deployment plan id is missing"))?;
        let key = DeploymentKey::from_id(&id);
        self.state_store.save_plan(&key, plan.clone()).await?;
        self.stop_worker_by_key(&key)?;

        let worker = self.worker_launcher.start(plan)?;
        self.deployments
            .insert(key.clone(), ActiveDeployment { id, worker });
        self.deployments
            .get(&key)
            .ok_or_else(|| DaemonError::invalid_state("deployment missing after apply"))
    }

    pub fn stop(&mut self) -> DaemonResult<()> {
        let deployments = std::mem::take(&mut self.deployments);
        for deployment in deployments.into_values() {
            self.worker_launcher.stop(deployment.worker)?;
        }
        Ok(())
    }

    pub async fn load_plan(&self, id: &ResourceId) -> DaemonResult<Option<DeploymentPlan>> {
        let key = DeploymentKey::from_id(id);
        self.state_store.load_plan(&key).await
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

    pub fn worker_status(&mut self, id: &ResourceId) -> DaemonResult<WorkerStatus> {
        let key = DeploymentKey::from_id(id);
        let deployment = self
            .deployments
            .get(&key)
            .ok_or_else(|| DaemonError::not_found("deployment worker not found"))?;
        self.worker_launcher.status(&deployment.worker)
    }

    fn stop_worker_by_key(&mut self, key: &DeploymentKey) -> DaemonResult<()> {
        if let Some(deployment) = self.deployments.remove(key) {
            self.worker_launcher.stop(deployment.worker)?;
        }
        Ok(())
    }
}
