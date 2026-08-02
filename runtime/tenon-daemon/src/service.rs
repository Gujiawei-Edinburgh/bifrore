use crate::{
    DaemonStartMode, DaemonError, DaemonErrorKind, DaemonStore, TenonDaemon, WorkerManager,
    WorkerStatus,
};
use tenon_message::daemon::v1::{
    StartMode, StartPipelineRequest, StartPipelineResponse, DeletePipelineRequest,
    DeletePipelineResponse, GetPipelineRequest, GetPipelineResponse, GetPipelineStatusRequest,
    GetPipelineStatusResponse, ListPipelinesRequest, ListPipelinesResponse, PutPipelineRequest,
    PutPipelineResponse, StopPipelineRequest, StopPipelineResponse, WorkerState,
};

pub struct DaemonService<L, S> {
    daemon: TenonDaemon<L, S>,
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

    // Resource APIs persist and validate the desired pipeline definition.
    pub async fn handle_put_pipeline(
        &mut self,
        request: PutPipelineRequest,
    ) -> PutPipelineResponse {
        let Some(pipeline) = request.pipeline else {
            return put_rejected(StartMode::RejectedWorkerError, "pipeline is missing");
        };

        match self.daemon.put_pipeline(pipeline).await {
            Ok(result) => PutPipelineResponse {
                accepted: true,
                id: Some(result.id),
                mode: StartMode::Unspecified as i32,
                message: String::new(),
            },
            Err(error) => put_rejected(start_error_mode(&error), error.message),
        }
    }

    pub async fn handle_get_pipeline(
        &self,
        request: GetPipelineRequest,
    ) -> GetPipelineResponse {
        match self
            .daemon
            .get_pipeline(&request.pipeline_name, request.pipeline_ver.as_deref())
            .await
        {
            Ok(Some(result)) => GetPipelineResponse {
                found: true,
                pipeline: Some(result.plan),
                message: String::new(),
            },
            Ok(None) => GetPipelineResponse {
                found: false,
                pipeline: None,
                message: String::new(),
            },
            Err(error) => GetPipelineResponse {
                found: false,
                pipeline: None,
                message: error.message,
            },
        }
    }

    pub async fn handle_list_pipelines(
        &self,
        _request: ListPipelinesRequest,
    ) -> ListPipelinesResponse {
        match self.daemon.list_pipelines().await {
            Ok(pipelines) => ListPipelinesResponse {
                pipelines,
                message: String::new(),
            },
            Err(error) => ListPipelinesResponse {
                pipelines: Vec::new(),
                message: error.message,
            },
        }
    }

    pub async fn handle_delete_pipeline(
        &mut self,
        request: DeletePipelineRequest,
    ) -> DeletePipelineResponse {
        match self
            .daemon
            .delete_pipeline(&request.pipeline_name, request.pipeline_ver.as_deref())
            .await
        {
            Ok(Some(id)) => DeletePipelineResponse {
                deleted: true,
                id: Some(id),
                message: String::new(),
            },
            Ok(None) => DeletePipelineResponse {
                deleted: false,
                id: None,
                message: "pipeline resource not found".to_string(),
            },
            Err(error) => DeletePipelineResponse {
                deleted: false,
                id: None,
                message: error.message,
            },
        }
    }

    // Deployment APIs control the worker running a persisted pipeline.
    pub async fn handle_start_pipeline(
        &mut self,
        request: StartPipelineRequest,
    ) -> StartPipelineResponse {
        match self
            .daemon
            .start_pipeline(&request.pipeline_name, request.pipeline_ver.as_deref())
            .await
        {
            Ok(result) => StartPipelineResponse {
                accepted: true,
                active_pipeline_id: Some(result.id),
                mode: start_mode_from_daemon(result.mode) as i32,
                message: String::new(),
            },
            Err(error) => StartPipelineResponse {
                accepted: false,
                active_pipeline_id: None,
                mode: start_error_mode(&error) as i32,
                message: error.message,
            },
        }
    }

