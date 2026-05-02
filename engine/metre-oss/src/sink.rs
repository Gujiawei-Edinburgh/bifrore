use metre_core::message::Message;
use metre_core::runtime::RuleMetadata;
use std::collections::HashMap;

pub struct Dispatcher {
    log_sink: Option<LogSink>,
    kafka_sink: Option<KafkaSink>,
    metadata_by_rule_index: HashMap<usize, RuleMetadata>,
}

impl Dispatcher {
    pub fn new(
        log_enabled: bool,
        kafka_enabled: bool,
        rule_metadata: Vec<RuleMetadata>,
    ) -> Self {
        Self {
            log_sink: log_enabled.then_some(LogSink),
            kafka_sink: kafka_enabled.then_some(KafkaSink::default()),
            metadata_by_rule_index: rule_metadata
                .into_iter()
                .map(|metadata| (metadata.rule_index, metadata))
                .collect(),
        }
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
    bootstrap_servers: &'static str,
    topic: &'static str,
}

impl Default for KafkaSink {
    fn default() -> Self {
        Self {
            bootstrap_servers: "127.0.0.1:9092",
            topic: "metre-output",
        }
    }
}

impl KafkaSink {
    fn send(&self, rule_index: usize, message: &Message, metadata: &RuleMetadata) {
        log::info!(
            "kafka sink scaffold bootstrap_servers={} topic={} rule_index={} destinations={:?} payload_bytes={}",
            self.bootstrap_servers,
            self.topic,
            rule_index,
            metadata.destinations,
            message.payload.len()
        );
    }
}
