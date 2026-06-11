use serde::{Deserialize, Serialize};

pub type ExtensionValue = serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum AuthResult {
    UsernamePassword { username: String, password: String },
}

impl AuthResult {
    pub fn username_password(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self::UsernamePassword {
            username: username.into(),
            password: password.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmitRecord {
    pub channel: String,
    pub payload: ExtensionValue,
}

impl EmitRecord {
    pub fn new(channel: impl Into<String>, payload: ExtensionValue) -> Self {
        Self {
            channel: channel.into(),
            payload,
        }
    }
}
