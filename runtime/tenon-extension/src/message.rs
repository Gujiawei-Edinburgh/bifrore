use crate::{ExtensionValue, ScriptApi};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceContext {
    pub name: String,
    pub version: String,
}

impl SourceContext {
    pub fn new(name: impl Into<String>, version: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: version.into(),
        }
    }
}

impl ScriptApi for SourceContext {
    const FIELDS: &'static [&'static str] = &["name", "version"];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Topic {
    pub raw: String,
    pub levels: Vec<String>,
}

impl Topic {
    pub fn new(raw: impl Into<String>) -> Self {
        let raw = raw.into();
        let levels = raw.split('/').map(str::to_string).collect();
        Self { raw, levels }
    }

    pub fn lua_level(&self, one_based_index: usize) -> Option<&str> {
        one_based_index
            .checked_sub(1)
            .and_then(|index| self.levels.get(index))
            .map(String::as_str)
    }
}

impl ScriptApi for Topic {
    const FIELDS: &'static [&'static str] = &["raw", "levels"];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MqttMetadata {
    pub pkid: u16,
    pub qos: u8,
    pub retain: bool,
    pub dup: bool,
}

impl MqttMetadata {
    pub fn new(pkid: u16, qos: u8, retain: bool, dup: bool) -> Self {
        Self {
            pkid,
            qos,
            retain,
            dup,
        }
    }
}

impl ScriptApi for MqttMetadata {
    const FIELDS: &'static [&'static str] = &["pkid", "qos", "retain", "dup"];
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Message {
    pub source: SourceContext,
    pub topic: Topic,
    pub payload: ExtensionValue,
    pub raw_payload: Vec<u8>,
    pub metadata: MqttMetadata,
    pub properties: HashMap<String, String>,
}

impl Message {
    pub fn new(
        source: SourceContext,
        topic: Topic,
        payload: ExtensionValue,
        raw_payload: Vec<u8>,
        metadata: MqttMetadata,
        properties: HashMap<String, String>,
    ) -> Self {
        Self {
            source,
            topic,
            payload,
            raw_payload,
            metadata,
            properties,
        }
    }
}

impl ScriptApi for Message {
    const FIELDS: &'static [&'static str] = &[
        "source",
        "topic",
        "payload",
        "raw_payload",
        "metadata",
        "properties",
    ];
}
