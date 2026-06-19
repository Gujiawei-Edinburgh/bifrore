mod service;
mod store;
mod worker;

pub use service::DaemonService;
pub use store::{DaemonStore, InMemoryDaemonStore};
pub use worker::{NoopWorkerLauncher, WorkerHandle, WorkerLauncher, WorkerStatus};

use std::collections::HashMap;
use tenon_message::plan::{resource, DeploymentPlan, Resource, ResourceId, ResourceKind};

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

    pub async fn put_resource(&mut self, resource: Resource) -> DaemonResult<ResourceId> {
        let id = resource_id_from_resource(&resource)
            .ok_or_else(|| DaemonError::invalid_state("resource id is missing"))?;
        let pipeline = match &resource.kind {
            Some(resource::Kind::Pipeline(plan)) => Some(plan.clone()),
            Some(_) => None,
            None => return Err(DaemonError::invalid_state("resource payload is missing")),
        };

        self.store.save_resource(resource).await?;
        if let Some(plan) = pipeline {
            self.apply(plan).await?;
        }
        Ok(id)
    }

    pub async fn get_resource(&self, id: &ResourceId) -> DaemonResult<Option<Resource>> {
        let key = ResourceKey::from_id(id);
        self.store.load_resource(&key).await
    }

    pub async fn update_resource(
        &mut self,
        previous_id: &ResourceId,
        resource: Resource,
    ) -> DaemonResult<Vec<ResourceId>> {
        let previous_key = ResourceKey::from_id(previous_id);
        let affected_pipelines = self.store.load_referencing_plans(&previous_key).await?;
        self.store.save_resource(resource).await?;
        Ok(affected_pipelines)
    }

    pub async fn delete_resource(&mut self, id: &ResourceId) -> DaemonResult<bool> {
        let key = ResourceKey::from_id(id);

        if ResourceKind::try_from(id.kind) == Ok(ResourceKind::Pipeline) {
            self.stop_worker_by_key(&key)?;
            return self.store.delete_resource(&key).await;
        }

        let references = self.store.load_referencing_plans(&key).await?;
        if !references.is_empty() {
            return Err(DaemonError::invalid_state(
                "resource is still referenced by pipelines",
            ));
        }

        self.store.delete_resource(&key).await
    }

    pub async fn apply(&mut self, plan: DeploymentPlan) -> DaemonResult<&ActiveDeployment> {
        let id = plan
            .id
            .clone()
            .ok_or_else(|| DaemonError::invalid_state("deployment plan id is missing"))?;
        let key = DeploymentKey::from_id(&id);
        self.stop_worker_by_key(&key)?;

        let worker = self.worker_launcher.start(plan)?;
        self.deployments
            .insert(key.clone(), ActiveDeployment { id, worker });
        self.deployments
            .get(&key)
            .ok_or_else(|| DaemonError::invalid_state("deployment missing after apply"))
    }

    pub async fn reload(&mut self, plan: DeploymentPlan) -> DaemonResult<&ActiveDeployment> {
        self.apply(plan).await
    }

    pub fn stop(&mut self) -> DaemonResult<()> {
        let deployments = std::mem::take(&mut self.deployments);
        for deployment in deployments.into_values() {
            self.worker_launcher.stop(deployment.worker)?;
        }
        Ok(())
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

fn resource_id_from_resource(resource: &Resource) -> Option<ResourceId> {
    match &resource.kind {
        Some(resource::Kind::Pipeline(pipeline)) => pipeline.id.clone(),
        Some(resource::Kind::MqttSource(source)) => source.id.clone(),
        Some(resource::Kind::Process(process)) => process.id.clone(),
        Some(resource::Kind::Egress(egress)) => egress.id.clone(),
        None => None,
    }
}