    pub async fn handle_stop_pipeline(
        &mut self,
        request: StopPipelineRequest,
    ) -> StopPipelineResponse {
        match self
            .daemon
            .stop_pipeline(&request.pipeline_name, request.pipeline_ver.as_deref())
            .await
        {
            Ok(state) => StopPipelineResponse {
                stopped: true,
                state: worker_state(state),
                message: String::new(),
            },
            Err(error) => StopPipelineResponse {
                stopped: false,
                state: WorkerState::Error as i32,
                message: error.message,
            },
        }
    }

    pub async fn handle_get_pipeline_status(
        &mut self,
        request: GetPipelineStatusRequest,
    ) -> GetPipelineStatusResponse {
        let pipeline = match self
            .daemon
            .get_pipeline(&request.pipeline_name, request.pipeline_ver.as_deref())
            .await
        {
            Ok(Some(pipeline)) => pipeline,
            Ok(None) => {
                return GetPipelineStatusResponse {
                    id: None,
                    state: WorkerState::Error as i32,
                    message: "pipeline resource not found".to_string(),
                }
            }
            Err(error) => {
                return GetPipelineStatusResponse {
                    id: None,
                    state: WorkerState::Error as i32,
                    message: error.message,
                }
            }
        };
        match self.daemon.worker_status(&pipeline.id).await {
            Ok(state) => GetPipelineStatusResponse {
                id: Some(pipeline.id),
                state: worker_state(state),
                message: String::new(),
            },
            Err(error) => GetPipelineStatusResponse {
                id: Some(pipeline.id),
                state: WorkerState::Error as i32,
                message: error.message,
            },
        }
    }
}

fn worker_state(status: WorkerStatus) -> i32 {
    (match status {
        WorkerStatus::Init => WorkerState::Init,
        WorkerStatus::Starting => WorkerState::Starting,
        WorkerStatus::Running => WorkerState::Running,
        WorkerStatus::Stopping => WorkerState::Stopping,
        WorkerStatus::Stopped => WorkerState::Stopped,
        WorkerStatus::Error => WorkerState::Error,
    }) as i32
}

fn put_rejected(mode: StartMode, message: impl Into<String>) -> PutPipelineResponse {
    PutPipelineResponse {
        accepted: false,
        id: None,
        mode: mode as i32,
        message: message.into(),
    }
}

fn start_error_mode(error: &DaemonError) -> StartMode {
    match error.kind {
        DaemonErrorKind::Worker => StartMode::RejectedWorkerError,
        DaemonErrorKind::Store | DaemonErrorKind::InvalidState | DaemonErrorKind::NotFound => {
            StartMode::RejectedWorkerError
        }
    }
}

