use crate::config::{SinkConfig, SinkDefinition};
use crate::ipc_sink::IpcSink;
use crate::metrics::OssMetrics;
use tenon_core::message::Message;
use tenon_core::runtime::RuleMetadata;
use rdkafka::config::ClientConfig;
use rdkafka::error::{KafkaError, RDKafkaErrorCode};
use rdkafka::producer::{BaseRecord, DefaultProducerContext, Producer, ThreadedProducer};
use rdkafka::util::Timeout;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use std::time::Duration;

pub struct Dispatcher {
    sinks_by_destination: HashMap<String, BuiltInSink>,
    metadata_by_rule_index: HashMap<usize, RuleMetadata>,
    metrics: Arc<OssMetrics>,
}

impl Dispatcher {
    pub fn new(
        sinks: SinkConfig,
        rule_metadata: Vec<RuleMetadata>,
        metrics: Arc<OssMetrics>,
    ) -> Result<Self, String> {
        let used_destinations = used_destinations(&rule_metadata);
        let missing_destinations: Vec<&String> = used_destinations
            .iter()
            .filter(|destination| !sinks.contains_key(*destination))
            .collect();
        if !missing_destinations.is_empty() {
            return Err(format!(
                "rule destinations are not configured as sinks: {}",
                missing_destinations
                    .iter()
                    .map(|destination| destination.as_str())
                    .collect::<Vec<_>>()
                    .join(",")
            ));
        }

        let mut sinks_by_destination = HashMap::new();
        for (destination, definition) in sinks {
            if !used_destinations.contains(&destination) {
                log::info!("skipping unused sink destination={destination}");
                continue;
            }
            let sink = match definition {
                SinkDefinition::Noop => BuiltInSink::Noop(NoopSink {
                    metrics: Arc::clone(&metrics),
                }),
                SinkDefinition::Log => BuiltInSink::Log(LogSink),
                SinkDefinition::Kafka {
                    bootstrap_servers,
                    topic,
                    properties,
                } => BuiltInSink::Kafka(KafkaSink::new(
                    destination.as_str(),
                    bootstrap_servers,
                    topic,
                    properties,
                    Arc::clone(&metrics),
                )?),
                SinkDefinition::Ipc {
                    path,
                    queue_capacity,
                    batch_max_messages,
                    batch_max_bytes,
                    flush_interval_millis,
                } => BuiltInSink::Ipc(IpcSink::new(
                    path,
                    queue_capacity,
                    batch_max_messages,
                    batch_max_bytes,
                    flush_interval_millis,
                    Arc::clone(&metrics),
                )?),
            };
            sinks_by_destination.insert(destination, sink);
        }

        Ok(Self {
            sinks_by_destination,
            metadata_by_rule_index: rule_metadata
                .into_iter()
                .map(|metadata| (metadata.rule_index, metadata))
                .collect(),
            metrics,
        })
    }

    pub fn dispatch(&self, rule_index: usize, message: &Message) {
        let Some(metadata) = self.metadata_by_rule_index.get(&rule_index) else {
            log::warn!("dropping result for unknown rule_index={rule_index}");
            return;
        };
        for destination in &metadata.destinations {
            let Some(sink) = self.sinks_by_destination.get(destination) else {
                self.metrics.record_sink_unsupported_destination();
                log::warn!("unsupported destination={destination} rule_index={rule_index}");
                continue;
            };
            sink.send(destination, rule_index, message, metadata);
        }
    }
}

fn used_destinations(rule_metadata: &[RuleMetadata]) -> HashSet<String> {
    rule_metadata
        .iter()
        .flat_map(|metadata| metadata.destinations.iter().cloned())
        .collect()
}

enum BuiltInSink {
    Noop(NoopSink),
    Log(LogSink),
    Kafka(KafkaSink),
    Ipc(IpcSink),
}

impl BuiltInSink {
    fn send(&self, destination: &str, rule_index: usize, message: &Message, metadata: &RuleMetadata) {
        match self {
            Self::Noop(sink) => sink.send(),
            Self::Log(sink) => sink.send(destination, rule_index, message, metadata),
            Self::Kafka(sink) => sink.send(rule_index, message, metadata),
            Self::Ipc(sink) => sink.send(destination, rule_index, message),
        }
    }
}

struct NoopSink {
    metrics: Arc<OssMetrics>,
}

impl NoopSink {
    fn send(&self) {
        self.metrics.record_noop_sink_message();
    }
}

struct LogSink;

impl LogSink {
    fn send(&self, destination: &str, rule_index: usize, message: &Message, metadata: &RuleMetadata) {
        log::info!(
            "tenon result destination={} rule_index={} destinations={:?} topic={} payload={}",
            destination,
            rule_index,
            metadata.destinations,
            message.topic,
            String::from_utf8_lossy(&message.payload)
        );
    }
}

struct KafkaSink {
    destination: String,
    bootstrap_servers: Vec<String>,
    topic: String,
    properties: HashMap<String, String>,
    producer: ThreadedProducer<DefaultProducerContext>,
    metrics: Arc<OssMetrics>,
}

impl KafkaSink {
    fn new(
        destination: &str,
        bootstrap_servers: Vec<String>,
        topic: String,
        properties: HashMap<String, String>,
        metrics: Arc<OssMetrics>,
    ) -> Result<Self, String> {
        if bootstrap_servers.is_empty() {
            return Err(format!(
                "sinks.{destination}.bootstrap_servers must not be empty"
            ));
        }
        if topic.is_empty() {
            return Err(format!("sinks.{destination}.topic must not be empty"));
        }
        let mut client_config = ClientConfig::new();
        client_config.set("bootstrap.servers", bootstrap_servers.join(","));
        for (key, value) in &properties {
            client_config.set(key, value);
        }
        let producer = client_config
            .create()
            .map_err(|err| format!("failed to create kafka producer for sink {destination}: {err}"))?;
        Ok(Self {
            destination: destination.to_string(),
            bootstrap_servers,
            topic,
            properties,
            producer,
            metrics,
        })
    }

    fn send(&self, rule_index: usize, message: &Message, metadata: &RuleMetadata) {
        let record = BaseRecord::to(&self.topic)
            .key(&message.topic)
            .payload(&message.payload);
        match self.producer.send(record) {
            Ok(()) => self.metrics.record_kafka_enqueue(),
            Err((err, _record)) => match err {
                KafkaError::MessageProduction(RDKafkaErrorCode::QueueFull) => {
                    self.metrics.record_kafka_queue_full();
                    log::warn!(
                        "kafka producer queue full destination={} topic={} rule_index={} destinations={:?} payload_bytes={}",
                        self.destination,
                        self.topic,
                        rule_index,
                        metadata.destinations,
                        message.payload.len()
                    );
                }
                other => {
                    self.metrics.record_kafka_enqueue_error();
                    log::error!(
                        "kafka producer enqueue failed destination={} topic={} rule_index={} destinations={:?} error={}",
                        self.destination,
                        self.topic,
                        rule_index,
                        metadata.destinations,
                        other
                    );
                }
            },
        }
    }
}

impl Drop for KafkaSink {
    fn drop(&mut self) {
        log::info!(
            "flushing kafka producer destination={} bootstrap_servers={} topic={} properties={}",
            self.destination,
            self.bootstrap_servers.join(","),
            self.topic,
            self.properties.len()
        );
        let _ = self.producer.flush(Timeout::After(Duration::from_secs(5)));
    }
}
