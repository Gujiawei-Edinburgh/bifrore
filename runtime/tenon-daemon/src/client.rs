use crate::{DaemonService, DaemonStore, WorkerManager};
use futures::channel::{mpsc, oneshot};
use futures::{SinkExt, StreamExt};
use std::future::Future;
use tenon_message::daemon::v1::{
    StartMode, StartPipelineRequest, StartPipelineResponse, DeletePipelineRequest,
    DeletePipelineResponse, GetPipelineRequest, GetPipelineResponse, GetPipelineStatusRequest,
    GetPipelineStatusResponse, ListPipelinesRequest, ListPipelinesResponse, PutPipelineRequest,
    PutPipelineResponse, StopPipelineRequest, StopPipelineResponse,
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

pub trait DaemonTransportProvider<L, S>
where
    L: WorkerManager,
    S: DaemonStore,
{
    type Client: DaemonClient;
    type Server: DaemonServer;
    type Config;

    fn create(service: DaemonService<L, S>, config: Self::Config) -> (Self::Client, Self::Server);
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct InProcDaemonConfig {
    pub channel_capacity: usize,
}

pub struct InProcDaemonTransportProvider;

#[derive(Clone)]
pub struct InProcDaemonClient {
    sender: mpsc::Sender<DaemonCommand>,
}

pub struct InProcDaemonServer<L, S> {
    service: DaemonService<L, S>,
    receiver: mpsc::Receiver<DaemonCommand>,
}

impl<L, S> DaemonTransportProvider<L, S> for InProcDaemonTransportProvider
where
    L: WorkerManager,
    S: DaemonStore,
{
    type Client = InProcDaemonClient;
    type Server = InProcDaemonServer<L, S>;
    type Config = InProcDaemonConfig;

    fn create(service: DaemonService<L, S>, config: Self::Config) -> (Self::Client, Self::Server) {
        let (sender, receiver) = mpsc::channel(config.channel_capacity);
        (
            InProcDaemonClient { sender },
            InProcDaemonServer { service, receiver },
        )
    }
}

impl DaemonClient for InProcDaemonClient {
    async fn put_pipeline(&self, request: PutPipelineRequest) -> PutPipelineResponse {
        let (reply, receiver) = oneshot::channel();
        let command = DaemonCommand::PutPipeline { request, reply };
        let mut sender = self.sender.clone();
        if sender.send(command).await.is_err() {
            return put_pipeline_failed("daemon service is stopped");
        }
        receiver
            .await
            .unwrap_or_else(|_| put_pipeline_failed("daemon service dropped response"))
    }

    async fn start_pipeline(&self, request: StartPipelineRequest) -> StartPipelineResponse {
        let (reply, receiver) = oneshot::channel();
        let command = DaemonCommand::StartPipeline { request, reply };
        let mut sender = self.sender.clone();
        if sender.send(command).await.is_err() {
            return start_pipeline_failed("daemon service is stopped");
        }
        receiver
            .await
            .unwrap_or_else(|_| start_pipeline_failed("daemon service dropped response"))
    }

    async fn get_pipeline(&self, request: GetPipelineRequest) -> GetPipelineResponse {
        let (reply, receiver) = oneshot::channel();
        let command = DaemonCommand::GetPipeline { request, reply };
        let mut sender = self.sender.clone();
        if sender.send(command).await.is_err() {
            return GetPipelineResponse {
                found: false,
                pipeline: None,
                message: "daemon service is stopped".to_string(),
            };
        }
        receiver.await.unwrap_or_else(|_| GetPipelineResponse {
            found: false,
            pipeline: None,
            message: "daemon service dropped response".to_string(),
        })
    }

    async fn list_pipelines(&self, request: ListPipelinesRequest) -> ListPipelinesResponse {
        let (reply, receiver) = oneshot::channel();
        let command = DaemonCommand::ListPipelines { request, reply };
        let mut sender = self.sender.clone();
        if sender.send(command).await.is_err() {
            return ListPipelinesResponse {
                pipelines: Vec::new(),
                message: "daemon service is stopped".to_string(),
            };
        }
        receiver.await.unwrap_or_else(|_| ListPipelinesResponse {
            pipelines: Vec::new(),
            message: "daemon service dropped response".to_string(),
        })
    }

    async fn delete_pipeline(&self, request: DeletePipelineRequest) -> DeletePipelineResponse {
        let (reply, receiver) = oneshot::channel();
        let command = DaemonCommand::DeletePipeline { request, reply };
        let mut sender = self.sender.clone();
        if sender.send(command).await.is_err() {
            return DeletePipelineResponse {
                deleted: false,
                id: None,
                message: "daemon service is stopped".to_string(),
            };
        }
        receiver.await.unwrap_or_else(|_| DeletePipelineResponse {
            deleted: false,
            id: None,
            message: "daemon service dropped response".to_string(),
        })
    }

    async fn stop_pipeline(&self, request: StopPipelineRequest) -> StopPipelineResponse {
        let (reply, receiver) = oneshot::channel();
        let command = DaemonCommand::StopPipeline { request, reply };
        let mut sender = self.sender.clone();
        if sender.send(command).await.is_err() {
            return stop_pipeline_failed("daemon service is stopped");
        }
        receiver
            .await
            .unwrap_or_else(|_| stop_pipeline_failed("daemon service dropped response"))
    }

    async fn get_pipeline_status(
        &self,
        request: GetPipelineStatusRequest,
    ) -> GetPipelineStatusResponse {
        let (reply, receiver) = oneshot::channel();
        let command = DaemonCommand::GetPipelineStatus { request, reply };
        let mut sender = self.sender.clone();
        if sender.send(command).await.is_err() {
            return pipeline_status_failed("daemon service is stopped");
        }
        receiver
            .await
            .unwrap_or_else(|_| pipeline_status_failed("daemon service dropped response"))
    }
}

impl<L, S> InProcDaemonServer<L, S>
where
    L: WorkerManager,
    S: DaemonStore,
{
    async fn handle_command(&mut self, command: DaemonCommand) {
        match command {
            DaemonCommand::PutPipeline { request, reply } => {
                let response = self.service.handle_put_pipeline(request).await;
                let _ = reply.send(response);
            }
            DaemonCommand::StartPipeline { request, reply } => {
                let response = self.service.handle_start_pipeline(request).await;
                let _ = reply.send(response);
            }
            DaemonCommand::GetPipeline { request, reply } => {
                let response = self.service.handle_get_pipeline(request).await;
                let _ = reply.send(response);
            }
            DaemonCommand::ListPipelines { request, reply } => {
                let response = self.service.handle_list_pipelines(request).await;
                let _ = reply.send(response);
            }
            DaemonCommand::DeletePipeline { request, reply } => {
                let response = self.service.handle_delete_pipeline(request).await;
                let _ = reply.send(response);
            }
            DaemonCommand::StopPipeline { request, reply } => {
                let response = self.service.handle_stop_pipeline(request).await;
                let _ = reply.send(response);
            }
            DaemonCommand::GetPipelineStatus { request, reply } => {
                let response = self.service.handle_get_pipeline_status(request).await;
                let _ = reply.send(response);
            }
        }
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
        while let Some(command) = self.receiver.next().await {
            self.handle_command(command).await;
        }
    }
}

enum DaemonCommand {
    PutPipeline {
        request: PutPipelineRequest,
        reply: oneshot::Sender<PutPipelineResponse>,
    },
    StartPipeline {
        request: StartPipelineRequest,
        reply: oneshot::Sender<StartPipelineResponse>,
    },
    GetPipeline {
        request: GetPipelineRequest,
        reply: oneshot::Sender<GetPipelineResponse>,
    },
    ListPipelines {
        request: ListPipelinesRequest,
        reply: oneshot::Sender<ListPipelinesResponse>,
    },
    DeletePipeline {
        request: DeletePipelineRequest,
        reply: oneshot::Sender<DeletePipelineResponse>,
    },
    StopPipeline {
        request: StopPipelineRequest,
        reply: oneshot::Sender<StopPipelineResponse>,
    },
    GetPipelineStatus {
        request: GetPipelineStatusRequest,
        reply: oneshot::Sender<GetPipelineStatusResponse>,
    },
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
            let (client, server) = InProcDaemonTransportProvider::create(
                service(),
                InProcDaemonConfig {
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
            let (client, server) = InProcDaemonTransportProvider::create(
                service(),
                InProcDaemonConfig {
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
