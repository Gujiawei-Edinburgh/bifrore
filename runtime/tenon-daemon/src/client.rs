use crate::{DaemonService, DaemonStore, WorkerManager};
use std::future::Future;
use tenon_message::daemon::v1::{
    StartMode, StartPipelineRequest, StartPipelineResponse, DeletePipelineRequest,
    DeletePipelineResponse, GetPipelineRequest, GetPipelineResponse, GetPipelineStatusRequest,
    GetPipelineStatusResponse, ListPipelinesRequest, ListPipelinesResponse, PutPipelineRequest,
    PutPipelineResponse, StopPipelineRequest, StopPipelineResponse,
};
use tenon_transport::{
    InProcRequester, InProcResponder, InProcTransportConfig, InProcTransportProvider, Requester,
    Responder, Transport,
};

pub trait DaemonClient {
    fn put_pipeline(
        &self,
        request: PutPipelineRequest,
    ) -> impl Future<Output = PutPipelineResponse> + Send + '_;

    fn start_pipeline(
        &self,
        request: StartPipelineRequest,
    ) -> impl Future<Output = StartPipelineResponse> + Send + '_;

    fn get_pipeline(
        &self,
        request: GetPipelineRequest,
    ) -> impl Future<Output = GetPipelineResponse> + Send + '_;

    fn list_pipelines(
        &self,
        request: ListPipelinesRequest,
    ) -> impl Future<Output = ListPipelinesResponse> + Send + '_;

    fn delete_pipeline(
        &self,
        request: DeletePipelineRequest,
    ) -> impl Future<Output = DeletePipelineResponse> + Send + '_;

    fn stop_pipeline(
        &self,
        request: StopPipelineRequest,
    ) -> impl Future<Output = StopPipelineResponse> + Send + '_;

    fn get_pipeline_status(
        &self,
        request: GetPipelineStatusRequest,
    ) -> impl Future<Output = GetPipelineStatusResponse> + Send + '_;
}

pub trait DaemonServer {
    fn serve(self) -> impl Future<Output = ()>;
}

#[derive(Clone)]
pub struct InProcDaemonClient {
    transport: InProcRequester<DaemonRequest, DaemonResponse>,
}

pub struct InProcDaemonServer<L, S> {
    service: DaemonService<L, S>,
    transport: InProcResponder<DaemonRequest, DaemonResponse>,
}

pub fn create_in_proc_daemon<L, S>(
    service: DaemonService<L, S>,
    config: InProcTransportConfig,
) -> (InProcDaemonClient, InProcDaemonServer<L, S>)
where
    L: WorkerManager,
    S: DaemonStore,
{
    let provider = Transport::provide::<InProcTransportProvider>(config);
    let (transport, server_transport) = provider.pair::<DaemonRequest, DaemonResponse>();
    (
        InProcDaemonClient { transport },
        InProcDaemonServer {
            service,
            transport: server_transport,
        },
    )
}

impl DaemonClient for InProcDaemonClient {
    async fn put_pipeline(&self, request: PutPipelineRequest) -> PutPipelineResponse {
        match self.transport.request(DaemonRequest::PutPipeline(request)).await {
            Ok(DaemonResponse::PutPipeline(response)) => response,
            Ok(_) => put_pipeline_failed("unexpected daemon response"),
            Err(error) => put_pipeline_failed(format!("daemon transport failed: {error:?}")),
        }
    }

    async fn start_pipeline(&self, request: StartPipelineRequest) -> StartPipelineResponse {
        match self.transport.request(DaemonRequest::StartPipeline(request)).await {
            Ok(DaemonResponse::StartPipeline(response)) => response,
            Ok(_) => start_pipeline_failed("unexpected daemon response"),
            Err(error) => start_pipeline_failed(format!("daemon transport failed: {error:?}")),
        }
    }

    async fn get_pipeline(&self, request: GetPipelineRequest) -> GetPipelineResponse {
        match self.transport.request(DaemonRequest::GetPipeline(request)).await {
            Ok(DaemonResponse::GetPipeline(response)) => response,
            Ok(_) => get_pipeline_failed("unexpected daemon response"),
            Err(error) => get_pipeline_failed(format!("daemon transport failed: {error:?}")),
        }
    }

