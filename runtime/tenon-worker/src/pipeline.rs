use crate::egress::{Egress, EgressConfig, EgressRuntime};
use crate::mqtt::{start_mqtt, IncomingDelivery, MqttAdapterConfig, MqttAdapterHandle};
use crate::processor::{processor_from_plan, Processor};
use crate::{WorkerComponent, WorkerError, WorkerFailure, WorkerMetrics, WorkerResult, WorkerSupervisor};
use flume::{Receiver, Sender};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use tenon_extension::{Context, SourceContext};
use tenon_message::plan::{DeploymentPlan, MqttSourceClientIds, MqttSourcePlan, ResourceId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerConfig {
    pub intake_queue_capacity: usize,
    pub mqtt_io_threads: usize,
    pub mqtt_clean_start: bool,
    pub mqtt_session_expiry_interval: u32,
    pub mqtt_keep_alive_secs: u16,
    pub egress_queue_capacity: usize,
    pub egress_socket_path: Option<PathBuf>,
    pub egress_send_timeout: Duration,
    pub detailed_metrics: bool,
    pub process_reload_timeout: Duration,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            intake_queue_capacity: 4096,
            mqtt_io_threads: 2,
            mqtt_clean_start: false,
            mqtt_session_expiry_interval: 3600,
            mqtt_keep_alive_secs: 30,
            egress_queue_capacity: 4096,
            egress_socket_path: None,
            egress_send_timeout: Duration::from_millis(5),
            detailed_metrics: true,
            process_reload_timeout: Duration::from_secs(10),
        }
    }
}

pub struct ActivePipeline {
    plan: DeploymentPlan,
    mqtt_handles: Vec<MqttAdapterHandle>,
    intake_tx: Sender<IncomingDelivery>,
    process_tx: Sender<ProcessCommand>,
    process_thread: Option<JoinHandle<()>>,
    egress: Option<EgressRuntime>,
    metrics: Arc<WorkerMetrics>,
    process_reload_timeout: Duration,
    supervisor: WorkerSupervisor,
}

impl ActivePipeline {
    pub fn start(
        plan: DeploymentPlan,
        source_client_ids: Vec<MqttSourceClientIds>,
        config: WorkerConfig,
    ) -> WorkerResult<Self> {
        let pipeline_id = plan
            .id
            .clone()
            .ok_or_else(|| WorkerError::pipeline("deployment plan id is missing"))?;
        let context = Context::with_empty_memory(SourceContext::new(
            pipeline_id.name.clone(),
            pipeline_id.version.clone(),
        ));
        let processor = processor_from_plan(plan.process.clone(), context)?;
        let metrics = Arc::new(WorkerMetrics::new(config.detailed_metrics));
        let (failure_tx, supervisor) = WorkerSupervisor::start();
        let egress = EgressRuntime::start(
            plan.egress.clone(),
            EgressConfig {
                queue_capacity: config.egress_queue_capacity,
                socket_path: config.egress_socket_path.clone().unwrap_or_else(default_egress_socket_path),
                send_timeout: config.egress_send_timeout,
                ..EgressConfig::default()
            },
            Arc::clone(&metrics),
            Some(failure_tx.clone())
        );
        let egress = match egress {
            Ok(egress) => egress,
            Err(error) => {
                supervisor.stop();
                return Err(error);
            }
        };
        let process_egress = egress.egress();
        let sources = plan.sources.clone();
        let (intake_tx, intake_rx) = flume::bounded(config.intake_queue_capacity.max(1));
        let (process_tx, process_rx) = flume::bounded(1);
        let process_thread = Some(start_process_loop(
            intake_rx,
            process_rx,
            processor,
            process_egress,
            Arc::clone(&metrics),
            failure_tx.clone(),
        ));
        let mut mqtt_handles: Vec<MqttAdapterHandle> = Vec::with_capacity(sources.len());

        for (source_index, source) in sources.into_iter().enumerate() {
            let handler_tx = intake_tx.clone();
            let adapter_config = mqtt_adapter_config(
                &config,
                &pipeline_id,
                source,
                source_client_ids
                    .get(source_index)
                    .map(|group| group.client_ids.clone())
                    .unwrap_or_default(),
            );
            let handle = match start_mqtt(
                adapter_config,
                std::sync::Arc::new(move |delivery| {
                    if handler_tx.send(delivery).is_err() {
                        log::error!("worker intake queue is closed; dropping MQTT delivery");
                    }
                }),
                failure_tx.clone(),
            ) {
                Ok(handle) => handle,
                Err(error) => {
                    for handle in mqtt_handles {
                        let _ = handle.stop();
                    }
                    drop(intake_tx);
                    drop(process_tx);
                    if let Some(process_thread) = process_thread {
                        let _ = process_thread.join();
                    }
                    let _ = egress.stop();
                    supervisor.stop();
                    return Err(error);
                }
            };
            mqtt_handles.push(handle);
        }

        Ok(Self {
            plan,
            mqtt_handles,
            intake_tx,
            process_tx,
            process_thread,
            egress: Some(egress),
            metrics,
            process_reload_timeout: config.process_reload_timeout,
            supervisor,
        })
    }

