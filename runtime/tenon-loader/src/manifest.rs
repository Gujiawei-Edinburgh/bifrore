use std::collections::HashMap;

use serde::Deserialize;

use crate::{
    AuthPlan, DeploymentPlan, EgressPlan, ExecutionMode, LoaderError, LoaderErrorKind, ModulePlan,
    ModuleRuntime, MqttBrokerPlan, MqttSourcePlan, MqttSubscriptionPlan, PayloadDecodePlan,
    ProcessPlan, ResourceId, ResourceKind,
};

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

pub(crate) fn resolve_deployment_plan(
    resources: Vec<ResourceDocument>,
) -> Result<DeploymentPlan, LoaderError> {
    ResourceRegistry::new(resources)?.resolve_deployment_plan()
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ResourceDocument {
    pub(crate) api_version: String,
    pub(crate) kind: ResourceKind,
    pub(crate) metadata: ResourceMetadata,
    pub(crate) spec: serde_yaml::Value,
}

impl ResourceDocument {
    pub(crate) fn id(&self) -> ResourceId {
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
pub(crate) struct ResourceMetadata {
    pub(crate) name: String,
    pub(crate) version: String,
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

struct ResourceRegistry {
    sources: HashMap<ResourceKey, ResourceDocument>,
    modules: HashMap<ResourceKey, ResourceDocument>,
    processes: HashMap<ResourceKey, ResourceDocument>,
    egresses: HashMap<ResourceKey, ResourceDocument>,
    pipelines: Vec<ResourceDocument>,
}

impl ResourceRegistry {
    fn new(resources: Vec<ResourceDocument>) -> Result<Self, LoaderError> {
        let mut registry = Self {
            sources: HashMap::new(),
            modules: HashMap::new(),
            processes: HashMap::new(),
            egresses: HashMap::new(),
            pipelines: Vec::new(),
        };
        let mut seen = HashMap::new();

        for resource in resources {
            let key = ResourceKey::from_id(&resource.id());
            if seen.insert(key.clone(), ()).is_some() {
                return Err(resource_error(format!(
                    "duplicate resource {}",
                    resource.display_name()
                )));
            }
            match resource.kind {
                ResourceKind::MqttSource => {
                    registry.sources.insert(key, resource);
                }
                ResourceKind::Module => {
                    registry.modules.insert(key, resource);
                }
                ResourceKind::Process => {
                    registry.processes.insert(key, resource);
                }
                ResourceKind::Egress => {
                    registry.egresses.insert(key, resource);
                }
                ResourceKind::Pipeline => {
                    registry.pipelines.push(resource);
                }
            }
        }

        Ok(registry)
    }

    fn resolve_deployment_plan(&self) -> Result<DeploymentPlan, LoaderError> {
        if self.pipelines.len() != 1 {
            return Err(resource_error(format!(
                "loader expects exactly one Pipeline resource, got {}",
                self.pipelines.len()
            )));
        }
        let pipeline = &self.pipelines[0];
        let spec = parse_spec::<PipelineSpec>(pipeline)?;
        let sources = spec
            .source_refs
            .iter()
            .map(|reference| self.resolve_source(reference))
            .collect::<Result<Vec<_>, _>>()?;
        let process = self.resolve_process(&spec.process_ref)?;
        let egress = self.resolve_egress(&spec.egress_ref)?;

        Ok(DeploymentPlan::new(
            pipeline.id(),
            ExecutionMode::IntraProc,
            sources,
            process,
            egress,
        ))
    }

    fn resolve_source(&self, reference: &ResourceRef) -> Result<MqttSourcePlan, LoaderError> {
        require_ref_kind(reference, ResourceKind::MqttSource, "Pipeline.sourceRefs")?;
        let resource = self
            .sources
            .get(&ResourceKey::from_ref(ResourceKind::MqttSource, reference))
            .ok_or_else(|| reference_error(format!("missing MqttSource {}", reference.display())))?;
        let spec = parse_spec::<MqttSourceSpec>(resource)?;
        Ok(MqttSourcePlan {
            id: resource.id(),
            broker: MqttBrokerPlan {
                host: spec.broker.host,
                port: spec.broker.port,
            },
            auth: self.resolve_auth(&resource.id(), spec.auth)?,
            subscriptions: spec
                .subscriptions
                .into_iter()
                .map(|subscription| MqttSubscriptionPlan {
                    topic: subscription.topic,
                    decode: subscription.decode.into(),
                })
                .collect(),
        })
    }

    fn resolve_auth(
        &self,
        source_id: &ResourceId,
        auth: Option<AuthSpec>,
    ) -> Result<AuthPlan, LoaderError> {
        match auth {
            None => Ok(AuthPlan::None),
            Some(AuthSpec::Static { username, password }) => {
                Ok(AuthPlan::Static { username, password })
            }
            Some(AuthSpec::Module { module }) => Ok(AuthPlan::Module {
                module: ModulePlan {
                    id: ResourceId {
                        kind: ResourceKind::Module,
                        name: format!("{}-auth", source_id.name),
                        version: source_id.version.clone(),
                    },
                    runtime: module.runtime.into(),
                    source: module.source,
                },
            }),
        }
    }

    fn resolve_process(&self, reference: &ResourceRef) -> Result<ProcessPlan, LoaderError> {
        require_ref_kind(reference, ResourceKind::Process, "Pipeline.processRef")?;
        let resource = self
            .processes
            .get(&ResourceKey::from_ref(ResourceKind::Process, reference))
            .ok_or_else(|| reference_error(format!("missing Process {}", reference.display())))?;
        let spec = parse_spec::<ProcessSpec>(resource)?;
        Ok(ProcessPlan {
            id: resource.id(),
            module: self.resolve_module(&spec.module_ref)?,
        })
    }

    fn resolve_module(&self, reference: &ModuleRef) -> Result<ModulePlan, LoaderError> {
        let resource = self
            .modules
            .get(&ResourceKey::new(
                ResourceKind::Module,
                &reference.name,
                &reference.version,
            ))
            .ok_or_else(|| {
                reference_error(format!(
                    "missing Module {}/{}",
                    reference.name, reference.version
                ))
            })?;
        let spec = parse_spec::<ModuleSpec>(resource)?;
        Ok(ModulePlan {
            id: resource.id(),
            runtime: spec.runtime.into(),
            source: spec.source,
        })
    }

    fn resolve_egress(&self, reference: &ResourceRef) -> Result<EgressPlan, LoaderError> {
        require_ref_kind(reference, ResourceKind::Egress, "Pipeline.egressRef")?;
        let resource = self
            .egresses
            .get(&ResourceKey::from_ref(ResourceKind::Egress, reference))
            .ok_or_else(|| reference_error(format!("missing Egress {}", reference.display())))?;
        let spec = parse_spec::<EgressSpec>(resource)?;
        Ok(EgressPlan {
            id: resource.id(),
            channel: spec.channel,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ResourceKey {
    kind: ResourceKind,
    name: String,
    version: String,
}

impl ResourceKey {
    fn new(kind: ResourceKind, name: &str, version: &str) -> Self {
        Self {
            kind,
            name: name.to_string(),
            version: version.to_string(),
        }
    }

    fn from_id(id: &ResourceId) -> Self {
        Self::new(id.kind, &id.name, &id.version)
    }

    fn from_ref(kind: ResourceKind, reference: &ResourceRef) -> Self {
        Self::new(kind, &reference.name, &reference.version)
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

impl From<PayloadDecodeSpec> for PayloadDecodePlan {
    fn from(value: PayloadDecodeSpec) -> Self {
        match value {
            PayloadDecodeSpec::Json => PayloadDecodePlan::Json,
        }
    }
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

impl From<ModuleRuntimeSpec> for ModuleRuntime {
    fn from(value: ModuleRuntimeSpec) -> Self {
        match value {
            ModuleRuntimeSpec::Lua => ModuleRuntime::Lua,
        }
    }
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

    fn display(&self) -> String {
        match self.kind {
            Some(kind) => format!("{kind} {}/{}", self.name, self.version),
            None => format!("{}/{}", self.name, self.version),
        }
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

fn reference_error(message: impl Into<String>) -> LoaderError {
    LoaderError::new(LoaderErrorKind::ReferenceResolution, message)
}

fn module_error(message: impl Into<String>) -> LoaderError {
    LoaderError::new(LoaderErrorKind::ModuleValidation, message)
}

fn require_ref_kind(
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
