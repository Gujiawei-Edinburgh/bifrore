use crate::{DaemonService, DaemonStore, WorkerManager};
use futures::channel::{mpsc, oneshot};
use futures::{SinkExt, StreamExt};
use std::future::Future;
use tenon_message::daemon::v1::{
    ApplyMode, ApplyResourceRequest, ApplyResourceResponse, PutPipelineRequest,
    PutPipelineResponse,
};

pub trait DaemonClient {
    fn put_pipeline(
        &self,
        request: PutPipelineRequest,
    ) -> impl Future<Output = PutPipelineResponse> + Send + '_;

    fn apply_resource(
        &self,
        request: ApplyResourceRequest,
    ) -> impl Future<Output = ApplyResourceResponse> + Send + '_;
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

    async fn apply_resource(&self, request: ApplyResourceRequest) -> ApplyResourceResponse {
        let (reply, receiver) = oneshot::channel();
        let command = DaemonCommand::ApplyResource { request, reply };
        let mut sender = self.sender.clone();
        if sender.send(command).await.is_err() {
            return apply_resource_failed("daemon service is stopped");
        }
        receiver
            .await
            .unwrap_or_else(|_| apply_resource_failed("daemon service dropped response"))
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
            DaemonCommand::ApplyResource { request, reply } => {
                let response = self.service.handle_apply_resource(request).await;
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
    ApplyResource {
        request: ApplyResourceRequest,
        reply: oneshot::Sender<ApplyResourceResponse>,
    },
}

fn put_pipeline_failed(message: impl Into<String>) -> PutPipelineResponse {
    PutPipelineResponse {
        accepted: false,
        id: None,
        mode: ApplyMode::RejectedWorkerError as i32,
        message: message.into(),
    }
}

fn apply_resource_failed(message: impl Into<String>) -> ApplyResourceResponse {
    ApplyResourceResponse {
        accepted: false,
        active_pipeline_id: None,
        mode: ApplyMode::RejectedWorkerError as i32,
        message: message.into(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{InMemoryDaemonStore, NoopWorkerLauncher, TenonDaemon};
    use futures::executor::block_on;
    use tenon_message::daemon::v1::ApplyMode;
    use tenon_message::plan::{
        DeliveryMode, DeploymentPlan, EgressPlan, ExecutionMode, MqttBrokerPlan, MqttSourcePlan,
        MqttSubscriptionPlan, PayloadDecodePlan, ProcessPlan, ResourceId, ScriptRuntime,
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

                let apply_request = ApplyResourceRequest {
                    pipeline_name: "sensor-pipeline".to_string(),
                    pipeline_ver: None,
                };
                client.apply_resource(apply_request).await
            };
            let (response, _) = futures::join!(requests, server.serve());

            assert!(response.accepted);
            assert_eq!(ApplyMode::try_from(response.mode), Ok(ApplyMode::Started));
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

    fn service() -> DaemonService<NoopWorkerLauncher, InMemoryDaemonStore> {
        DaemonService::with_daemon(TenonDaemon::with_components(
            NoopWorkerLauncher::default(),
            InMemoryDaemonStore::default(),
        ))
    }
}
