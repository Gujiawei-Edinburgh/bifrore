mod context;
mod error;
mod message;
mod output;

pub use context::{Context, EmitBuffer, State};
pub use error::{ExtensionError, ExtensionErrorKind, ExtensionResult};
pub use message::{Message, MqttMetadata, SourceContext, Topic};
pub use output::{AuthResult, EmitRecord, ExtensionValue};

pub const AUTH_CREDENTIALS_FN: &str = "credentials";
pub const PROCESS_ON_MESSAGE_FN: &str = "on_message";

pub const CONTEXT_ARG: &str = "ctx";
pub const MESSAGE_ARG: &str = "msg";

pub const CONTEXT_STATE_FIELD: &str = "state";
pub const CONTEXT_EMIT_FN: &str = "emit";

pub const STATE_GET_FN: &str = "get";
pub const STATE_SET_FN: &str = "set";
pub const STATE_DELETE_FN: &str = "delete";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExtensionPoint {
    AuthCredentials,
    ProcessOnMessage,
}

impl ExtensionPoint {
    pub fn function_name(self) -> &'static str {
        match self {
            Self::AuthCredentials => AUTH_CREDENTIALS_FN,
            Self::ProcessOnMessage => PROCESS_ON_MESSAGE_FN,
        }
    }
}
