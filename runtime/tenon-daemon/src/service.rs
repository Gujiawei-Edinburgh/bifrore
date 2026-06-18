use crate::{
    DaemonError, DaemonErrorKind, DaemonResult, DaemonStore, InMemoryDaemonStore, NoopWorkerLauncher,
    ResourceKey, TenonDaemon, WorkerLauncher,
};
use tenon_message::daemon::v1::{
    worker_envelope, ApplyMode, DeleteResourceRequest, DeleteResourceResponse, GetResourceRequest,
    GetResourceResponse, PutResourceRequest, PutResourceResponse, UpdateResourceRequest,
    UpdateResourceResponse, WorkerEnvelope,
};
use tenon_message::plan::{resource, Resource, ResourceId, ResourceKind, StoredDeploymentPlan};

pub struct DaemonService<L = NoopWorkerLauncher, S = InMemoryDaemonStore> {
    daemon: TenonDaemon<L, S>,
}

impl DaemonService<NoopWorkerLauncher, InMemoryDaemonStore> {
    pub fn new() -> Self {
        Self::with_daemon(TenonDaemon::new())
    }
}

impl Default for DaemonService<NoopWorkerLauncher, InMemoryDaemonStore> {
    fn default() -> Self {
        Self::new()
    }
}

impl<L, S> DaemonService<L, S>
where
    L: WorkerLauncher,
    S: DaemonStore,
{
    pub fn with_daemon(daemon: TenonDaemon<L, S>) -> Self {
        Self { daemon }
    }

    pub fn daemon(&self) -> &TenonDaemon<L, S> {
        &self.daemon
    }

    pub fn daemon_mut(&mut self) -> &mut TenonDaemon<L, S> {
        &mut self.daemon
    }

    pub async fn handle_put_resource(
        &mut self,
        request: PutResourceRequest,
    ) -> PutResourceResponse {
        let Some(resource) = request.resource else {
            return put_rejected(ApplyMode::RejectedWorkerError, "resource is missing");
        };

        match resource.kind {
            Some(resource::Kind::Pipeline(pipeline)) => {
                match self.put_pipeline_resource(pipeline).await {
                    Ok(active_id) => PutResourceResponse {
                        accepted: true,
                        id: Some(active_id),
                        mode: ApplyMode::Started as i32,
                        message: String::new(),
                    },
                    Err(error) => put_rejected(apply_error_mode(&error), error.message),
                }
            },
            Some(_) => {
                let id = resource_id_from_resource(&resource);
                match self.daemon.store.save_resource(resource).await {
                    Ok(()) => PutResourceResponse {
                        accepted: true,
                        id,
                        mode: ApplyMode::Unspecified as i32,
                        message: String::new(),
                    },
                    Err(error) => put_rejected(apply_error_mode(&error), error.message),
                }
            }
            None => put_rejected(ApplyMode::RejectedWorkerError, "resource payload is missing"),
        }
    }

    pub async fn handle_get_resource(
        &self,
        request: GetResourceRequest,
    ) -> GetResourceResponse {
        let Some(id) = request.id else {
            return GetResourceResponse {
                found: false,
                resource: None,
                message: "resource id is missing".to_string(),
            };
        };
        let key = ResourceKey::from_id(&id);

        match self.daemon.store.load_resource(&key).await {
            Ok(Some(resource)) => GetResourceResponse {
                found: true,
                resource: Some(resource),
                message: String::new(),
            },
            Ok(None) => GetResourceResponse {
                found: false,
                resource: None,
                message: "resource not found".to_string(),
            },
            Err(error) => GetResourceResponse {
                found: false,
                resource: None,
                message: error.message,
            },
        }
    }

    pub async fn handle_update_resource(
        &mut self,
        request: UpdateResourceRequest,
    ) -> UpdateResourceResponse {
        let Some(previous_id) = request.previous_id else {
            return update_rejected("previous resource id is missing");
        };
        let previous_key = ResourceKey::from_id(&previous_id);
        let affected_pipelines = match self.daemon.store.load_referencing_plans(&previous_key).await
        {
            Ok(pipelines) => pipelines,
            Err(error) => return update_rejected(error.message),
        };

        let Some(resource) = request.resource else {
            return update_rejected("updated resource is missing");
        };
        let result = match resource.kind {
            Some(resource::Kind::MqttSource(_))
            | Some(resource::Kind::Process(_))
            | Some(resource::Kind::Egress(_))
            | Some(resource::Kind::Pipeline(_)) => self.daemon.store.save_resource(resource).await,
            None => return update_rejected("updated resource is missing"),
        };

        match result {
            Ok(()) => UpdateResourceResponse {
                accepted: true,
                affected_pipelines,
                message: String::new(),
            },
            Err(error) => update_rejected(error.message),
        }
    }

    pub async fn handle_delete_resource(
        &mut self,
        request: DeleteResourceRequest,
    ) -> DeleteResourceResponse {
        let Some(id) = request.id else {
            return delete_rejected("resource id is missing");
        };
        let key = ResourceKey::from_id(&id);

        if ResourceKind::try_from(id.kind) == Ok(ResourceKind::Pipeline) {
            let _ = self.daemon.stop_worker_by_key(&key);
            return match self.daemon.store.delete_resource(&key).await {
                Ok(deleted) => DeleteResourceResponse {
                    deleted,
                    message: if deleted {
                        String::new()
                    } else {
                        "resource not found".to_string()
                    },
                },
                Err(error) => delete_rejected(error.message),
            };
        }

        let references = match self.daemon.store.load_referencing_plans(&key).await {
            Ok(references) => references,
            Err(error) => return delete_rejected(error.message),
        };
        if !references.is_empty() {
            return delete_rejected("resource is still referenced by pipelines");
        }

        match self.daemon.store.delete_resource(&key).await {
            Ok(deleted) => DeleteResourceResponse {
                deleted,
                message: if deleted {
                    String::new()
                } else {
                    "resource not found".to_string()
                },
            },
            Err(error) => delete_rejected(error.message),
        }
    }

    pub fn handle_worker_envelope(&mut self, envelope: WorkerEnvelope) -> DaemonResult<()> {
        match envelope.payload {
            Some(worker_envelope::Payload::Heartbeat(_)) => Ok(()),
            Some(worker_envelope::Payload::StartWorker(_)) => Err(DaemonError::invalid_state(
                "worker must not send StartWorkerRequest to daemon",
            )),
            None => Err(DaemonError::invalid_state("worker envelope payload is missing")),
        }
    }

    async fn put_pipeline_resource(
        &mut self,
        pipeline: StoredDeploymentPlan,
    ) -> DaemonResult<ResourceId> {
        self.daemon.store.save_resource(Resource {
            kind: Some(resource::Kind::Pipeline(pipeline.clone())),
        }).await?;
        let id = pipeline
            .id
            .clone()
            .ok_or_else(|| DaemonError::invalid_state("pipeline resource id is missing"))?;
        let plan = self
            .daemon
            .load_plan(&id)
            .await?
            .ok_or_else(|| DaemonError::invalid_state("pipeline resource was not stored"))?;
        self.daemon.apply_plan(plan).await?;
        Ok(id)
    }
}

