use std::collections::HashMap;

use serde::Deserialize;

mod spec;

use spec::{
    require_non_empty, require_ref_kind, resource_error, AuthSpec, EgressSpec, ModuleSpec,
    MqttSourceSpec, PipelineSpec, ProcessSpec, ResourceRef,
};

use crate::{
    auth_plan, AuthPlan, DeliveryMode, DeploymentPlan, EgressPlan, ExecutionMode, LoaderError,
    LoaderErrorKind, ModulePlan, ModuleRuntime, MqttBrokerPlan, MqttSourcePlan,
    MqttSubscriptionPlan, NoAuth, PayloadDecodePlan, ProcessPlan, ResourceId,
    ResourceKind as PlanResourceKind,
    UsernamePasswordAuth,
};

const API_VERSION: &str = "tenon.apache.org/v1alpha1";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) enum ManifestResourceKind {
    MqttSource,
    Module,
    Egress,
    Process,
    Pipeline,
}

impl ManifestResourceKind {
    fn into_plan_kind(self) -> PlanResourceKind {
        match self {
            Self::MqttSource => PlanResourceKind::MqttSource,
            Self::Module => PlanResourceKind::Module,
            Self::Egress => PlanResourceKind::Egress,
            Self::Process => PlanResourceKind::Process,
            Self::Pipeline => PlanResourceKind::Pipeline,
        }
    }
}

impl std::fmt::Display for ManifestResourceKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::MqttSource => "MqttSource",
            Self::Module => "Module",
            Self::Egress => "Egress",
            Self::Process => "Process",
            Self::Pipeline => "Pipeline",
        })
    }
}

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
    pub(crate) kind: ManifestResourceKind,
    pub(crate) metadata: ResourceMetadata,
    pub(crate) spec: serde_yaml::Value,
}

