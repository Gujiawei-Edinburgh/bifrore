mod service;
mod store;
mod worker;

pub use service::DaemonService;
pub use store::{DaemonStore, InMemoryDaemonStore};
pub use worker::{NoopWorkerLauncher, WorkerHandle, WorkerLauncher, WorkerStatus};

use std::collections::HashMap;
use tenon_message::plan::{DeploymentPlan, ResourceId};

pub type DaemonResult<T> = Result<T, DaemonError>;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonError {
    pub kind: DaemonErrorKind,
    pub message: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonErrorKind {
    Worker,
    Store,
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

    pub fn store(message: impl Into<String>) -> Self {
        Self::new(DaemonErrorKind::Store, message)
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
pub struct ResourceKey {
    pub kind: i32,
    pub name: String,
    pub version: String,
}

impl ResourceKey {
    pub fn from_id(id: &ResourceId) -> Self {
        Self {
            kind: id.kind,
            name: id.name.clone(),
            version: id.version.clone(),
        }
    }

    pub fn as_store_key(&self) -> String {
        format!(
            "{}:{}:{}",
            resource_kind_label(self.kind),
            self.name,
            self.version
        )
    }
}

pub type DeploymentKey = ResourceKey;

fn resource_kind_label(kind: i32) -> String {
    match tenon_message::plan::ResourceKind::try_from(kind) {
        Ok(tenon_message::plan::ResourceKind::MqttSource) => "mqtt_source".to_string(),
        Ok(tenon_message::plan::ResourceKind::Egress) => "egress".to_string(),
        Ok(tenon_message::plan::ResourceKind::Process) => "process".to_string(),
        Ok(tenon_message::plan::ResourceKind::Pipeline) => "pipeline".to_string(),
        Ok(tenon_message::plan::ResourceKind::Unspecified) | Err(_) => {
            format!("kind_{}", kind)
        }
    }
}

pub struct TenonDaemon<L, P> {
    worker_launcher: L,
    store: P,
    deployments: HashMap<DeploymentKey, ActiveDeployment>,
}

impl TenonDaemon<NoopWorkerLauncher, InMemoryDaemonStore> {
    pub fn new() -> Self {
        Self::with_components(NoopWorkerLauncher::default(), InMemoryDaemonStore::default())
    }
}

impl Default for TenonDaemon<NoopWorkerLauncher, InMemoryDaemonStore> {
    fn default() -> Self {
        Self::new()
    }
}

impl<L, P> TenonDaemon<L, P>
where
    L: WorkerLauncher,
    P: DaemonStore,
{
    pub fn with_components(worker_launcher: L, store: P) -> Self {
        Self {
            worker_launcher,
            store,
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
        self.store.save_plan(plan.clone()).await?;
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
        self.store.load_plan(&key).await
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
