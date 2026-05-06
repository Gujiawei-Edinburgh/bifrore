mod oss;
mod traits;

pub use oss::OssCoordinator;
pub use traits::{ClientIdSink, ClientIdSource, RuleSource};

pub struct EngineCoordinator {
    rule_source: Box<dyn RuleSource>,
    client_id_source: Box<dyn ClientIdSource>,
    client_id_sink: Box<dyn ClientIdSink>,
}

impl EngineCoordinator {
    pub fn new(
        rule_source: Box<dyn RuleSource>,
        client_id_source: Box<dyn ClientIdSource>,
        client_id_sink: Box<dyn ClientIdSink>,
    ) -> Self {
        Self {
            rule_source,
            client_id_source,
            client_id_sink,
        }
    }

    pub fn from_oss_files(rule_path: String, client_ids_path: String) -> Self {
        OssCoordinator::from_files(rule_path, client_ids_path).into_engine_coordinator()
    }

    pub fn load_rule_bytes(&self) -> Result<Vec<u8>, String> {
        self.rule_source.load_rules()
    }

    pub fn rule_source_label(&self) -> &str {
        self.rule_source.label()
    }

    pub fn resolve_client_ids(&self, node_id: &str, client_count: u16) -> Vec<String> {
        if let Ok(Some(values)) = self.client_id_source.load_client_ids() {
            return values;
        }
        (0..normalize_client_count(client_count))
            .map(|index| format!("{}_{}", node_id, index))
            .collect()
    }

    pub fn persist_client_ids(&self, client_ids: &[String]) -> Result<(), String> {
        self.client_id_sink.persist_client_ids(client_ids)
    }

    pub fn client_id_sink_label(&self) -> &str {
        self.client_id_sink.label()
    }
}

fn normalize_client_count(client_count: u16) -> usize {
    client_count.max(1) as usize
}
