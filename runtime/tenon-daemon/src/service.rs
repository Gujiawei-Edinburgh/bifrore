use crate::{
    DaemonApplyMode, DaemonError, DaemonErrorKind, DaemonResult, DaemonStore,
    InMemoryDaemonStore, NoopWorkerLauncher, TenonDaemon, WorkerManager,
};
use tenon_message::daemon::v1::{
    worker_envelope, ApplyMode, ApplyResourceRequest, ApplyResourceResponse, PutPipelineRequest,
    PutPipelineResponse, WorkerEnvelope,
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
    L: WorkerManager,
    S: DaemonStore,
{
    pub fn with_daemon(daemon: TenonDaemon<L, S>) -> Self {
        Self { daemon }
    }

    pub fn daemon(&self) -> &TenonDaemon<L, S> {
        &self.daemon
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
            },
            Err(error) => put_rejected(apply_error_mode(&error), error.message),
        }
    }

    pub async fn handle_apply_resource(
        &mut self,
        request: ApplyResourceRequest,
    ) -> ApplyResourceResponse {
        match self
            .daemon
            .apply_pipeline(&request.pipeline_name, request.pipeline_ver.as_deref())
            .await
        {
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
    use futures::executor::block_on;
    use tenon_message::daemon::v1::Heartbeat;
    use tenon_message::plan::{
        DeliveryMode, DeploymentPlan, EgressPlan, ExecutionMode, MqttBrokerPlan, MqttSourcePlan,
        MqttSubscriptionPlan, PayloadDecodePlan, ProcessPlan, ResourceId, ScriptRuntime,
    };

    #[test]
    fn puts_pipeline_and_returns_generated_revision() {
        block_on(async {
            let mut service = DaemonService::new();
            let response = service
                .handle_put_pipeline(PutPipelineRequest {
                    pipeline: Some(plan("function on_message(ctx, msg) end")),
                })
                .await;

            assert!(response.accepted);
            assert_eq!(ApplyMode::try_from(response.mode), Ok(ApplyMode::Unspecified));
            assert_eq!(
                response.id,
                Some(id("sensor-pipeline", "r1"))
            );
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
            assert_eq!(response.message, "pipeline is missing");
        });
    }

    #[test]
    fn repeated_put_advances_pipeline_revision() {
        block_on(async {
            let mut service = DaemonService::new();

            let first = service
                .handle_put_pipeline(PutPipelineRequest {
                    pipeline: Some(plan("function on_message(ctx, msg) end")),
                })
                .await;
            let second = service
                .handle_put_pipeline(PutPipelineRequest {
                    pipeline: Some(plan("function on_message(ctx, msg) end")),
                })
                .await;

            assert_eq!(first.id, Some(id("sensor-pipeline", "r1")));
            assert_eq!(second.id, Some(id("sensor-pipeline", "r2")));
        });
    }

    #[test]
    fn applies_latest_pipeline_when_revision_is_missing() {
        block_on(async {
            let mut service = DaemonService::new();
            service
                .handle_put_pipeline(PutPipelineRequest {
                    pipeline: Some(plan("function on_message(ctx, msg) end")),
                })
                .await;

            let apply = service
                .handle_apply_resource(ApplyResourceRequest {
                    pipeline_name: "sensor-pipeline".to_string(),
                    pipeline_ver: None,
                })
                .await;

            assert!(apply.accepted);
            assert_eq!(
                apply.active_pipeline_id,
                Some(id("sensor-pipeline", "r1"))
            );
            assert_eq!(ApplyMode::try_from(apply.mode), Ok(ApplyMode::Started));
        });
    }

    #[test]
    fn applies_named_revision() {
        block_on(async {
            let mut service = DaemonService::new();
            service
                .handle_put_pipeline(PutPipelineRequest {
                    pipeline: Some(plan("function on_message(ctx, msg) end")),
                })
                .await;
            service
                .handle_put_pipeline(PutPipelineRequest {
                    pipeline: Some(plan("function on_message(ctx, msg) return nil end")),
                })
                .await;

            let apply = service
                .handle_apply_resource(ApplyResourceRequest {
                    pipeline_name: "sensor-pipeline".to_string(),
                    pipeline_ver: Some("r1".to_string()),
                })
                .await;

            assert!(apply.accepted);
            assert_eq!(
                apply.active_pipeline_id,
                Some(id("sensor-pipeline", "r1"))
            );
        });
    }

    #[test]
    fn process_only_change_hot_reloads() {
        block_on(async {
            let mut service = DaemonService::new();
            service
                .handle_put_pipeline(PutPipelineRequest {
                    pipeline: Some(plan("function on_message(ctx, msg) end")),
                })
                .await;
            service
                .handle_apply_resource(ApplyResourceRequest {
                    pipeline_name: "sensor-pipeline".to_string(),
                    pipeline_ver: None,
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

            service
                .handle_put_pipeline(PutPipelineRequest {
                    pipeline: Some(plan("function on_message(ctx, msg) return nil end")),
                })
                .await;
            let apply = service
                .handle_apply_resource(ApplyResourceRequest {
                    pipeline_name: "sensor-pipeline".to_string(),
                    pipeline_ver: None,
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

            assert!(apply.accepted);
            assert_eq!(ApplyMode::try_from(apply.mode), Ok(ApplyMode::HotReload));
            assert_eq!(current_worker_id, original_worker_id);
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

        service.handle_worker_envelope(envelope).expect("heartbeat");
    }

    #[test]
    fn rejects_worker_start_request() {
        let mut service = DaemonService::new();
        let envelope = WorkerEnvelope {
            payload: Some(worker_envelope::Payload::StartWorker(
                tenon_message::daemon::v1::StartWorkerRequest {
                    plan: Some(plan("function on_message(ctx, msg) end")),
                },
            )),
        };

        let error = service
            .handle_worker_envelope(envelope)
            .expect_err("worker start request should be rejected");

        assert_eq!(error.kind, DaemonErrorKind::InvalidState);
    }

    fn plan(process_source: &str) -> DeploymentPlan {
        DeploymentPlan {
            id: Some(id("sensor-pipeline", "v1")),
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
                client_count: 1,
            }],
            process: Some(ProcessPlan {
                runtime: ScriptRuntime::Lua as i32,
                source: process_source.to_string(),
            }),
            egress: Some(EgressPlan {
                delivery: DeliveryMode::Single as i32,
            }),
        }
    }

    fn id(name: &str, version: &str) -> ResourceId {
        ResourceId {
            name: name.to_string(),
            version: version.to_string(),
        }
    }
}
