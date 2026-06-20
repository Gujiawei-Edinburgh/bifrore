use crate::{
    DaemonApplyMode, DaemonError, DaemonErrorKind, DaemonResult, DaemonStore,
    InMemoryDaemonStore, NoopWorkerLauncher, TenonDaemon, WorkerLauncher,
};
use tenon_message::daemon::v1::{
    worker_envelope, ApplyMode, ApplyResourceRequest, ApplyResourceResponse, DeleteResourceRequest,
    DeleteResourceResponse, GetResourceRequest, GetResourceResponse, PutPipelineRequest,
    PutPipelineResponse, ReviseResourceRequest, ReviseResourceResponse, WorkerEnvelope,
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

    pub async fn handle_put_pipeline(
        &mut self,
        request: PutPipelineRequest,
    ) -> PutPipelineResponse {
        let Some(pipeline) = request.pipeline else {
            return put_rejected(ApplyMode::RejectedWorkerError, "pipeline is missing");
        };

        match self.daemon.put_pipeline(pipeline).await {
            Ok(result) => PutPipelineResponse {
                accepted: true,
                id: Some(result.id),
                mode: ApplyMode::Unspecified as i32,
                message: String::new(),
                resource_ids: result.resource_ids,
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

    pub async fn handle_apply_resource(
        &mut self,
        request: ApplyResourceRequest,
    ) -> ApplyResourceResponse {
        let Some(pipeline_id) = request.pipeline_id else {
            return apply_rejected("pipeline id is missing");
        };

        match self.daemon.apply_resource(&pipeline_id).await {
            Ok(result) => ApplyResourceResponse {
                accepted: true,
                active_pipeline_id: Some(result.id),
                mode: apply_mode_from_daemon(result.mode) as i32,
                message: String::new(),
            },
            Err(error) => ApplyResourceResponse {
                accepted: false,
                active_pipeline_id: None,
                mode: apply_error_mode(&error) as i32,
                message: error.message,
            },
        }
    }

    pub async fn handle_revise_resource(
        &mut self,
        request: ReviseResourceRequest,
    ) -> ReviseResourceResponse {
        let Some(pipeline_id) = request.pipeline_id else {
            return revise_rejected("pipeline id is missing");
        };
        let Some(previous_id) = request.previous_resource_id else {
            return revise_rejected("previous resource id is missing");
        };
        let Some(resource) = request.new_resource else {
            return revise_rejected("revised resource is missing");
        };
        if resource.kind.is_none() {
            return revise_rejected("revised resource is missing");
        }

        match self
            .daemon
            .revise_resource(&pipeline_id, &previous_id, resource)
            .await
        {
            Ok(result) => ReviseResourceResponse {
                accepted: true,
                revised_pipeline_id: Some(result.revised_pipeline_id),
                revised_resource_id: Some(result.revised_resource_id),
                resource_ids: result.resource_ids,
                message: String::new(),
            },
            Err(error) => revise_rejected(error.message),
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

fn put_rejected(mode: ApplyMode, message: impl Into<String>) -> PutPipelineResponse {
    PutPipelineResponse {
        accepted: false,
        id: None,
        mode: mode as i32,
        message: message.into(),
        resource_ids: Vec::new(),
    }
}

fn apply_rejected(message: impl Into<String>) -> ApplyResourceResponse {
    ApplyResourceResponse {
        accepted: false,
        active_pipeline_id: None,
        mode: ApplyMode::RejectedWorkerError as i32,
        message: message.into(),
    }
}

fn revise_rejected(message: impl Into<String>) -> ReviseResourceResponse {
    ReviseResourceResponse {
        accepted: false,
        revised_pipeline_id: None,
        revised_resource_id: None,
        resource_ids: Vec::new(),
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

fn apply_mode_from_daemon(mode: DaemonApplyMode) -> ApplyMode {
    match mode {
        DaemonApplyMode::Started => ApplyMode::Started,
        DaemonApplyMode::HotReload => ApplyMode::HotReload,
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
    fn puts_pipeline_resource_and_returns_generated_ids() {
        block_on(async {
            let mut service = DaemonService::new();
            let plan = plan();

            let response = service
                .handle_put_pipeline(PutPipelineRequest {
                    pipeline: Some(plan.clone()),
                })
                .await;

            assert!(response.accepted);
            assert_eq!(ApplyMode::try_from(response.mode), Ok(ApplyMode::Unspecified));
            assert_eq!(
                response.id,
                Some(id(ResourceKind::Pipeline, "sensor-pipeline", "r1"))
            );
            assert_eq!(response.resource_ids.len(), 4);
        });
    }

    #[test]
    fn rejects_put_without_pipeline() {
        block_on(async {
            let mut service = DaemonService::new();

            let response = service
                .handle_put_pipeline(PutPipelineRequest { pipeline: None })
                .await;

            assert!(!response.accepted);
            assert_eq!(
                ApplyMode::try_from(response.mode),
                Ok(ApplyMode::RejectedWorkerError)
            );
            assert_eq!(response.message, "pipeline is missing");
        });
    }

    #[test]
    fn gets_pipeline_resource_after_apply() {
        block_on(async {
            let mut service = DaemonService::new();
            let plan = plan();
            let put = service
                .handle_put_pipeline(PutPipelineRequest {
                    pipeline: Some(plan.clone()),
                })
                .await;

            let response = service
                .handle_get_resource(GetResourceRequest {
                    id: put.id.clone(),
                })
                .await;

            assert!(response.found);
            assert_eq!(
                response.resource,
                Some(Resource {
                    kind: Some(resource::Kind::Pipeline(loaded_plan_from_put(&plan))),
                })
            );
        });
    }

    #[test]
    fn repeated_put_advances_pipeline_revision() {
        block_on(async {
            let mut service = DaemonService::new();
            let plan = plan();

            let first = service
                .handle_put_pipeline(PutPipelineRequest {
                    pipeline: Some(plan.clone()),
                })
                .await;
            let second = service
                .handle_put_pipeline(PutPipelineRequest {
                    pipeline: Some(plan),
                })
                .await;

            assert!(first.accepted);
            assert!(second.accepted);
            assert_eq!(first.id, Some(id(ResourceKind::Pipeline, "sensor-pipeline", "r1")));
            assert_eq!(second.id, Some(id(ResourceKind::Pipeline, "sensor-pipeline", "r2")));
        });
    }

    #[test]
    fn revises_process_and_returns_new_pipeline_id() {
        block_on(async {
            let mut service = DaemonService::new();
            let plan = plan();
            let stored_plan = put_full_plan(&mut service, &plan).await;
            let previous_process_id = stored_plan
                .process
                .as_ref()
                .and_then(|process| process.id.clone())
                .expect("process id");

            let new_process = plan.process.clone().expect("process");
            let response = service
                .handle_revise_resource(ReviseResourceRequest {
                    pipeline_id: stored_plan.id.clone(),
                    previous_resource_id: Some(previous_process_id),
                    new_resource: Some(Resource {
                        kind: Some(resource::Kind::Process(new_process)),
                    }),
                })
                .await;

            assert!(response.accepted);
            assert_eq!(
                response.revised_pipeline_id,
                Some(id(ResourceKind::Pipeline, "sensor-pipeline", "r2"))
            );
            assert_eq!(
                response.revised_resource_id,
                Some(id(ResourceKind::Process, "sensor-process", "r2"))
            );
        });
    }

    #[test]
    fn applies_revised_process_with_hot_reload() {
        block_on(async {
            let mut service = DaemonService::new();
            let plan = plan();
            let stored_plan = put_full_plan(&mut service, &plan).await;
            let previous_process_id = stored_plan
                .process
                .as_ref()
                .and_then(|process| process.id.clone())
                .expect("process id");
            service
                .handle_apply_resource(ApplyResourceRequest {
                    pipeline_id: stored_plan.id.clone(),
                })
                .await;
            let original_worker_id = service
                .daemon()
                .deployments()
                .next()
                .expect("active deployment")
                .worker
                .id
                .clone();

            let new_process = plan.process.clone().expect("process");
            let revise = service
                .handle_revise_resource(ReviseResourceRequest {
                    pipeline_id: stored_plan.id.clone(),
                    previous_resource_id: Some(previous_process_id),
                    new_resource: Some(Resource {
                        kind: Some(resource::Kind::Process(new_process)),
                    }),
                })
                .await;
            let apply = service
                .handle_apply_resource(ApplyResourceRequest {
                    pipeline_id: revise.revised_pipeline_id.clone(),
                })
                .await;
            let current_worker_id = service
                .daemon()
                .deployments()
                .next()
                .expect("active deployment")
                .worker
                .id
                .clone();

            assert!(revise.accepted);
            assert!(apply.accepted);
            assert_eq!(ApplyMode::try_from(apply.mode), Ok(ApplyMode::HotReload));
            assert_eq!(current_worker_id, original_worker_id);
        });
    }

    #[test]
    fn applies_latest_pipeline_when_revision_is_empty() {
        block_on(async {
            let mut service = DaemonService::new();
            let plan = plan();
            service
                .handle_put_pipeline(PutPipelineRequest {
                    pipeline: Some(plan),
                })
                .await;

            let apply = service
                .handle_apply_resource(ApplyResourceRequest {
                    pipeline_id: Some(id(ResourceKind::Pipeline, "sensor-pipeline", "")),
                })
                .await;

            assert!(apply.accepted);
            assert_eq!(
                apply.active_pipeline_id,
                Some(id(ResourceKind::Pipeline, "sensor-pipeline", "r1"))
            );
            assert_eq!(ApplyMode::try_from(apply.mode), Ok(ApplyMode::Started));
        });
    }

    #[test]
    fn rejects_revise_for_non_process_resource() {
        block_on(async {
            let mut service = DaemonService::new();
            let plan = plan();
            let stored_plan = put_full_plan(&mut service, &plan).await;

            let egress = plan.egress.clone().expect("egress");
            let response = service
                .handle_revise_resource(ReviseResourceRequest {
                    pipeline_id: stored_plan.id.clone(),
                    previous_resource_id: stored_plan.egress.as_ref().and_then(|egress| egress.id.clone()),
                    new_resource: Some(Resource {
                        kind: Some(resource::Kind::Egress(egress)),
                    }),
                })
                .await;

            assert!(!response.accepted);
            assert!(response.message.contains("process resources only"));
        });
    }

    #[test]
    fn rejects_delete_for_referenced_resource() {
        block_on(async {
            let mut service = DaemonService::new();
            let plan = plan();
            let stored_plan = put_full_plan(&mut service, &plan).await;
            let source_id = stored_plan.sources[0].id.clone();

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
            let stored_plan = put_full_plan(&mut service, &plan).await;
            let plan_id = stored_plan.id.clone();
            let source_id = stored_plan.sources[0].id.clone().expect("source id");

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

    async fn put_full_plan(service: &mut DaemonService, plan: &DeploymentPlan) -> DeploymentPlan {
        let put = service
            .handle_put_pipeline(PutPipelineRequest {
                pipeline: Some(plan.clone()),
            })
            .await;
        let get = service
            .handle_get_resource(GetResourceRequest { id: put.id })
            .await;
        match get.resource.expect("stored pipeline").kind {
            Some(resource::Kind::Pipeline(plan)) => plan,
            _ => panic!("expected pipeline"),
        }
    }

    fn loaded_plan_from_put(plan: &DeploymentPlan) -> DeploymentPlan {
        let mut plan = plan.clone();
        plan.id = Some(id(ResourceKind::Pipeline, "sensor-pipeline", "r1"));
        plan.sources[0].id = Some(id(ResourceKind::MqttSource, "sensor-source", "r1"));
        plan.process.as_mut().expect("process").id =
            Some(id(ResourceKind::Process, "sensor-process", "r1"));
        plan.egress.as_mut().expect("egress").id =
            Some(id(ResourceKind::Egress, "sensor-egress", "r1"));
        plan
    }

}
