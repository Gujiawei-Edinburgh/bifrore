use crate::config::{KafkaSinkConfig, SinkConfig};
use metre_core::message::Message;
use metre_core::runtime::RuleMetadata;
use rdkafka::config::ClientConfig;
use rdkafka::error::{KafkaError, RDKafkaErrorCode};
use rdkafka::producer::{BaseRecord, DefaultProducerContext, Producer, ThreadedProducer};
use rdkafka::util::Timeout;
use std::collections::HashMap;
use std::time::Duration;

pub struct Dispatcher {
    log_sink: Option<LogSink>,
    kafka_sink: Option<KafkaSink>,
    metadata_by_rule_index: HashMap<usize, RuleMetadata>,
}

impl Dispatcher {
    pub fn new(sinks: SinkConfig, rule_metadata: Vec<RuleMetadata>) -> Result<Self, String> {
        Ok(Self {
            log_sink: sinks.log.map(|_| LogSink),
            kafka_sink: sinks.kafka.map(KafkaSink::new).transpose()?,
            metadata_by_rule_index: rule_metadata
                .into_iter()
                .map(|metadata| (metadata.rule_index, metadata))
                .collect(),
        })
    }

    pub fn dispatch(&self, rule_index: usize, message: &Message) {
        let Some(metadata) = self.metadata_by_rule_index.get(&rule_index) else {
            log::warn!("dropping result for unknown rule_index={rule_index}");
            return;
        };
        for destination in &metadata.destinations {
            match destination.as_str() {
                "log" => {
                    if let Some(sink) = &self.log_sink {
                        sink.send(rule_index, message, metadata);
                    }
                }
                "kafka" => {
                    if let Some(sink) = &self.kafka_sink {
                        sink.send(rule_index, message, metadata);
                    }
                }
                other => {
                    log::warn!("unsupported destination={other} rule_index={rule_index}");
                }
            }
        }
    }
}

struct LogSink;

impl LogSink {
    fn send(&self, rule_index: usize, message: &Message, metadata: &RuleMetadata) {
        log::info!(
            "metre result rule_index={} destinations={:?} topic={} payload={}",
            rule_index,
            metadata.destinations,
            message.topic,
            String::from_utf8_lossy(&message.payload)
        );
    }
}

struct KafkaSink {
    bootstrap_servers: Vec<String>,
    topic: String,
    properties: HashMap<String, String>,
    producer: ThreadedProducer<DefaultProducerContext>,
}

impl KafkaSink {
    fn new(config: KafkaSinkConfig) -> Result<Self, String> {
        if config.bootstrap_servers.is_empty() {
            return Err("sinks.kafka.bootstrap_servers must not be empty".to_string());
        }
        if config.topic.is_empty() {
            return Err("sinks.kafka.topic must not be empty".to_string());
        }
        let mut client_config = ClientConfig::new();
        client_config.set("bootstrap.servers", config.bootstrap_servers.join(","));
        for (key, value) in &config.properties {
            client_config.set(key, value);
        }
        let producer = client_config
            .create()
            .map_err(|err| format!("failed to create kafka producer: {err}"))?;
        Ok(Self {
            bootstrap_servers: config.bootstrap_servers,
            topic: config.topic,
            properties: config.properties,
            producer,
        })
    }

    fn send(&self, rule_index: usize, message: &Message, metadata: &RuleMetadata) {
        let record = BaseRecord::to(&self.topic)
            .key(&message.topic)
            .payload(&message.payload);
        if let Err((err, _record)) = self.producer.send(record) {
            match err {
                KafkaError::MessageProduction(RDKafkaErrorCode::QueueFull) => {
                    log::warn!(
                        "kafka producer queue full topic={} rule_index={} destinations={:?} payload_bytes={}",
                        self.topic,
                        rule_index,
                        metadata.destinations,
                        message.payload.len()
                    );
                }
                other => {
                    log::error!(
                        "kafka producer enqueue failed topic={} rule_index={} destinations={:?} error={}",
                        self.topic,
                        rule_index,
                        metadata.destinations,
                        other
                    );
                }
            }
        }
    }
}

impl Drop for KafkaSink {
    fn drop(&mut self) {
        log::info!(
            "flushing kafka producer bootstrap_servers={} topic={} properties={}",
            self.bootstrap_servers.join(","),
            self.topic,
            self.properties.len()
        );
        let _ = self.producer.flush(Timeout::After(Duration::from_secs(5)));
    }
}
