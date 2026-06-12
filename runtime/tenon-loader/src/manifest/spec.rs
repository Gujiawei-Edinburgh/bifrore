use serde::Deserialize;
use tenon_extension::{AUTH_CREDENTIALS_FN, PROCESS_ON_MESSAGE_FN};

use crate::{
    DeliveryMode, LoaderError, LoaderErrorKind, ModuleRuntime, PayloadDecodePlan, ResourceKind,
};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct MqttSourceSpec {
    pub(crate) broker: MqttBrokerSpec,
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
    Module {
        module: InlineModuleSpec,
    },
}

impl AuthSpec {
    pub(crate) fn validate(&self) -> Result<(), LoaderError> {
        match self {
            Self::Static { username, password } => {
                require_non_empty("auth.username", username)?;
                require_non_empty("auth.password", password)
            }
            Self::Module { module } => {
                module.validate()?;
                Ok(())
            }
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct InlineModuleSpec {
    pub(crate) runtime: ModuleRuntimeSpec,
    pub(crate) source: String,
}

impl InlineModuleSpec {
    fn validate(&self) -> Result<(), LoaderError> {
        match self.runtime {
            ModuleRuntimeSpec::Lua => {
                validate_lua_function("auth.module.source", &self.source, AUTH_CREDENTIALS_FN)
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

impl From<PayloadDecodeSpec> for PayloadDecodePlan {
    fn from(value: PayloadDecodeSpec) -> Self {
        match value {
            PayloadDecodeSpec::Json => PayloadDecodePlan::Json,
        }
    }
}

#[derive(Debug, Deserialize)]
pub(crate) struct ModuleSpec {
    pub(crate) runtime: ModuleRuntimeSpec,
    pub(crate) source: String,
}

impl ModuleSpec {
    pub(crate) fn validate(&self) -> Result<(), LoaderError> {
        match self.runtime {
            ModuleRuntimeSpec::Lua => {}
        }
        validate_lua_function("Module.spec.source", &self.source, PROCESS_ON_MESSAGE_FN)
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum ModuleRuntimeSpec {
    Lua,
}

impl From<ModuleRuntimeSpec> for ModuleRuntime {
    fn from(value: ModuleRuntimeSpec) -> Self {
        match value {
            ModuleRuntimeSpec::Lua => ModuleRuntime::Lua,
        }
    }
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
    pub(crate) module_ref: ResourceRef,
}

impl ProcessSpec {
    pub(crate) fn validate(&self) -> Result<(), LoaderError> {
        self.module_ref.validate("Process.moduleRef")?;
        require_ref_kind(&self.module_ref, ResourceKind::Module, "Process.moduleRef")
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
    pub(crate) kind: Option<ResourceKind>,
    pub(crate) name: String,
    pub(crate) version: String,
}

impl ResourceRef {
    pub(crate) fn validate(&self, field: &str) -> Result<(), LoaderError> {
        match &self.kind {
            Some(_) => {}
            None => return Err(resource_error(format!("{field}.kind must not be empty"))),
        }
        require_non_empty(&format!("{field}.name"), &self.name)?;
        require_non_empty(&format!("{field}.version"), &self.version)
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
    expected: ResourceKind,
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

fn module_error(message: impl Into<String>) -> LoaderError {
    LoaderError::new(LoaderErrorKind::ModuleValidation, message)
}

fn validate_lua_function(
    field: &str,
    source: &str,
    function_name: &str,
) -> Result<(), LoaderError> {
    if source.trim().is_empty() {
        return Err(module_error(format!("{field} must not be empty")));
    }
    let named_function = format!("function {function_name}");
    let assigned_function = format!("{function_name} = function");
    if source.contains(&named_function) || source.contains(&assigned_function) {
        return Ok(());
    }
    Err(module_error(format!(
        "{field} must define Lua function {function_name}"
    )))
}