    pub fn reload_process(&mut self, plan: DeploymentPlan) -> WorkerResult<()> {
        if self.plan.id.as_ref().map(|id| &id.name) != plan.id.as_ref().map(|id| &id.name) {
            return Err(WorkerError::pipeline(
                "reload plan does not target the active pipeline",
            ));
        }
        if self.plan.sources != plan.sources || self.plan.egress != plan.egress {
            return Err(WorkerError::pipeline(
                "worker reload only supports process changes",
            ));
        }
        let process = plan
            .process
            .clone()
            .ok_or_else(|| WorkerError::pipeline("process plan is missing"))?;
        let (reload_tx, reload_rx) = flume::bounded(1);
        self.process_tx
            .send(ProcessCommand::Reload {
                process,
                completion: reload_tx,
            })
            .map_err(|_| WorkerError::pipeline("process thread is not running"))?;
        reload_rx
            .recv_timeout(self.process_reload_timeout)
            .map_err(|error| {
                WorkerError::pipeline(format!(
                    "timed out waiting for process reload completion: {error}"
                ))
            })??;
        self.plan.process = plan.process;
        self.plan.id = plan.id;
        Ok(())
    }

    pub fn metrics(&self) -> &Arc<WorkerMetrics> {
        &self.metrics
    }

    pub fn failure(&self) -> Option<WorkerFailure> {
        self.supervisor.failure()
    }

    pub fn stop(mut self) -> WorkerResult<()> {
        for handle in self.mqtt_handles.drain(..) {
            handle.stop()?;
        }
        drop(self.intake_tx);
        if let Some(process_thread) = self.process_thread.take() {
            process_thread
                .join()
                .map_err(|_| WorkerError::pipeline("process thread panicked"))?;
        }
        if let Some(egress) = self.egress.take() {
            egress.stop()?;
        }
        self.supervisor.stop();
        Ok(())
    }
}

fn default_egress_socket_path() -> PathBuf {
    std::env::temp_dir().join(format!("tenon-worker-egress-{}.sock", std::process::id()))
}

enum ProcessCommand {
    Reload {
        process: tenon_message::plan::ProcessPlan,
        completion: Sender<WorkerResult<()>>,
    },
}

fn start_process_loop(
    intake_rx: Receiver<IncomingDelivery>,
    process_rx: Receiver<ProcessCommand>,
    mut processor: Box<dyn Processor>,
    egress: Egress,
    metrics: Arc<WorkerMetrics>,
    failure_tx: Sender<WorkerFailure>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            loop {
                match flume::Selector::new()
                    .recv(&process_rx, ProcessEvent::Command)
                    .recv(&intake_rx, ProcessEvent::Delivery)
                    .wait()
                {
                    ProcessEvent::Command(Ok(command)) => {
                        processor = handle_process_command(processor, command);
                    }
                    ProcessEvent::Command(Err(_)) => break,
                    ProcessEvent::Delivery(Ok(delivery)) => {
                        process_delivery(&mut *processor, delivery, &egress, &metrics)
                    }
                    ProcessEvent::Delivery(Err(_)) => break,
                }
            }
        }));
        if result.is_err() {
            let _ = failure_tx.send(WorkerFailure::fatal(
                WorkerComponent::Processor,
                "processor thread panicked",
            ));
        }
    })
}

