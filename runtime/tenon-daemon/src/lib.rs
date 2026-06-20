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

#[derive(Debug, Clone, PartialEq)]
pub struct ActiveDeployment {
    pub id: ResourceId,
    pub plan: DeploymentPlan,
    pub worker: WorkerHandle,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonApplyMode {
    Started,
    HotReload,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonApplyResult {
    pub id: ResourceId,
    pub mode: DaemonApplyMode,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonPutPipelineResult {
    pub id: ResourceId,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ResourceKey {
    pub name: String,
    pub version: String,
}

impl ResourceKey {
    pub fn from_id(id: &ResourceId) -> Self {
        Self {
            name: id.name.clone(),
            version: id.version.clone(),
        }
    }

    pub fn as_store_key(&self) -> String {
        format!("{}:{}", self.name, self.version)
    }
}

pub type DeploymentKey = ResourceKey;

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

    pub async fn put_pipeline(
        &mut self,
        pipeline: DeploymentPlan,
    ) -> DaemonResult<DaemonPutPipelineResult> {
        let pipeline = self.assign_pipeline_revision(pipeline).await?;
        let id = pipeline
            .id
            .clone()
            .ok_or_else(|| DaemonError::invalid_state("pipeline id is missing"))?;

        self.store.save_pipeline(pipeline).await?;
        Ok(DaemonPutPipelineResult { id })
    }

    pub async fn apply_pipeline(
        &mut self,
        pipeline_name: &str,
        pipeline_ver: Option<&str>,
    ) -> DaemonResult<DaemonApplyResult> {
        let id = self.get_pipeline_key(pipeline_name, pipeline_ver).await?;
        let plan = self
            .store
            .load_pipeline(&id)
            .await?
            .ok_or_else(|| DaemonError::not_found("pipeline resource not found"))?;
        self.apply(plan).await
    }

    async fn apply(&mut self, plan: DeploymentPlan) -> DaemonResult<DaemonApplyResult> {
        let id = plan
            .id
            .clone()
            .ok_or_else(|| DaemonError::invalid_state("deployment plan id is missing"))?;
        let key = DeploymentKey::from_id(&id);

        if let Some(active_key) = self.active_key_for_pipeline(&id) {
            if let Some(mut deployment) = self.deployments.remove(&active_key) {
                if can_reload_process_only(&deployment.plan, &plan) {
                    self.worker_launcher.reload(&deployment.worker, plan.clone())?;
                    deployment.id = id;
                    deployment.plan = plan;
                    self.deployments.insert(key.clone(), deployment);
                    return Ok(DaemonApplyResult {
                        id: self
                            .deployments
                            .get(&key)
                            .ok_or_else(|| {
                                DaemonError::invalid_state("deployment missing after reload")
                            })?
                            .id
                            .clone(),
                        mode: DaemonApplyMode::HotReload,
                    });
                }
                self.worker_launcher.stop(deployment.worker)?;
            }
        }

        let worker = self.worker_launcher.start(plan.clone())?;
        self.deployments
            .insert(key.clone(), ActiveDeployment { id, plan, worker });
        Ok(DaemonApplyResult {
            id: self
                .deployments
                .get(&key)
                .ok_or_else(|| DaemonError::invalid_state("deployment missing after apply"))?
                .id
                .clone(),
            mode: DaemonApplyMode::Started,
        })
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

    fn active_key_for_pipeline(&self, id: &ResourceId) -> Option<DeploymentKey> {
        self.deployments
            .keys()
            .find(|key| key.name == id.name)
            .cloned()
    }

    async fn get_pipeline_key(
        &self,
        pipeline_name: &str,
        pipeline_ver: Option<&str>,
    ) -> DaemonResult<ResourceId> {
        if pipeline_name.trim().is_empty() {
            return Err(DaemonError::invalid_state("pipeline name is missing"));
        }
        let Some(version) = pipeline_ver.filter(|version| !version.is_empty()) else {
            return self
                .store
                .load_latest_pipeline_id(pipeline_name)
                .await?
                .ok_or_else(|| DaemonError::not_found("pipeline resource not found"));
        };

        Ok(ResourceId {
            name: pipeline_name.to_string(),
            version: version.to_string(),
        })
    }

    async fn assign_pipeline_revision(
        &mut self,
        mut plan: DeploymentPlan,
    ) -> DaemonResult<DeploymentPlan> {
        let id = plan
            .id
            .take()
            .ok_or_else(|| DaemonError::invalid_state("pipeline id is missing"))?;
        let next_revision = self.next_pipeline_revision(&id.name).await?;
        plan.id = Some(ResourceId {
            name: id.name.clone(),
            version: next_revision,
        });
        Ok(plan)
    }

    async fn next_pipeline_revision(&self, pipeline_name: &str) -> DaemonResult<String> {
        if pipeline_name.trim().is_empty() {
            return Err(DaemonError::invalid_state("pipeline name is missing"));
        }
        let Some(latest_id) = self.store.load_latest_pipeline_id(pipeline_name).await? else {
            return Ok("r1".to_string());
        };
        let latest_revision = latest_id
            .version
            .strip_prefix('r')
            .and_then(|version| version.parse::<u64>().ok())
            .ok_or_else(|| {
                DaemonError::invalid_state(format!(
                    "invalid latest pipeline revision: {}",
                    latest_id.version
                ))
            })?;
        Ok(format!("r{}", latest_revision + 1))
    }
}

fn can_reload_process_only(current: &DeploymentPlan, target: &DeploymentPlan) -> bool {
    let Some(current_id) = current.id.as_ref() else {
        return false;
    };
    let Some(target_id) = target.id.as_ref() else {
        return false;
    };
    current_id.name == target_id.name
        && current.execution == target.execution
        && current.sources == target.sources
        && current.egress == target.egress
        && current.process != target.process
}