    async fn list_pipelines(&self, request: ListPipelinesRequest) -> ListPipelinesResponse {
        match self.transport.request(DaemonRequest::ListPipelines(request)).await {
            Ok(DaemonResponse::ListPipelines(response)) => response,
            Ok(_) => list_pipelines_failed("unexpected daemon response"),
            Err(error) => list_pipelines_failed(format!("daemon transport failed: {error:?}")),
        }
    }

    async fn delete_pipeline(&self, request: DeletePipelineRequest) -> DeletePipelineResponse {
        match self.transport.request(DaemonRequest::DeletePipeline(request)).await {
            Ok(DaemonResponse::DeletePipeline(response)) => response,
            Ok(_) => delete_pipeline_failed("unexpected daemon response"),
            Err(error) => delete_pipeline_failed(format!("daemon transport failed: {error:?}")),
        }
    }

    async fn stop_pipeline(&self, request: StopPipelineRequest) -> StopPipelineResponse {
        match self.transport.request(DaemonRequest::StopPipeline(request)).await {
            Ok(DaemonResponse::StopPipeline(response)) => response,
            Ok(_) => stop_pipeline_failed("unexpected daemon response"),
            Err(error) => stop_pipeline_failed(format!("daemon transport failed: {error:?}")),
        }
    }

    async fn get_pipeline_status(
        &self,
        request: GetPipelineStatusRequest,
    ) -> GetPipelineStatusResponse {
        match self
            .transport
            .request(DaemonRequest::GetPipelineStatus(request))
            .await
        {
            Ok(DaemonResponse::GetPipelineStatus(response)) => response,
            Ok(_) => pipeline_status_failed("unexpected daemon response"),
            Err(error) => pipeline_status_failed(format!("daemon transport failed: {error:?}")),
        }
    }
}

impl<L, S> InProcDaemonServer<L, S>
where
    L: WorkerManager,
    S: DaemonStore,
{
    async fn handle_command(&mut self, request: DaemonRequest) {
        let response = match request {
            DaemonRequest::PutPipeline(request) => {
                let response = self.service.handle_put_pipeline(request).await;
                DaemonResponse::PutPipeline(response)
            }
            DaemonRequest::StartPipeline(request) => {
                let response = self.service.handle_start_pipeline(request).await;
                DaemonResponse::StartPipeline(response)
            }
            DaemonRequest::GetPipeline(request) => {
                let response = self.service.handle_get_pipeline(request).await;
                DaemonResponse::GetPipeline(response)
            }
            DaemonRequest::ListPipelines(request) => {
                let response = self.service.handle_list_pipelines(request).await;
                DaemonResponse::ListPipelines(response)
            }
            DaemonRequest::DeletePipeline(request) => {
                let response = self.service.handle_delete_pipeline(request).await;
                DaemonResponse::DeletePipeline(response)
            }
            DaemonRequest::StopPipeline(request) => {
                let response = self.service.handle_stop_pipeline(request).await;
                DaemonResponse::StopPipeline(response)
            }
            DaemonRequest::GetPipelineStatus(request) => {
                let response = self.service.handle_get_pipeline_status(request).await;
                DaemonResponse::GetPipelineStatus(response)
            }
        };
        let _ = self.transport.respond(response).await;
    }

    pub fn service(&self) -> &DaemonService<L, S> {
        &self.service
    }

    pub fn service_mut(&mut self) -> &mut DaemonService<L, S> {
        &mut self.service
    }
}

impl<L, S> DaemonServer for InProcDaemonServer<L, S>
where
    L: WorkerManager,
    S: DaemonStore,
{
    async fn serve(mut self) {
        while let Ok(request) = self.transport.receive().await {
            self.handle_command(request).await;
        }
    }
}

enum DaemonRequest {
    PutPipeline(PutPipelineRequest),
    StartPipeline(StartPipelineRequest),
    GetPipeline(GetPipelineRequest),
    ListPipelines(ListPipelinesRequest),
    DeletePipeline(DeletePipelineRequest),
    StopPipeline(StopPipelineRequest),
    GetPipelineStatus(GetPipelineStatusRequest),
}

