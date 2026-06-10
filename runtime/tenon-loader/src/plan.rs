use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeploymentPlan {
    pub(crate) id: ResourceId,
    pub(crate) execution: ExecutionMode,
    pub(crate) sources: Vec<MqttSourcePlan>,
    pub(crate) process: ProcessPlan,
    pub(crate) egress: EgressPlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ResourceId {
    pub(crate) kind: ResourceKind,
    pub(crate) name: String,
    pub(crate) version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) enum ResourceKind {
    MqttSource,
    Module,
    Egress,
    Process,
    Pipeline,
}

impl std::fmt::Display for ResourceKind {
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ExecutionMode {
    IntraProc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MqttSourcePlan {
    pub(crate) id: ResourceId,
    pub(crate) broker: MqttBrokerPlan,
    pub(crate) auth: AuthPlan,
    pub(crate) subscriptions: Vec<MqttSubscriptionPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MqttBrokerPlan {
    pub(crate) host: String,
    pub(crate) port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum AuthPlan {
    None,
    Static {
        username: String,
        password: String,
    },
    Module {
        module: ModulePlan,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MqttSubscriptionPlan {
    pub(crate) topic: String,
    pub(crate) decode: PayloadDecodePlan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PayloadDecodePlan {
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ProcessPlan {
    pub(crate) id: ResourceId,
    pub(crate) module: ModulePlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ModulePlan {
    pub(crate) id: ResourceId,
    pub(crate) runtime: ModuleRuntime,
    pub(crate) source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ModuleRuntime {
    Lua,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct EgressPlan {
    pub(crate) id: ResourceId,
    pub(crate) channel: String,
}

impl DeploymentPlan {
    pub(crate) fn new(
        id: ResourceId,
        execution: ExecutionMode,
        sources: Vec<MqttSourcePlan>,
        process: ProcessPlan,
        egress: EgressPlan,
    ) -> Self {
        Self {
            id,
            execution,
            sources,
            process,
            egress,
        }
    }
}
