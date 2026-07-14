mod client;
mod service;
mod store;
mod worker;

pub use client::{
    DaemonClient, DaemonServer, DaemonTransportProvider, InProcDaemonClient, InProcDaemonConfig,
    InProcDaemonServer, InProcDaemonTransportProvider,
};
pub use service::DaemonService;
pub use store::{DaemonStore, InMemoryDaemonStore};
pub use worker::{
    NoopWorkerManager, UdsWorkerManager, UdsWorkerManagerConfig, WorkerDeployment, WorkerHandle,
    WorkerManager, WorkerStatus,
};

use std::collections::HashMap;
use tenon_message::daemon::v1::WorkerStats;
use tenon_message::plan::{DeploymentPlan, MqttSourceClientIds, ResourceId};

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

pub type DeploymentKey = ResourceId;

pub struct TenonDaemon<M, P> {
    worker_manager: M,
    store: P,
    deployments: HashMap<DeploymentKey, ActiveDeployment>,
}

impl<M, P> TenonDaemon<M, P>
where
    M: WorkerManager,
    P: DaemonStore,
{
    pub fn with_components(worker_manager: M, store: P) -> Self {
        Self {
            worker_manager,
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
        validate_put_pipeline(&pipeline)?;
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
        let key = id.clone();

        if let Some(active_key) = self.active_key_for_pipeline(&id) {
            if let Some(mut deployment) = self.deployments.remove(&active_key) {
                if can_reload_process_only(&deployment.plan, &plan) {
                    self.worker_manager.reload(&deployment.worker, plan.clone())?;
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
                self.worker_manager.stop(deployment.worker)?;
            }
        }

        let worker_deployment = self.worker_deployment(&plan).await?;
        let worker = self.worker_manager.start(worker_deployment)?;
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
            self.worker_manager.stop(deployment.worker)?;
        }
        Ok(())
    }

    pub fn worker_status(&mut self, id: &ResourceId) -> DaemonResult<WorkerStatus> {
        let deployment = self
            .deployments
            .get(id)
            .ok_or_else(|| DaemonError::not_found("deployment worker not found"))?;
        self.worker_manager.status(&deployment.worker)
    }

    pub fn worker_stats(&mut self, id: &ResourceId) -> DaemonResult<WorkerStats> {
        let worker = self
            .deployments
            .get(id)
            .ok_or_else(|| DaemonError::not_found("deployment worker not found"))?
            .worker
            .clone();
        self.worker_manager.stats(&worker)
    }

    pub fn all_worker_stats(&mut self) -> DaemonResult<Vec<(ResourceId, WorkerStats)>> {
        let workers: Vec<_> = self
            .deployments
            .values()
            .map(|deployment| (deployment.id.clone(), deployment.worker.clone()))
            .collect();
        workers
            .into_iter()
            .map(|(id, worker)| self.worker_manager.stats(&worker).map(|stats| (id, stats)))
            .collect()
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

    async fn worker_deployment(&mut self, plan: &DeploymentPlan) -> DaemonResult<WorkerDeployment> {
        let pipeline_id = plan
            .id
            .as_ref()
            .ok_or_else(|| DaemonError::invalid_state("deployment plan id is missing"))?;
        let mut source_client_ids = Vec::with_capacity(plan.sources.len());

        for (source_index, source) in plan.sources.iter().enumerate() {
            let source_id = mqtt_source_client_ids_key(pipeline_id, source_index)?;
            let mut client_ids = self.store.load_mqtt_client_ids(&source_id).await?;
            let required = usize::try_from(source.client_count.max(1)).map_err(|_| {
                DaemonError::invalid_state("MQTT source client count is too large")
            })?;

            if client_ids.len() < required {
                for index in client_ids.len()..required {
                    client_ids.push(mqtt_client_id(pipeline_id, source_index, index));
                }
                self.store
                    .save_mqtt_client_ids(&source_id, client_ids.clone())
                    .await?;
            } else if client_ids.len() > required {
                client_ids.truncate(required);
                self.store
                    .save_mqtt_client_ids(&source_id, client_ids.clone())
                    .await?;
            }

            source_client_ids.push(MqttSourceClientIds {
                source_index: u32::try_from(source_index).map_err(|_| {
                    DaemonError::invalid_state("MQTT source index is too large")
                })?,
                client_ids,
            });
        }

        Ok(WorkerDeployment {
            plan: plan.clone(),
            source_client_ids,
        })
    }
}

fn validate_put_pipeline(plan: &DeploymentPlan) -> DaemonResult<()> {
    let process = plan
        .process
        .as_ref()
        .ok_or_else(|| DaemonError::invalid_state("process plan is missing"))?;
    if process.access_plan.is_none() {
        return Err(DaemonError::invalid_state("process access_plan is missing"));
    }
    Ok(())
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

fn mqtt_source_client_ids_key(
    pipeline_id: &ResourceId,
    source_index: usize,
) -> DaemonResult<ResourceId> {
    if pipeline_id.name.trim().is_empty() {
        return Err(DaemonError::invalid_state("pipeline name is missing"));
    }
    if pipeline_id.version.trim().is_empty() {
        return Err(DaemonError::invalid_state("pipeline version is missing"));
    }
    Ok(ResourceId {
        name: format!("{}:source:{source_index}", pipeline_id.name),
        version: "client-ids".to_string(),
    })
}

fn mqtt_client_id(pipeline_id: &ResourceId, source_index: usize, client_index: usize) -> String {
    format!("{}-{}-{}", pipeline_id.name, source_index, client_index)
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use tenon_message::plan::{
        EgressPlan, ExecutionMode, MqttBrokerPlan, MqttSourcePlan,
        MqttSubscriptionPlan, PayloadDecodePlan, ProcessPlan, ScriptRuntime, MessageAccessPlan,
    };

    #[test]
    fn apply_generates_client_ids_for_worker_start() {
        block_on(async {
            let mut daemon = daemon();
            daemon.put_pipeline(plan("a", 2)).await.expect("put");

            daemon
                .apply_pipeline("sensor-pipeline", None)
                .await
                .expect("apply");

            assert_eq!(daemon.worker_manager.starts.len(), 1);
            assert_eq!(
                daemon.worker_manager.starts[0].source_client_ids[0].client_ids,
                vec![
                    "sensor-pipeline-0-0".to_string(),
                    "sensor-pipeline-0-1".to_string()
                ]
            );
        });
    }

    #[test]
    fn process_only_revision_reuses_worker_for_reload() {
        block_on(async {
            let mut daemon = daemon();
            daemon.put_pipeline(plan("a", 2)).await.expect("put r1");
            daemon
                .apply_pipeline("sensor-pipeline", None)
                .await
                .expect("apply r1");

            daemon.put_pipeline(plan("b", 2)).await.expect("put r2");
            daemon
                .apply_pipeline("sensor-pipeline", None)
                .await
                .expect("apply r2");

            assert_eq!(daemon.worker_manager.starts.len(), 1);
            assert_eq!(daemon.worker_manager.reloads.len(), 1);
            assert_eq!(
                daemon.worker_manager.reloads[0]
                    .process
                    .as_ref()
                    .expect("process")
                    .source,
                "b"
            );
        });
    }

    #[derive(Default)]
    struct RecordingWorkerManager {
        starts: Vec<WorkerDeployment>,
        reloads: Vec<DeploymentPlan>,
        next_id: u64,
    }

    impl WorkerManager for RecordingWorkerManager {
        fn start(&mut self, deployment: WorkerDeployment) -> DaemonResult<WorkerHandle> {
            self.starts.push(deployment);
            self.next_id += 1;
            Ok(WorkerHandle {
                id: format!("worker-{}", self.next_id),
            })
        }

        fn reload(
            &mut self,
            _worker: &WorkerHandle,
            plan: DeploymentPlan,
        ) -> DaemonResult<()> {
            self.reloads.push(plan);
            Ok(())
        }

        fn stop(&mut self, _worker: WorkerHandle) -> DaemonResult<()> {
            Ok(())
        }

        fn status(&mut self, _worker: &WorkerHandle) -> DaemonResult<WorkerStatus> {
            Ok(WorkerStatus::Running)
        }

        fn stats(&mut self, _worker: &WorkerHandle) -> DaemonResult<WorkerStats> {
            Ok(WorkerStats::default())
        }
    }

    fn daemon() -> TenonDaemon<RecordingWorkerManager, InMemoryDaemonStore> {
        TenonDaemon::with_components(
            RecordingWorkerManager::default(),
            InMemoryDaemonStore::default(),
        )
    }

    fn plan(process_source: &str, client_count: u32) -> DeploymentPlan {
        DeploymentPlan {
            id: Some(ResourceId {
                name: "sensor-pipeline".to_string(),
                version: String::new(),
            }),
            execution: ExecutionMode::IntraProc as i32,
            sources: vec![MqttSourcePlan {
                broker: Some(MqttBrokerPlan {
                    host: "127.0.0.1".to_string(),
                    port: 1883,
                }),
                auth: None,
                subscriptions: vec![MqttSubscriptionPlan {
                    topic: "sensor/+/data".to_string(),
                    decode: PayloadDecodePlan::Json as i32,
                }],
                client_count,
            }],
            process: Some(ProcessPlan {
                runtime: ScriptRuntime::Lua as i32,
                source: process_source.to_string(),
                access_plan: Some(MessageAccessPlan::default()),
            }),
            egress: Some(EgressPlan {}),
        }
    }
}
