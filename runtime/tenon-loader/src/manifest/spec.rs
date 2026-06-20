use serde::Deserialize;
use tenon_extension::{AUTH_CREDENTIALS_FN, PROCESS_ON_MESSAGE_FN};

use crate::{lua, DeliveryMode, LoaderError, LoaderErrorKind};

use super::ManifestResourceKind;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MqttSourceSpec {
    pub(crate) broker: MqttBrokerSpec,
    #[serde(default = "default_client_count")]
    pub(crate) client_count: u32,
    #[serde(default)]
    pub(crate) auth: Option<AuthSpec>,
    pub(crate) subscriptions: Vec<MqttSubscriptionSpec>,
}

impl MqttSourceSpec {
    pub(crate) fn validate(&self) -> Result<(), LoaderError> {
        require_non_empty("broker.host", &self.broker.host)?;
        if self.broker.port == 0 {
            return Err(resource_error("broker.port must be greater than 0"));
        }
        if self.client_count == 0 {
            return Err(resource_error("clientCount must be greater than 0"));
        }
        if self.subscriptions.is_empty() {
            return Err(resource_error("MqttSource must declare at least one subscription"));
        }
        for subscription in &self.subscriptions {
            subscription.validate()?;
        }
        if let Some(auth) = &self.auth {
            auth.validate()?;
        }
        Ok(())
    }
}

fn default_client_count() -> u32 {
    1
}

#[derive(Debug, Deserialize)]
pub(crate) struct MqttBrokerSpec {
    pub(crate) host: String,
    pub(crate) port: u16,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
pub(crate) enum AuthSpec {
    Static {
        username: String,
        password: String,
    },
    Script {
        script: InlineScriptSpec,
    },
}

impl AuthSpec {
    pub(crate) fn validate(&self) -> Result<(), LoaderError> {
        match self {
            Self::Static { username, password } => {
                require_non_empty("auth.username", username)?;
                require_non_empty("auth.password", password)
            }
            Self::Script { script } => {
                script.validate()?;
                Ok(())
            }
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct InlineScriptSpec {
    pub(crate) runtime: ScriptRuntimeSpec,
    pub(crate) source: String,
}

impl InlineScriptSpec {
    fn validate(&self) -> Result<(), LoaderError> {
        match self.runtime {
            ScriptRuntimeSpec::Lua => {
                validate_lua_function("auth.script.source", &self.source, AUTH_CREDENTIALS_FN, 1)
            }
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct MqttSubscriptionSpec {
    pub(crate) topic: String,
    pub(crate) decode: PayloadDecodeSpec,
}

impl MqttSubscriptionSpec {
    fn validate(&self) -> Result<(), LoaderError> {
        require_non_empty("subscription.topic", &self.topic)?;
        match self.decode {
            PayloadDecodeSpec::Json => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum PayloadDecodeSpec {
    Json,
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ScriptRuntimeSpec {
    Lua,
}

#[derive(Debug, Deserialize)]
pub(crate) struct EgressSpec {
    pub(crate) delivery: DeliveryModeSpec,
}

impl EgressSpec {
    pub(crate) fn validate(&self) -> Result<(), LoaderError> {
        match self.delivery {
            DeliveryModeSpec::Single | DeliveryModeSpec::Broadcast => Ok(()),
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum DeliveryModeSpec {
    Single,
    Broadcast,
}

impl From<DeliveryModeSpec> for DeliveryMode {
    fn from(value: DeliveryModeSpec) -> Self {
        match value {
            DeliveryModeSpec::Single => DeliveryMode::Single,
            DeliveryModeSpec::Broadcast => DeliveryMode::Broadcast,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProcessSpec {
    pub(crate) runtime: ScriptRuntimeSpec,
    pub(crate) source: String,
}

impl ProcessSpec {
    pub(crate) fn validate(&self) -> Result<(), LoaderError> {
        match self.runtime {
            ScriptRuntimeSpec::Lua => {}
        }
        validate_lua_function("Process.spec.source", &self.source, PROCESS_ON_MESSAGE_FN, 2)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PipelineSpec {
    pub(crate) execution: ExecutionSpec,
    pub(crate) source_refs: Vec<ResourceRef>,
    pub(crate) process_ref: ResourceRef,
    pub(crate) egress_ref: ResourceRef,
}

impl PipelineSpec {
    pub(crate) fn validate(&self) -> Result<(), LoaderError> {
        if self.execution.mode != "intra-proc" {
            return Err(resource_error(format!(
                "unsupported execution mode {}",
                self.execution.mode
            )));
        }
        if self.source_refs.is_empty() {
            return Err(resource_error("Pipeline must declare at least one sourceRef"));
        }
        for source_ref in &self.source_refs {
            source_ref.validate("Pipeline.sourceRefs")?;
        }
        self.process_ref.validate("Pipeline.processRef")?;
        self.egress_ref.validate("Pipeline.egressRef")?;
        Ok(())
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct ExecutionSpec {
    pub(crate) mode: String,
}

#[derive(Debug, Deserialize)]
pub(crate) struct ResourceRef {
    pub(crate) kind: Option<ManifestResourceKind>,
    pub(crate) name: String,
    #[serde(default)]
    pub(crate) version: String,
}

impl ResourceRef {
    pub(crate) fn validate(&self, field: &str) -> Result<(), LoaderError> {
        match &self.kind {
            Some(_) => {}
            None => return Err(resource_error(format!("{field}.kind must not be empty"))),
        }
        require_non_empty(&format!("{field}.name"), &self.name)?;
        Ok(())
    }

    pub(crate) fn display(&self) -> String {
        match self.kind {
            Some(kind) => format!("{kind} {}/{}", self.name, self.version),
            None => format!("{}/{}", self.name, self.version),
        }
    }
}

pub(crate) fn require_ref_kind(
    reference: &ResourceRef,
    expected: ManifestResourceKind,
    field: &str,
) -> Result<(), LoaderError> {
    if reference.kind == Some(expected) {
        return Ok(());
    }
    Err(reference_error(format!(
        "{field} must reference {expected}, got {}",
        reference.display()
    )))
}

pub(crate) fn require_non_empty(field: &str, value: &str) -> Result<(), LoaderError> {
    if value.trim().is_empty() {
        return Err(resource_error(format!("{field} must not be empty")));
    }
    Ok(())
}

pub(crate) fn resource_error(message: impl Into<String>) -> LoaderError {
    LoaderError::new(LoaderErrorKind::ResourceValidation, message)
}

fn reference_error(message: impl Into<String>) -> LoaderError {
    LoaderError::new(LoaderErrorKind::ReferenceResolution, message)
}

fn script_error(message: impl Into<String>) -> LoaderError {
    LoaderError::new(LoaderErrorKind::ScriptValidation, message)
}

fn validate_lua_function(
    field: &str,
    source: &str,
    function_name: &str,
    expected_arity: usize,
) -> Result<(), LoaderError> {
    if source.trim().is_empty() {
        return Err(script_error(format!("{field} must not be empty")));
    }
    lua::validate_extension_function(source, function_name, expected_arity)
        .map_err(|error| script_error(format!("{field}: {error}")))
}
