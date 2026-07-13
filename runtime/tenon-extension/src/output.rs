use serde::{Deserialize, Serialize};

pub type ExtensionValue = serde_json::Value;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "kebab-case")]
pub enum AuthResult {
    UsernamePassword { username: String, password: String },
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

#[derive(Debug, Clone, Default, PartialEq)]
pub struct InvocationOutcome {
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
