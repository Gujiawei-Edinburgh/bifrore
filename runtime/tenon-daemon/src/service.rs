use crate::{
    DaemonError, DaemonErrorKind, DaemonResult, DaemonStore, InMemoryDaemonStore, NoopWorkerLauncher,
    TenonDaemon, WorkerLauncher,
};
use tenon_message::daemon::v1::{
    worker_envelope, ApplyMode, DeleteResourceRequest, DeleteResourceResponse, GetResourceRequest,
    GetResourceResponse, PutResourceRequest, PutResourceResponse, UpdateResourceRequest,
    UpdateResourceResponse, WorkerEnvelope,
};

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

        let mode = match resource.kind {
            Some(tenon_message::plan::resource::Kind::Pipeline(_)) => ApplyMode::Started,
            Some(_) => ApplyMode::Unspecified,
            None => {
                return put_rejected(ApplyMode::RejectedWorkerError, "resource payload is missing");
            }
        };

        match self.daemon.put_resource(resource).await {
            Ok(id) => PutResourceResponse {
                accepted: true,
                id: Some(id),
                mode: mode as i32,
                message: String::new(),
            },
            Err(error) => put_rejected(apply_error_mode(&error), error.message),
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
        match self.daemon.get_resource(&id).await {
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
        let Some(resource) = request.resource else {
            return update_rejected("updated resource is missing");
        };
        if resource.kind.is_none() {
            return update_rejected("updated resource is missing");
        }

        match self.daemon.update_resource(&previous_id, resource).await {
            Ok(affected_pipelines) => UpdateResourceResponse {
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
        match self.daemon.delete_resource(&id).await {
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
    use crate::ResourceKey;
    use futures::executor::block_on;
    use tenon_message::daemon::v1::Heartbeat;
    use tenon_message::plan::{
        resource, DeliveryMode, DeploymentPlan, EgressPlan, ExecutionMode, MqttBrokerPlan,
        MqttSourcePlan, MqttSubscriptionPlan, PayloadDecodePlan, ProcessPlan, Resource,
        ResourceId, ResourceKind, ScriptRuntime,
    };

    #[test]
    fn puts_pipeline_resource_and_starts_worker() {
        block_on(async {
            let mut service = DaemonService::new();
            let plan = plan();

            let response = service
                .handle_put_resource(PutResourceRequest {
                    resource: Some(Resource {
                        kind: Some(resource::Kind::Pipeline(plan.clone())),
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
            service
                .handle_put_resource(PutResourceRequest {
                    resource: Some(Resource {
                        kind: Some(resource::Kind::Pipeline(plan.clone())),
                    }),
                })
                .await;

            let response = service
                .handle_get_resource(GetResourceRequest {
                    id: plan.id.clone(),
                })
                .await;

            assert!(response.found);
            assert_eq!(
                response.resource,
                Some(Resource {
                    kind: Some(resource::Kind::Pipeline(plan)),
                })
            );
        });
    }

    #[test]
    fn rejects_duplicate_pipeline_put() {
        block_on(async {
            let mut service = DaemonService::new();
            let plan = plan();

            let first = service
                .handle_put_resource(PutResourceRequest {
                    resource: Some(Resource {
                        kind: Some(resource::Kind::Pipeline(plan.clone())),
                    }),
                })
                .await;
            let duplicate = service
                .handle_put_resource(PutResourceRequest {
                    resource: Some(Resource {
                        kind: Some(resource::Kind::Pipeline(plan)),
                    }),
                })
                .await;

            assert!(first.accepted);
            assert!(!duplicate.accepted);
            assert!(duplicate.message.contains("already exists"));
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
                    kind: Some(resource::Kind::Pipeline(plan.clone())),
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

}
