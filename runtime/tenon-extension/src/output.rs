use serde::{Deserialize, Serialize};

pub type ExtensionValue = serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum AuthResult {
    UsernamePassword { username: String, password: String },
    BearerToken { token: String },
    ClientCertificate {
        cert_path: String,
        key_path: String,
        ca_path: Option<String>,
    },
    Custom {
        username: Option<String>,
        password: Option<String>,
        properties: Vec<(String, String)>,
    },
}

impl AuthResult {
    pub fn username_password(username: impl Into<String>, password: impl Into<String>) -> Self {
        Self::UsernamePassword {
            username: username.into(),
            password: password.into(),
        }
    }

    pub fn bearer_token(token: impl Into<String>) -> Self {
        Self::BearerToken {
            token: token.into(),
        }
    }

    pub fn client_certificate(
        cert_path: impl Into<String>,
        key_path: impl Into<String>,
        ca_path: Option<String>,
    ) -> Self {
        Self::ClientCertificate {
            cert_path: cert_path.into(),
            key_path: key_path.into(),
            ca_path,
        }
    }

    pub fn custom(
        username: Option<String>,
        password: Option<String>,
        properties: Vec<(String, String)>,
    ) -> Self {
        Self::Custom {
            username,
            password,
            properties,
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