enum ProcessEvent {
    Command(Result<ProcessCommand, flume::RecvError>),
    Delivery(Result<IncomingDelivery, flume::RecvError>),
}

fn handle_process_command(
    processor: Box<dyn Processor>,
    command: ProcessCommand,
) -> Box<dyn Processor> {
    match command {
        ProcessCommand::Reload {
            process,
            completion,
        } => {
            let result = replace_processor(processor.as_ref(), process);
            match result {
                Ok(processor) => {
                    let _ = completion.send(Ok(()));
                    processor
                }
                Err(error) => {
                    let _ = completion.send(Err(error.clone()));
                    log::error!("process reload failed: {error}");
                    processor
                }
            }
        }
    }
}

fn replace_processor(
    processor: &dyn Processor,
    process: tenon_message::plan::ProcessPlan,
) -> WorkerResult<Box<dyn Processor>> {
    let context = processor.context().clone();
    processor_from_plan(Some(process), context)
}

fn process_delivery(
    processor: &mut dyn Processor,
    delivery: IncomingDelivery,
    egress: &Egress,
    metrics: &WorkerMetrics,
) {
    let topic = delivery.message.topic.raw.clone();
    let packet_id = delivery.message.metadata.pkid;
    match processor.process(&delivery.message) {
        Ok(outcome) => {
            metrics.record_processed_message();
            metrics.record_emitted_records(outcome.emits.len());
            delivery.ack();
            egress.dispatch(outcome, metrics);
        }
        Err(error) => {
            log::error!(
                "dropping message after processor error packet_id={} topic={} error={}",
                packet_id,
                topic,
                error
            );
            metrics.record_processor_error();
            delivery.ack();
        }
    }
}

