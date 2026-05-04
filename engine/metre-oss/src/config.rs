use serde::Deserialize;
use std::collections::HashMap;

#[derive(Debug, Deserialize)]
pub struct OssConfig {
    pub rule_json_path: String,
    #[serde(default = "default_client_ids_path")]
    pub client_ids_path: String,
    #[serde(default)]
    pub payload: PayloadConfig,
    #[serde(default)]
    pub mqtt: MqttOssConfig,
    #[serde(default)]
    pub sinks: SinkConfig,
    #[serde(default)]
    pub metrics: MetricsConfig,
}

#[derive(Debug, Deserialize)]
pub struct PayloadConfig {
    #[serde(default = "default_payload_format")]
    pub format: String,
    pub protobuf_descriptor_set_path: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct MqttOssConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub node_id: Option<String>,
    #[serde(default = "default_client_count")]
    pub client_count: u16,
    pub username: Option<String>,
    pub password: Option<String>,
    #[serde(default = "default_clean_start")]
    pub clean_start: bool,
    #[serde(default = "default_session_expiry_interval")]
    pub session_expiry_interval: u32,
    #[serde(default = "default_group_name")]
    pub group_name: String,
    #[serde(default)]
    pub ordered: bool,
    #[serde(default)]
    pub ordered_prefix: String,
    #[serde(default = "default_keep_alive_secs")]
    pub keep_alive_secs: u16,
    #[serde(default = "default_io_threads")]
    pub io_threads: u16,
    #[serde(default = "default_queue_capacity")]
    pub queue_capacity: u16,
}

#[derive(Debug, Deserialize)]
pub struct SinkConfig {
    pub noop: Option<NoopSinkConfig>,
    pub log: Option<LogSinkConfig>,
    pub kafka: Option<KafkaSinkConfig>,
}

#[derive(Debug, Deserialize)]
pub struct NoopSinkConfig {}

#[derive(Debug, Deserialize)]
pub struct LogSinkConfig {}

#[derive(Debug, Deserialize)]
pub struct KafkaSinkConfig {
    pub bootstrap_servers: Vec<String>,
    pub topic: String,
    #[serde(default)]
    pub properties: HashMap<String, String>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct MetricsConfig {
    #[serde(default)]
    pub detailed_latency: bool,
}

impl Default for PayloadConfig {
    fn default() -> Self {
        Self {
            format: default_payload_format(),
            protobuf_descriptor_set_path: None,
        }
    }
}

impl Default for MqttOssConfig {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            node_id: None,
            client_count: default_client_count(),
            username: None,
            password: None,
            clean_start: default_clean_start(),
            session_expiry_interval: default_session_expiry_interval(),
            group_name: default_group_name(),
            ordered: false,
            ordered_prefix: String::new(),
            keep_alive_secs: default_keep_alive_secs(),
            io_threads: default_io_threads(),
            queue_capacity: default_queue_capacity(),
        }
    }
}

impl Default for SinkConfig {
    fn default() -> Self {
        Self {
            noop: None,
            log: None,
            kafka: None,
        }
    }
}

impl Default for MetricsConfig {
    fn default() -> Self {
        Self {
            detailed_latency: false,
        }
    }
}

impl OssConfig {
    pub fn normalize(mut self) -> Self {
        let client_ids_path = self.client_ids_path.trim();
        self.client_ids_path = if client_ids_path.is_empty() {
            default_client_ids_path()
        } else {
            client_ids_path.to_string()
        };
        self
    }
}

fn default_client_ids_path() -> String {
    "./client_ids".to_string()
}

fn default_payload_format() -> String {
    "json".to_string()
}

fn default_host() -> String {
    "127.0.0.1".to_string()
}

fn default_port() -> u16 {
    1883
}

fn default_client_count() -> u16 {
    1
}

fn default_clean_start() -> bool {
    true
}

fn default_session_expiry_interval() -> u32 {
    3600
}

fn default_group_name() -> String {
    "metre-oss".to_string()
}

fn default_keep_alive_secs() -> u16 {
    30
}

fn default_io_threads() -> u16 {
    2
}

fn default_queue_capacity() -> u16 {
    4096
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_client_ids_path_uses_default() {
        let config: OssConfig = serde_json::from_str(
            r#"{
              "rule_json_path": "rules.json"
            }"#,
        )
        .unwrap();

        assert_eq!(config.normalize().client_ids_path, "./client_ids");
    }

    #[test]
    fn blank_client_ids_path_uses_default() {
        let config: OssConfig = serde_json::from_str(
            r#"{
              "rule_json_path": "rules.json",
              "client_ids_path": "   "
            }"#,
        )
        .unwrap();

        assert_eq!(config.normalize().client_ids_path, "./client_ids");
    }
}