impl ResourceDocument {
    pub(crate) fn id(&self) -> ResourceId {
        ResourceId {
            kind: self.kind.into_plan_kind() as i32,
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
        ManifestResourceKind::MqttSource => parse_spec::<MqttSourceSpec>(resource)?.validate(),
        ManifestResourceKind::Module => parse_spec::<ModuleSpec>(resource)?.validate(),
        ManifestResourceKind::Egress => parse_spec::<EgressSpec>(resource)?.validate(),
        ManifestResourceKind::Process => parse_spec::<ProcessSpec>(resource)?.validate(),
        ManifestResourceKind::Pipeline => parse_spec::<PipelineSpec>(resource)?.validate(),
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
            let key = ResourceKey::new(
                resource.kind,
                &resource.metadata.name,
                &resource.metadata.version,
            );
            if seen.insert(key.clone(), ()).is_some() {
                return Err(resource_error(format!(
                    "duplicate resource {}",
                    resource.display_name()
                )));
            }
            match resource.kind {
                ManifestResourceKind::MqttSource => {
                    registry.sources.insert(key, resource);
                }
                ManifestResourceKind::Module => {
                    registry.modules.insert(key, resource);
                }
                ManifestResourceKind::Process => {
                    registry.processes.insert(key, resource);
                }
                ManifestResourceKind::Egress => {
                    registry.egresses.insert(key, resource);
                }
                ManifestResourceKind::Pipeline => {
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

        Ok(DeploymentPlan {
            id: Some(pipeline.id()),
            execution: ExecutionMode::IntraProc as i32,
            sources,
            process: Some(process),
            egress: Some(egress),
        })
    }

    fn resolve_source(&self, reference: &ResourceRef) -> Result<MqttSourcePlan, LoaderError> {
        require_ref_kind(reference, ManifestResourceKind::MqttSource, "Pipeline.sourceRefs")?;
        let resource = self
            .sources
            .get(&ResourceKey::from_ref(ManifestResourceKind::MqttSource, reference))
            .ok_or_else(|| reference_error(format!("missing MqttSource {}", reference.display())))?;
        let spec = parse_spec::<MqttSourceSpec>(resource)?;
        Ok(MqttSourcePlan {
            id: Some(resource.id()),
            broker: Some(MqttBrokerPlan {
                host: spec.broker.host,
                port: spec.broker.port as u32,
            }),
            auth: Some(self.resolve_auth(&resource.id(), spec.auth)?),
            subscriptions: spec
                .subscriptions
                .into_iter()
                .map(|subscription| MqttSubscriptionPlan {
                    topic: subscription.topic,
                    decode: PayloadDecodePlan::Json as i32,
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
            None => Ok(AuthPlan {
                kind: Some(auth_plan::Kind::None(NoAuth {})),
            }),
            Some(AuthSpec::Static { username, password }) => Ok(AuthPlan {
                kind: Some(auth_plan::Kind::UsernamePassword(UsernamePasswordAuth {
                    username,
                    password,
                })),
            }),
            Some(AuthSpec::Module { module }) => Ok(AuthPlan {
                kind: Some(auth_plan::Kind::Module(ModulePlan {
                    id: Some(ResourceId {
                        kind: PlanResourceKind::Module as i32,
                        name: format!("{}-auth", source_id.name),
                        version: source_id.version.clone(),
                    }),
                    runtime: ModuleRuntime::Lua as i32,
                    source: module.source,
                })),
            }),
        }
    }

    fn resolve_process(&self, reference: &ResourceRef) -> Result<ProcessPlan, LoaderError> {
        require_ref_kind(reference, ManifestResourceKind::Process, "Pipeline.processRef")?;
        let resource = self
            .processes
            .get(&ResourceKey::from_ref(ManifestResourceKind::Process, reference))
            .ok_or_else(|| reference_error(format!("missing Process {}", reference.display())))?;
        let spec = parse_spec::<ProcessSpec>(resource)?;
        Ok(ProcessPlan {
            id: Some(resource.id()),
            module: Some(self.resolve_module(&spec.module_ref)?),
        })
    }

    fn resolve_module(&self, reference: &ResourceRef) -> Result<ModulePlan, LoaderError> {
        require_ref_kind(reference, ManifestResourceKind::Module, "Process.moduleRef")?;
        let resource = self
            .modules
            .get(&ResourceKey::from_ref(ManifestResourceKind::Module, reference))
            .ok_or_else(|| reference_error(format!("missing Module {}", reference.display())))?;
        let spec = parse_spec::<ModuleSpec>(resource)?;
        Ok(ModulePlan {
            id: Some(resource.id()),
            runtime: ModuleRuntime::Lua as i32,
            source: spec.source,
        })
    }

    fn resolve_egress(&self, reference: &ResourceRef) -> Result<EgressPlan, LoaderError> {
        require_ref_kind(reference, ManifestResourceKind::Egress, "Pipeline.egressRef")?;
        let resource = self
            .egresses
            .get(&ResourceKey::from_ref(ManifestResourceKind::Egress, reference))
            .ok_or_else(|| reference_error(format!("missing Egress {}", reference.display())))?;
        let spec = parse_spec::<EgressSpec>(resource)?;
        Ok(EgressPlan {
            id: Some(resource.id()),
            delivery: i32::from(DeliveryMode::from(spec.delivery)),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct ResourceKey {
    kind: ManifestResourceKind,
    name: String,
    version: String,
}

impl ResourceKey {
    fn new(kind: ManifestResourceKind, name: &str, version: &str) -> Self {
        Self {
            kind,
            name: name.to_string(),
            version: version.to_string(),
        }
    }

    fn from_ref(kind: ManifestResourceKind, reference: &ResourceRef) -> Self {
        Self::new(kind, &reference.name, &reference.version)
    }
}

fn reference_error(message: impl Into<String>) -> LoaderError {
    LoaderError::new(LoaderErrorKind::ReferenceResolution, message)
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