enum DaemonResponse {
    PutPipeline(PutPipelineResponse),
    StartPipeline(StartPipelineResponse),
    GetPipeline(GetPipelineResponse),
    ListPipelines(ListPipelinesResponse),
    DeletePipeline(DeletePipelineResponse),
    StopPipeline(StopPipelineResponse),
    GetPipelineStatus(GetPipelineStatusResponse),
}

fn put_pipeline_failed(message: impl Into<String>) -> PutPipelineResponse {
    PutPipelineResponse {
        accepted: false,
        id: None,
        mode: StartMode::RejectedWorkerError as i32,
        message: message.into(),
    }
}

fn start_pipeline_failed(message: impl Into<String>) -> StartPipelineResponse {
    StartPipelineResponse {
        accepted: false,
        active_pipeline_id: None,
        mode: StartMode::RejectedWorkerError as i32,
        message: message.into(),
    }
}

fn get_pipeline_failed(message: impl Into<String>) -> GetPipelineResponse {
    GetPipelineResponse {
        found: false,
        pipeline: None,
        message: message.into(),
    }
}

fn list_pipelines_failed(message: impl Into<String>) -> ListPipelinesResponse {
    ListPipelinesResponse {
        pipelines: Vec::new(),
        message: message.into(),
    }
}

fn delete_pipeline_failed(message: impl Into<String>) -> DeletePipelineResponse {
    DeletePipelineResponse {
        deleted: false,
        id: None,
        message: message.into(),
    }
}

fn stop_pipeline_failed(message: impl Into<String>) -> StopPipelineResponse {
    StopPipelineResponse {
        stopped: false,
        state: tenon_message::daemon::v1::WorkerState::Error as i32,
        message: message.into(),
    }
}

fn pipeline_status_failed(message: impl Into<String>) -> GetPipelineStatusResponse {
    GetPipelineStatusResponse {
        id: None,
        state: tenon_message::daemon::v1::WorkerState::Error as i32,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InMemoryDaemonStore, NoopWorkerManager, TenonDaemon};
    use futures::executor::block_on;
    use tenon_message::daemon::v1::StartMode;
    use tenon_message::plan::{
        DeploymentPlan, EgressPlan, ExecutionMode, MqttBrokerPlan, MqttSourcePlan,
        MqttSubscriptionPlan, PayloadDecodePlan, ProcessPlan, ResourceId, ScriptRuntime,
        MessageAccessPlan,
    };

    #[test]
    fn in_proc_client_puts_pipeline_through_server() {
        block_on(async {
            let (client, server) = create_in_proc_daemon(
                service(),
                InProcTransportConfig {
                    channel_capacity: 8,
                },
            );
            let requests = async move {
                let request = PutPipelineRequest {
                    pipeline: Some(plan("function on_message(ctx, msg) end")),
                };
                client.put_pipeline(request).await
            };

            let (response, _) = futures::join!(requests, server.serve());

            assert!(response.accepted);
            assert_eq!(response.id, Some(id("sensor-pipeline", "r1")));
        });
    }

    #[test]
    fn in_proc_client_applies_latest_pipeline() {
        block_on(async {
            let (client, server) = create_in_proc_daemon(
                service(),
                InProcTransportConfig {
                    channel_capacity: 8,
                },
            );
            let requests = async move {
                let put_request = PutPipelineRequest {
                    pipeline: Some(plan("function on_message(ctx, msg) end")),
                };
                let _ = client.put_pipeline(put_request).await;

                let start_request = StartPipelineRequest {
                    pipeline_name: "sensor-pipeline".to_string(),
                    pipeline_ver: None,
                };
                client.start_pipeline(start_request).await
            };
            let (response, _) = futures::join!(requests, server.serve());

            assert!(response.accepted);
            assert_eq!(StartMode::try_from(response.mode), Ok(StartMode::Started));
            assert_eq!(response.active_pipeline_id, Some(id("sensor-pipeline", "r1")));
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