fn mqtt_adapter_config(
    config: &WorkerConfig,
    pipeline_id: &ResourceId,
    source: MqttSourcePlan,
    client_ids: Vec<String>,
) -> MqttAdapterConfig {
    MqttAdapterConfig {
        pipeline_id: pipeline_id.clone(),
        source,
        client_ids,
        group_name: pipeline_id.name.clone(),
        io_threads: config.mqtt_io_threads,
        queue_capacity: config.intake_queue_capacity,
        clean_start: config.mqtt_clean_start,
        session_expiry_interval: config.mqtt_session_expiry_interval,
        keep_alive_secs: config.mqtt_keep_alive_secs,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::sync::atomic::{AtomicU64, Ordering};
    use tenon_extension::{
        Context, EmitRecord, InvocationOutcome, Message, MqttMetadata, SourceContext, Topic,
    };
    use tenon_message::plan::{
        EgressPlan, ExecutionMode, MqttBrokerPlan, MqttSubscriptionPlan, PayloadDecodePlan,
    };

    static SOCKET_SEQ: AtomicU64 = AtomicU64::new(0);

    #[test]
    fn rejects_pipeline_without_id() {
        let error = match ActivePipeline::start(
            DeploymentPlan::default(),
            Vec::new(),
            WorkerConfig::default(),
        ) {
            Ok(_) => panic!("pipeline id should be required"),
            Err(error) => error,
        };

        assert_eq!(error.kind, crate::WorkerErrorKind::Pipeline);
    }

    #[test]
    fn reload_process_rejects_source_changes() {
        let plan = plan();
        let (_, supervisor) = WorkerSupervisor::start();
        let mut pipeline = ActivePipeline {
            plan: plan.clone(),
            mqtt_handles: Vec::new(),
            intake_tx: flume::bounded(1).0,
            process_tx: flume::bounded(1).0,
            process_thread: None,
            egress: None,
            metrics: Arc::new(WorkerMetrics::default()),
            process_reload_timeout: Duration::from_secs(10),
            supervisor,
        };
        let mut target = plan;
        target.sources[0].client_count = 2;

        let error = pipeline
            .reload_process(target)
            .expect_err("source changes should be rejected");

        assert_eq!(error.message, "worker reload only supports process changes");
    }

    #[test]
    fn process_delivery_acks_then_dispatches_emits() {
        let mut processor = FakeProcessor::ok(2);
        let metrics = WorkerMetrics::default();
        let egress_runtime = egress_runtime(2);
        let egress = egress_runtime.egress();

        process_delivery(&mut processor, delivery(), &egress, &metrics);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.processed_messages, 1);
        assert_eq!(snapshot.processor_errors, 0);
        assert_eq!(snapshot.emitted_records, 2);
        assert_eq!(snapshot.egress_enqueued_records, 2);
        drop(egress);
        egress_runtime.stop().expect("egress stop");
    }

    #[test]
    fn process_delivery_drops_bad_message_and_does_not_dispatch() {
        let mut processor = FakeProcessor::err();
        let metrics = WorkerMetrics::default();
        let egress_runtime = egress_runtime(1);
        let egress = egress_runtime.egress();

        process_delivery(&mut processor, delivery(), &egress, &metrics);

        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.processed_messages, 0);
        assert_eq!(snapshot.processor_errors, 1);
        assert_eq!(snapshot.emitted_records, 0);
        assert_eq!(snapshot.egress_enqueued_records, 0);
        drop(egress);
        egress_runtime.stop().expect("egress stop");
    }

    fn plan() -> DeploymentPlan {
        DeploymentPlan {
            id: Some(ResourceId {
                name: "sensor".to_string(),
                version: "r1".to_string(),
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
                client_count: 1,
            }],
            process: None,
            egress: None,
        }
    }

    fn delivery() -> IncomingDelivery {
        IncomingDelivery::without_ack(Message::new(
            SourceContext::new("sensor", "r1"),
            Topic::new("sensor/a/data"),
            json!({"temp": 30}),
            br#"{"temp":30}"#.to_vec(),
            MqttMetadata::new(7, 1, false, false),
            std::collections::HashMap::new(),
        ))
    }

    fn egress_runtime(capacity: usize) -> EgressRuntime {
        EgressRuntime::start(
            Some(EgressPlan {}),
            EgressConfig {
                queue_capacity: capacity,
                socket_path: PathBuf::from("/private/tmp").join(format!(
                    "tenon-worker-pipeline-test-{}-{}.sock",
                    std::process::id(),
                    SOCKET_SEQ.fetch_add(1, Ordering::Relaxed)
                )),
                ..EgressConfig::default()
            },
            Arc::new(WorkerMetrics::default()),
            None,
        )
        .expect("egress runtime")
    }

    struct FakeProcessor {
        emits: usize,
        error: bool,
    }

    impl FakeProcessor {
        fn ok(emits: usize) -> Self {
            Self {
                emits,
                error: false,
            }
        }

        fn err() -> Self {
            Self {
                emits: 0,
                error: true,
            }
        }
    }

    impl Processor for FakeProcessor {
        fn process(&mut self, _message: &Message) -> WorkerResult<InvocationOutcome> {
            if self.error {
                return Err(WorkerError::processor("test processor error"));
            }
            Ok(InvocationOutcome {
                emits: (0..self.emits)
                    .map(|index| EmitRecord::new(json!({ "index": index })))
                    .collect(),
            })
        }

        fn into_context(self: Box<Self>) -> Context {
            Context::with_empty_memory(SourceContext::new("sensor", "r1"))
        }

        fn context(&self) -> &Context {
            panic!("fake processor context is not used by these tests")
        }
    }
}
