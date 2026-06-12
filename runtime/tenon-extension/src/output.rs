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
#[serde(tag = "op", rename_all = "kebab-case")]
pub enum StateMutation {
    Set {
        key: String,
        value: ExtensionValue,
    },
    Delete {
        key: String,
    },
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct InvocationOutcome {
    pub state_delta: Vec<StateMutation>,
    pub emits: Vec<EmitRecord>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EmitRecord {
    pub payload: ExtensionValue,
}

impl EmitRecord {
    pub fn new(payload: ExtensionValue) -> Self {
        Self { payload }
    }
}