fn start_mode_from_daemon(mode: DaemonStartMode) -> StartMode {
    match mode {
        DaemonStartMode::Started => StartMode::Started,
        DaemonStartMode::HotReload => StartMode::HotReload,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InMemoryDaemonStore, NoopWorkerManager};
    use futures::executor::block_on;
    use tenon_message::plan::{
        DeploymentPlan, EgressPlan, ExecutionMode, MqttBrokerPlan, MqttSourcePlan,
        MqttSubscriptionPlan, PayloadDecodePlan, ProcessPlan, ResourceId, ScriptRuntime,
        MessageAccessPlan,
    };

    #[test]
    fn puts_pipeline_and_returns_generated_revision() {
        block_on(async {
            let mut service = service();
            let response = service
                .handle_put_pipeline(PutPipelineRequest {
                    pipeline: Some(plan("function on_message(ctx, msg) end")),
                })
                .await;

            assert!(response.accepted);
            assert_eq!(StartMode::try_from(response.mode), Ok(StartMode::Unspecified));
            assert_eq!(
                response.id,
                Some(id("sensor-pipeline", "r1"))
            );
        });
    }

    #[test]
    fn rejects_put_without_pipeline() {
        block_on(async {
            let mut service = service();
            let response = service
                .handle_put_pipeline(PutPipelineRequest { pipeline: None })
                .await;

            assert!(!response.accepted);
            assert_eq!(response.message, "pipeline is missing");
        });
    }

    #[test]
    fn manages_pipeline_resource_lifecycle() {
        block_on(async {
            let mut service = service();
            let put = service
                .handle_put_pipeline(PutPipelineRequest {
                    pipeline: Some(plan("function on_message(ctx, msg) end")),
                })
                .await;
            let id = put.id.clone().expect("pipeline id");

            let get = service
                .handle_get_pipeline(GetPipelineRequest {
                    pipeline_name: id.name.clone(),
                    pipeline_ver: None,
                })
                .await;
            assert!(get.found);
            assert_eq!(get.pipeline.and_then(|plan| plan.id), Some(id.clone()));

            let list = service
                .handle_list_pipelines(ListPipelinesRequest {})
                .await;
            assert_eq!(list.pipelines, vec![id.clone()]);

            let delete = service
                .handle_delete_pipeline(DeletePipelineRequest {
                    pipeline_name: id.name.clone(),
                    pipeline_ver: None,
                })
                .await;
            assert!(delete.deleted);
            assert_eq!(delete.id, Some(id));
        });
    }

    #[test]
    fn rejects_put_pipeline_without_process_access_plan() {
        block_on(async {
            let mut service = service();
            let mut plan = plan("function on_message(ctx, msg) end");
            plan.process.as_mut().expect("process").access_plan = None;

            let response = service
                .handle_put_pipeline(PutPipelineRequest {
                    pipeline: Some(plan),
                })
                .await;

            assert!(!response.accepted);
            assert!(response.message.contains("process access_plan is missing"));

            let apply = service
                .handle_start_pipeline(StartPipelineRequest {
                    pipeline_name: "sensor-pipeline".to_string(),
                    pipeline_ver: None,
                })
                .await;
            assert!(!apply.accepted);
            assert!(apply.message.contains("pipeline resource not found"));
        });
    }

    #[test]
    fn repeated_put_advances_pipeline_revision() {
        block_on(async {
            let mut service = service();

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
            let mut service = service();
            service
                .handle_put_pipeline(PutPipelineRequest {
                    pipeline: Some(plan("function on_message(ctx, msg) end")),
                })
                .await;

            let apply = service
                .handle_start_pipeline(StartPipelineRequest {
                    pipeline_name: "sensor-pipeline".to_string(),
                    pipeline_ver: None,
                })
                .await;

            assert!(apply.accepted);
            assert_eq!(
                apply.active_pipeline_id,
                Some(id("sensor-pipeline", "r1"))
            );
            assert_eq!(StartMode::try_from(apply.mode), Ok(StartMode::Started));
        });
    }

    #[test]
    fn applies_named_revision() {
        block_on(async {
            let mut service = service();
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
                .handle_start_pipeline(StartPipelineRequest {
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
            let mut service = service();
            service
                .handle_put_pipeline(PutPipelineRequest {
                    pipeline: Some(plan("function on_message(ctx, msg) end")),
                })
                .await;
            service
                .handle_start_pipeline(StartPipelineRequest {
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
                .handle_start_pipeline(StartPipelineRequest {
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
            assert_eq!(StartMode::try_from(apply.mode), Ok(StartMode::HotReload));
            assert_eq!(current_worker_id, original_worker_id);
        });
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
                access_plan: Some(MessageAccessPlan::default()),
            }),
            egress: Some(EgressPlan {}),
        }
    }

    fn id(name: &str, version: &str) -> ResourceId {
        ResourceId {
            name: name.to_string(),
            version: version.to_string(),
        }
    }

    fn service() -> DaemonService<NoopWorkerManager, InMemoryDaemonStore> {
        DaemonService::with_daemon(TenonDaemon::with_components(
            NoopWorkerManager::default(),
            InMemoryDaemonStore::default(),
        ))
    }
}