fn put_rejected(mode: ApplyMode, message: impl Into<String>) -> PutResourceResponse {
    PutResourceResponse {
        accepted: false,
        id: None,
        mode: mode as i32,
        message: message.into(),
    }
}

fn update_rejected(message: impl Into<String>) -> UpdateResourceResponse {
    UpdateResourceResponse {
        accepted: false,
        affected_pipelines: Vec::new(),
        message: message.into(),
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

fn delete_rejected(message: impl Into<String>) -> DeleteResourceResponse {
    DeleteResourceResponse {
        deleted: false,
        message: message.into(),
    }
}

fn apply_error_mode(error: &DaemonError) -> ApplyMode {
    match error.kind {
        DaemonErrorKind::Worker => ApplyMode::RejectedWorkerError,
        DaemonErrorKind::Store | DaemonErrorKind::InvalidState | DaemonErrorKind::NotFound => {
            ApplyMode::RejectedWorkerError
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use tenon_message::daemon::v1::Heartbeat;
    use tenon_message::plan::{
        resource, DeliveryMode, DeploymentPlan, EgressPlan, ExecutionMode, MqttBrokerPlan,
        MqttSourcePlan, MqttSubscriptionPlan, PayloadDecodePlan, ProcessPlan, Resource,
        ResourceId, ResourceKind, ScriptRuntime, StoredDeploymentPlan,
    };

    #[test]
    fn puts_pipeline_resource_and_starts_worker() {
        block_on(async {
            let mut service = DaemonService::new();
            let plan = plan();
            put_plan_components(&mut service, &plan).await;
            let pipeline = stored_pipeline(&plan);

            let response = service
                .handle_put_resource(PutResourceRequest {
                    resource: Some(Resource {
                        kind: Some(resource::Kind::Pipeline(pipeline)),
                    }),
                })
                .await;

            assert!(response.accepted);
            assert_eq!(ApplyMode::try_from(response.mode), Ok(ApplyMode::Started));
            assert_eq!(
                response.id,
                Some(id(ResourceKind::Pipeline, "sensor-pipeline", "v1"))
            );
        });
    }

    #[test]
    fn rejects_put_without_resource() {
        block_on(async {
            let mut service = DaemonService::new();

            let response = service
                .handle_put_resource(PutResourceRequest { resource: None })
                .await;

            assert!(!response.accepted);
            assert_eq!(
                ApplyMode::try_from(response.mode),
                Ok(ApplyMode::RejectedWorkerError)
            );
            assert_eq!(response.message, "resource is missing");
        });
    }

    #[test]
    fn gets_pipeline_resource_after_apply() {
        block_on(async {
            let mut service = DaemonService::new();
            let plan = plan();
            let expected_pipeline = stored_pipeline(&plan);
            put_plan_components(&mut service, &plan).await;
            service
                .handle_put_resource(PutResourceRequest {
                    resource: Some(Resource {
                        kind: Some(resource::Kind::Pipeline(expected_pipeline.clone())),
                    }),
                })
                .await;

            let response = service
                .handle_get_resource(GetResourceRequest {
                    id: expected_pipeline.id.clone(),
                })
                .await;

            assert!(response.found);
            assert_eq!(
                response.resource,
                Some(Resource {
                    kind: Some(resource::Kind::Pipeline(expected_pipeline)),
                })
            );
        });
    }

    #[test]
    fn edits_resource_and_reports_affected_pipelines() {
        block_on(async {
            let mut service = DaemonService::new();
            let plan = plan();
            let previous_process_id = plan
                .process
                .as_ref()
                .and_then(|process| process.id.clone())
                .expect("process id");
            put_full_plan(&mut service, &plan).await;

            let mut new_process = plan.process.clone().expect("process");
            new_process.id = Some(id(ResourceKind::Process, "sensor-process", "v2"));
            let response = service
                .handle_update_resource(UpdateResourceRequest {
                    previous_id: Some(previous_process_id),
                    resource: Some(Resource {
                        kind: Some(resource::Kind::Process(new_process)),
                    }),
                })
                .await;

            assert!(response.accepted);
            assert_eq!(response.affected_pipelines, vec![plan.id.expect("plan id")]);
        });
    }

    #[test]
    fn rejects_delete_for_referenced_resource() {
        block_on(async {
            let mut service = DaemonService::new();
            let plan = plan();
            let source_id = plan.sources[0].id.clone();
            put_full_plan(&mut service, &plan).await;

            let response = service
                .handle_delete_resource(DeleteResourceRequest { id: source_id })
                .await;

            assert!(!response.deleted);
            assert_eq!(response.message, "resource is still referenced by pipelines");
        });
    }

    #[test]
    fn deletes_pipeline_and_clears_reverse_refs() {
        block_on(async {
            let mut service = DaemonService::new();
            let plan = plan();
            let plan_id = plan.id.clone();
            let source_id = plan.sources[0].id.clone().expect("source id");
            put_full_plan(&mut service, &plan).await;

            let response = service
                .handle_delete_resource(DeleteResourceRequest { id: plan_id })
                .await;

            assert!(response.deleted);
            assert!(
                service
                    .daemon
                    .store
                    .load_referencing_plans(&ResourceKey::from_id(&source_id))
                    .await
                    .expect("load refs")
                    .is_empty()
            );
        });
    }

    #[test]
    fn accepts_worker_heartbeat() {
        let mut service = DaemonService::new();
        let envelope = WorkerEnvelope {
            payload: Some(worker_envelope::Payload::Heartbeat(Heartbeat {
                timestamp_millis: 10,
            })),
        };

        service
            .handle_worker_envelope(envelope)
            .expect("heartbeat");
    }

    #[test]
    fn rejects_worker_start_request() {
        let mut service = DaemonService::new();
        let envelope = WorkerEnvelope {
            payload: Some(worker_envelope::Payload::StartWorker(
                tenon_message::daemon::v1::StartWorkerRequest { plan: Some(plan()) },
            )),
        };

        let error = service
            .handle_worker_envelope(envelope)
            .expect_err("worker start request should be rejected");

        assert_eq!(error.kind, DaemonErrorKind::InvalidState);
    }

    fn plan() -> DeploymentPlan {
        DeploymentPlan {
            id: Some(id(ResourceKind::Pipeline, "sensor-pipeline", "v1")),
            execution: ExecutionMode::IntraProc as i32,
            sources: vec![MqttSourcePlan {
                id: Some(id(ResourceKind::MqttSource, "sensor-source", "v1")),
                broker: Some(MqttBrokerPlan {
                    host: "127.0.0.1".to_string(),
                    port: 1883,
                }),
                auth: None,
                subscriptions: vec![MqttSubscriptionPlan {
                    topic: "sensor/+/data".to_string(),
                    decode: PayloadDecodePlan::Json as i32,
                }],
                client_count: 1,
            }],
            process: Some(ProcessPlan {
                id: Some(id(ResourceKind::Process, "sensor-process", "v1")),
                runtime: ScriptRuntime::Lua as i32,
                source: "function on_message(ctx, msg) end".to_string(),
            }),
            egress: Some(EgressPlan {
                id: Some(id(ResourceKind::Egress, "sensor-egress", "v1")),
                delivery: DeliveryMode::Single as i32,
            }),
        }
    }

    fn id(kind: ResourceKind, name: &str, version: &str) -> ResourceId {
        ResourceId {
            kind: kind as i32,
            name: name.to_string(),
            version: version.to_string(),
        }
    }

    async fn put_full_plan(service: &mut DaemonService, plan: &DeploymentPlan) {
        put_plan_components(service, plan).await;
        service
            .handle_put_resource(PutResourceRequest {
                resource: Some(Resource {
                    kind: Some(resource::Kind::Pipeline(stored_pipeline(plan))),
                }),
            })
            .await;
    }

    async fn put_plan_components(service: &mut DaemonService, plan: &DeploymentPlan) {
        for source in &plan.sources {
            service
                .handle_put_resource(PutResourceRequest {
                    resource: Some(Resource {
                        kind: Some(resource::Kind::MqttSource(source.clone())),
                    }),
                })
                .await;
        }
        service
            .handle_put_resource(PutResourceRequest {
                resource: Some(Resource {
                    kind: Some(resource::Kind::Process(
                        plan.process.clone().expect("process"),
                    )),
                }),
            })
            .await;
        service
            .handle_put_resource(PutResourceRequest {
                resource: Some(Resource {
                    kind: Some(resource::Kind::Egress(plan.egress.clone().expect("egress"))),
                }),
            })
            .await;
    }

    fn stored_pipeline(plan: &DeploymentPlan) -> StoredDeploymentPlan {
        StoredDeploymentPlan {
            id: plan.id.clone(),
            execution: plan.execution,
            source_refs: plan
                .sources
                .iter()
                .map(|source| source.id.clone().expect("source id"))
                .collect(),
            process_ref: plan
                .process
                .as_ref()
                .and_then(|process| process.id.clone()),
            egress_ref: plan.egress.as_ref().and_then(|egress| egress.id.clone()),
        }
    }
}
