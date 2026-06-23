use crate::mqtt::{payload_decode_is_json, start_mqtt, IncomingDelivery, MqttAdapterConfig, MqttAdapterHandle};
use crate::{WorkerError, WorkerResult};
use flume::{Receiver, Sender};
use std::thread::JoinHandle;
use tenon_message::plan::{DeploymentPlan, MqttSourcePlan, ResourceId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerConfig {
    pub intake_queue_capacity: usize,
    pub mqtt_io_threads: usize,
    pub mqtt_clean_start: bool,
    pub mqtt_session_expiry_interval: u32,
    pub mqtt_keep_alive_secs: u16,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            intake_queue_capacity: 4096,
            mqtt_io_threads: 2,
            mqtt_clean_start: false,
            mqtt_session_expiry_interval: 3600,
            mqtt_keep_alive_secs: 30,
        }
    }
}

pub struct ActivePipeline {
    mqtt_handles: Vec<MqttAdapterHandle>,
    intake_tx: Sender<IncomingDelivery>,
    process_thread: Option<JoinHandle<()>>,
}

impl ActivePipeline {
    pub fn start(plan: DeploymentPlan, config: WorkerConfig) -> WorkerResult<Self> {
        let pipeline_id = plan
            .id
            .clone()
            .ok_or_else(|| WorkerError::pipeline("deployment plan id is missing"))?;
        let sources = validate_sources(plan.sources)?;
        let (intake_tx, intake_rx) = flume::bounded(config.intake_queue_capacity.max(1));
        let process_thread = Some(start_process_loop(intake_rx));
        let mut mqtt_handles = Vec::with_capacity(sources.len());

        for (source_index, source) in sources.into_iter().enumerate() {
            let handler_tx = intake_tx.clone();
            let adapter_config =
                mqtt_adapter_config(&config, &pipeline_id, source_index, source);
            let handle = start_mqtt(adapter_config, std::sync::Arc::new(move |delivery| {
                if handler_tx.send(delivery).is_err() {
                    eprintln!("worker intake queue is closed; dropping MQTT delivery");
                }
            }))?;
            mqtt_handles.push(handle);
        }

        Ok(Self {
            mqtt_handles,
            intake_tx,
            process_thread,
        })
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
        Ok(())
    }
}

fn start_process_loop(intake_rx: Receiver<IncomingDelivery>) -> JoinHandle<()> {
    std::thread::spawn(move || {
        while let Ok(delivery) = intake_rx.recv() {
            process_delivery(delivery);
        }
    })
}

fn process_delivery(delivery: IncomingDelivery) {
    let _message = &delivery.message;
    delivery.ack();
}

fn mqtt_adapter_config(
    config: &WorkerConfig,
    pipeline_id: &ResourceId,
    source_index: usize,
    source: MqttSourcePlan,
) -> MqttAdapterConfig {
    MqttAdapterConfig {
        pipeline_id: pipeline_id.clone(),
        source_index,
        source,
        group_name: pipeline_id.name.clone(),
        io_threads: config.mqtt_io_threads,
        queue_capacity: config.intake_queue_capacity,
        clean_start: config.mqtt_clean_start,
        session_expiry_interval: config.mqtt_session_expiry_interval,
        keep_alive_secs: config.mqtt_keep_alive_secs,
    }
}

fn validate_sources(sources: Vec<MqttSourcePlan>) -> WorkerResult<Vec<MqttSourcePlan>> {
    if sources.is_empty() {
        return Err(WorkerError::pipeline("deployment plan has no MQTT sources"));
    }
    for source in &sources {
        if source.broker.is_none() {
            return Err(WorkerError::pipeline("MQTT broker plan is missing"));
        }
        if source.subscriptions.is_empty() {
            return Err(WorkerError::pipeline("MQTT source has no subscriptions"));
        }
        for subscription in &source.subscriptions {
            if subscription.topic.trim().is_empty() {
                return Err(WorkerError::pipeline("MQTT subscription topic is empty"));
            }
            if !payload_decode_is_json(subscription.decode) {
                return Err(WorkerError::pipeline("only JSON payload decode is supported"));
            }
        }
    }
    Ok(sources)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tenon_message::plan::{
        ExecutionMode, MqttBrokerPlan, MqttSubscriptionPlan, PayloadDecodePlan,
    };

    #[test]
    fn rejects_pipeline_without_id() {
        let error = match ActivePipeline::start(DeploymentPlan::default(), WorkerConfig::default())
        {
            Ok(_) => panic!("pipeline id should be required"),
            Err(error) => error,
        };

        assert_eq!(error.kind, crate::WorkerErrorKind::Pipeline);
    }

    #[test]
    fn rejects_non_json_decode() {
        let mut plan = plan();
        plan.sources[0].subscriptions[0].decode = PayloadDecodePlan::Unspecified as i32;

        let error = validate_sources(plan.sources).expect_err("decode should be rejected");

        assert_eq!(error.message, "only JSON payload decode is supported");
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
}
