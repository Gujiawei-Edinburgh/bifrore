use serde::Deserialize;

use crate::{LoaderError, LoaderErrorKind, ResourceId, ResourceKind};

const API_VERSION: &str = "tenon.apache.org/v1alpha1";

pub(crate) fn parse_resource_documents(
    manifest: &str,
) -> Result<Vec<ResourceDocument>, LoaderError> {
    let mut resources = Vec::new();
    for document in serde_yaml::Deserializer::from_str(manifest) {
        let value = serde_yaml::Value::deserialize(document).map_err(|error| {
            LoaderError::new(
                LoaderErrorKind::ManifestParsing,
                format!("failed to parse YAML document: {error}"),
            )
        })?;
        if value.is_null() {
            continue;
        }
        let resource = ResourceDocument::deserialize(value).map_err(|error| {
            LoaderError::new(
                LoaderErrorKind::ManifestParsing,
                format!("failed to parse Tenon resource: {error}"),
            )
        })?;
        resource.validate()?;
        resources.push(resource);
    }
    if resources.is_empty() {
        return Err(LoaderError::new(
            LoaderErrorKind::EmptyManifest,
            "pipeline manifest does not contain Tenon resources",
        ));
    }
    Ok(resources)
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ResourceDocument {
    pub api_version: String,
    pub kind: ResourceKind,
    pub metadata: ResourceMetadata,
    pub spec: serde_yaml::Value,
}

impl ResourceDocument {
    pub fn id(&self) -> ResourceId {
        ResourceId {
            kind: self.kind,
            name: self.metadata.name.clone(),
            version: self.metadata.version.clone(),
        }
    }

    fn validate(&self) -> Result<(), LoaderError> {
        if self.api_version != API_VERSION {
            return Err(resource_error(format!(
                "unsupported apiVersion {} for {}",
                self.api_version,
                self.display_name()
            )));
        }
        require_non_empty("metadata.name", &self.metadata.name)?;
        require_non_empty("metadata.version", &self.metadata.version)?;
        validate_spec(self)
    }

    fn display_name(&self) -> String {
        format!("{} {}/{}", self.kind, self.metadata.name, self.metadata.version)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ResourceMetadata {
    pub name: String,
    pub version: String,
}

fn validate_spec(resource: &ResourceDocument) -> Result<(), LoaderError> {
    match resource.kind {
        ResourceKind::MqttSource => parse_spec::<MqttSourceSpec>(resource)?.validate(),
        ResourceKind::Module => parse_spec::<ModuleSpec>(resource)?.validate(),
        ResourceKind::Egress => parse_spec::<EgressSpec>(resource)?.validate(),
        ResourceKind::Process => parse_spec::<ProcessSpec>(resource)?.validate(),
        ResourceKind::Pipeline => parse_spec::<PipelineSpec>(resource)?.validate(),
    }
}

fn parse_spec<T: for<'de> Deserialize<'de>>(
    resource: &ResourceDocument,
) -> Result<T, LoaderError> {
    serde_yaml::from_value(resource.spec.clone()).map_err(|error| {
        resource_error(format!(
            "invalid spec for {}: {error}",
            resource.display_name()
        ))
    })
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct MqttSourceSpec {
    broker: MqttBrokerSpec,
    #[serde(default)]
    auth: Option<AuthSpec>,
    subscriptions: Vec<MqttSubscriptionSpec>,
}

impl MqttSourceSpec {
    fn validate(&self) -> Result<(), LoaderError> {
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
struct MqttBrokerSpec {
    host: String,
    port: u16,
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum AuthSpec {
    Static {
        username: String,
        password: String,
    },
    Module {
        module: InlineModuleSpec,
    },
}

impl AuthSpec {
    fn validate(&self) -> Result<(), LoaderError> {
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
struct InlineModuleSpec {
    runtime: ModuleRuntimeSpec,
    source: String,
}

impl InlineModuleSpec {
    fn validate(&self) -> Result<(), LoaderError> {
        match self.runtime {
            ModuleRuntimeSpec::Lua => {
                validate_lua_function("auth.module.source", &self.source, "credentials")
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct MqttSubscriptionSpec {
    topic: String,
    decode: PayloadDecodeSpec,
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
enum PayloadDecodeSpec {
    Json,
}

#[derive(Debug, Deserialize)]
struct ModuleSpec {
    runtime: ModuleRuntimeSpec,
    source: String,
}

impl ModuleSpec {
    fn validate(&self) -> Result<(), LoaderError> {
        match self.runtime {
            ModuleRuntimeSpec::Lua => {}
        }
        validate_lua_function("Module.spec.source", &self.source, "on_message")
    }
}

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "kebab-case")]
enum ModuleRuntimeSpec {
    Lua,
}

#[derive(Debug, Deserialize)]
struct EgressSpec {
    channel: String,
}

impl EgressSpec {
    fn validate(&self) -> Result<(), LoaderError> {
        require_non_empty("Egress.channel", &self.channel)
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct ProcessSpec {
    module_ref: ModuleRef,
}

impl ProcessSpec {
    fn validate(&self) -> Result<(), LoaderError> {
        require_non_empty("Process.moduleRef.name", &self.module_ref.name)?;
        require_non_empty("Process.moduleRef.version", &self.module_ref.version)
    }
}

#[derive(Debug, Deserialize)]
struct ModuleRef {
    name: String,
    version: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct PipelineSpec {
    execution: ExecutionSpec,
    source_refs: Vec<ResourceRef>,
    process_ref: ResourceRef,
    egress_ref: ResourceRef,
}

impl PipelineSpec {
    fn validate(&self) -> Result<(), LoaderError> {
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
struct ExecutionSpec {
    mode: String,
}

#[derive(Debug, Deserialize)]
struct ResourceRef {
    kind: Option<ResourceKind>,
    name: String,
    version: String,
}

impl ResourceRef {
    fn validate(&self, field: &str) -> Result<(), LoaderError> {
        match &self.kind {
            Some(_) => {}
            None => return Err(resource_error(format!("{field}.kind must not be empty"))),
        }
        require_non_empty(&format!("{field}.name"), &self.name)?;
        require_non_empty(&format!("{field}.version"), &self.version)
    }
}

fn require_non_empty(field: &str, value: &str) -> Result<(), LoaderError> {
    if value.trim().is_empty() {
        return Err(resource_error(format!("{field} must not be empty")));
    }
    Ok(())
}

fn resource_error(message: impl Into<String>) -> LoaderError {
    LoaderError::new(LoaderErrorKind::ResourceValidation, message)
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
