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
    pub resource_ids: Vec<ResourceId>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DaemonReviseResult {
    pub revised_pipeline_id: ResourceId,
    pub revised_resource_id: ResourceId,
    pub resource_ids: Vec<ResourceId>,
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

    pub async fn put_pipeline(
        &mut self,
        pipeline: DeploymentPlan,
    ) -> DaemonResult<DaemonPutPipelineResult> {
        let resource = self
            .assign_pipeline_revisions(pipeline)
            .await
            .map(|pipeline| Resource {
                kind: Some(resource::Kind::Pipeline(pipeline)),
            })?;
        let id = resource_id_from_resource(&resource)
            .ok_or_else(|| DaemonError::invalid_state("pipeline id is missing"))?;
        let resource_ids = resource_ids_from_resource(&resource);

        self.store.save_resource(resource).await?;
        Ok(DaemonPutPipelineResult { id, resource_ids })
    }

    pub async fn get_resource(&self, id: &ResourceId) -> DaemonResult<Option<Resource>> {
        let key = ResourceKey::from_id(id);
        self.store.load_resource(&key).await
    }

    pub async fn revise_resource(
        &mut self,
        pipeline_id: &ResourceId,
        previous_id: &ResourceId,
        resource: Resource,
    ) -> DaemonResult<DaemonReviseResult> {
        if ResourceKind::try_from(previous_id.kind) != Ok(ResourceKind::Process) {
            return Err(DaemonError::invalid_state(
                "revise currently accepts process resources only",
            ));
        }

        let resource = self.assign_process_revisions(resource).await?;
        let process = match resource.kind {
            Some(resource::Kind::Process(process)) => process,
            Some(_) => {
                return Err(DaemonError::invalid_state(
                    "revised resource must be a process",
                ));
            }
            None => return Err(DaemonError::invalid_state("revised resource is missing")),
        };
        let process_id = process
            .id
            .clone()
            .ok_or_else(|| DaemonError::invalid_state("revised process id is missing"))?;
        if ResourceKind::try_from(process_id.kind) != Ok(ResourceKind::Process) {
            return Err(DaemonError::invalid_state(
                "revised process id must use Process kind",
            ));
        }
        let pipeline = self
            .get_pipeline_resource(pipeline_id)
            .await?
            .ok_or_else(|| DaemonError::not_found("pipeline resource not found"))?;
        let mut plan = match pipeline.kind {
            Some(resource::Kind::Pipeline(plan)) => plan,
            _ => return Err(DaemonError::invalid_state("resource is not a pipeline")),
        };
        let current_process_id = plan
            .process
            .as_ref()
            .and_then(|process| process.id.as_ref())
            .ok_or_else(|| DaemonError::invalid_state("pipeline process id is missing"))?;
        if current_process_id != previous_id {
            return Err(DaemonError::invalid_state(
                "pipeline does not reference previous resource id",
            ));
        }

        let revised_pipeline_id = self
            .allocate_resource_id(ResourceKind::Pipeline, &pipeline_id.name)
            .await?;

        plan.id = Some(revised_pipeline_id.clone());
        plan.process = Some(process);
        self.store
            .save_resource(Resource {
                kind: Some(resource::Kind::Pipeline(plan)),
            })
            .await?;
        Ok(DaemonReviseResult {
            revised_pipeline_id: revised_pipeline_id.clone(),
            revised_resource_id: process_id.clone(),
            resource_ids: vec![revised_pipeline_id, process_id],
        })
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

    pub async fn apply_pipeline(
        &mut self,
        pipeline_ver: Option<&str>,
    ) -> DaemonResult<DaemonApplyResult> {
        let id = self.apply_pipeline_id(pipeline_ver).await?;
        let resource = self
            .get_pipeline_resource(&id)
            .await?
            .ok_or_else(|| DaemonError::not_found("pipeline resource not found"))?;
        match resource.kind {
            Some(resource::Kind::Pipeline(plan)) => self.apply(plan).await,
            _ => Err(DaemonError::invalid_state("resource is not a pipeline")),
        }
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

    pub async fn reload(&mut self, plan: DeploymentPlan) -> DaemonResult<DaemonApplyResult> {
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

    fn active_key_for_pipeline(&self, id: &ResourceId) -> Option<DeploymentKey> {
        self.deployments
            .keys()
            .find(|key| key.kind == id.kind && key.name == id.name)
            .cloned()
    }

    async fn apply_pipeline_id(&self, pipeline_ver: Option<&str>) -> DaemonResult<ResourceId> {
        let latest_id = match self.store.load_latest_pipeline_id().await? {
            Some(id) => id,
            None => self
                .deployments
                .values()
                .next()
                .map(|deployment| deployment.id.clone())
                .ok_or_else(|| DaemonError::not_found("pipeline resource not found"))?,
        };

        let Some(version) = pipeline_ver.filter(|version| !version.is_empty()) else {
            return Ok(latest_id);
        };

        Ok(ResourceId {
            kind: ResourceKind::Pipeline as i32,
            name: latest_id.name,
            version: version.to_string(),
        })
    }

    async fn get_pipeline_resource(&self, id: &ResourceId) -> DaemonResult<Option<Resource>> {
        if ResourceKind::try_from(id.kind) != Ok(ResourceKind::Pipeline) {
            return Err(DaemonError::invalid_state("resource is not a pipeline"));
        }
        if id.version.is_empty() {
            return self
                .store
                .load_latest_resource(ResourceKind::Pipeline, &id.name)
                .await;
        }
        self.get_resource(id).await
    }

    async fn assign_process_revisions(&mut self, resource: Resource) -> DaemonResult<Resource> {
        match resource.kind {
            Some(resource::Kind::Process(mut process)) => {
                process.id = Some(self.assign_id(process.id, ResourceKind::Process).await?);
                Ok(Resource {
                    kind: Some(resource::Kind::Process(process)),
                })
            }
            Some(_) => {
                Err(DaemonError::invalid_state("resource requires process only"))
            }
            None => Err(DaemonError::invalid_state("resource payload is missing")),
        }
    }

    async fn assign_pipeline_revisions(
        &mut self,
        mut plan: DeploymentPlan,
    ) -> DaemonResult<DeploymentPlan> {
        plan.id = Some(self.assign_id(plan.id, ResourceKind::Pipeline).await?);
        for source in &mut plan.sources {
            source.id = Some(self.assign_id(source.id.clone(), ResourceKind::MqttSource).await?);
        }
        if let Some(process) = &mut plan.process {
            process.id = Some(self.assign_id(process.id.clone(), ResourceKind::Process).await?);
        }
        if let Some(egress) = &mut plan.egress {
            egress.id = Some(self.assign_id(egress.id.clone(), ResourceKind::Egress).await?);
        }
        Ok(plan)
    }

    async fn assign_id(
        &mut self,
        id: Option<ResourceId>,
        kind: ResourceKind,
    ) -> DaemonResult<ResourceId> {
        let id = id.ok_or_else(|| {
            DaemonError::invalid_state(format!("{kind} resource id is missing"))
        })?;
        self.allocate_resource_id(kind, &id.name).await
    }

    async fn allocate_resource_id(
        &mut self,
        kind: ResourceKind,
        name: &str,
    ) -> DaemonResult<ResourceId> {
        Ok(ResourceId {
            kind: kind as i32,
            name: name.to_string(),
            version: self.store.allocate_revision(kind, name).await?,
        })
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

fn resource_ids_from_resource(resource: &Resource) -> Vec<ResourceId> {
    match &resource.kind {
        Some(resource::Kind::Pipeline(pipeline)) => {
            let mut ids = Vec::new();
            if let Some(id) = &pipeline.id {
                ids.push(id.clone());
            }
            ids.extend(
                pipeline
                    .sources
                    .iter()
                    .filter_map(|source| source.id.clone()),
            );
            if let Some(process) = &pipeline.process {
                if let Some(id) = &process.id {
                    ids.push(id.clone());
                }
            }
            if let Some(egress) = &pipeline.egress {
                if let Some(id) = &egress.id {
                    ids.push(id.clone());
                }
            }
            ids
        }
        _ => resource_id_from_resource(resource).into_iter().collect(),
    }
}

fn can_reload_process_only(current: &DeploymentPlan, target: &DeploymentPlan) -> bool {
    let Some(current_id) = current.id.as_ref() else {
        return false;
    };
    let Some(target_id) = target.id.as_ref() else {
        return false;
    };
    current_id.kind == target_id.kind
        && current_id.name == target_id.name
        && current.execution == target.execution
        && current.sources == target.sources
        && current.egress == target.egress
        && current.process != target.process
}
