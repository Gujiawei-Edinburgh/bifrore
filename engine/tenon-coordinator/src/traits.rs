pub trait RuleSource: Send + Sync {
    fn load_rules(&self) -> Result<Vec<u8>, String>;

    fn label(&self) -> &str;
}

pub trait ClientIdSource: Send + Sync {
    fn load_client_ids(&self) -> Result<Option<Vec<String>>, String>;
}

pub trait ClientIdSink: Send + Sync {
    fn persist_client_ids(&self, client_ids: &[String]) -> Result<(), String>;

    fn label(&self) -> &str;
}
