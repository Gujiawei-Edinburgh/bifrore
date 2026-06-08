use serde::Deserialize;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeploymentPlan {
    pub id: ResourceId,
    pub execution: ExecutionMode,
    pub sources: Vec<MqttSourcePlan>,
    pub process: ProcessPlan,
    pub egress: EgressPlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceId {
    pub kind: ResourceKind,
    pub name: String,
    pub version: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub enum ResourceKind {
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
pub enum ExecutionMode {
    IntraProc,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MqttSourcePlan {
    pub id: ResourceId,
    pub broker: MqttBrokerPlan,
    pub auth: AuthPlan,
    pub subscriptions: Vec<MqttSubscriptionPlan>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MqttBrokerPlan {
    pub host: String,
    pub port: u16,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AuthPlan {
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
pub struct MqttSubscriptionPlan {
    pub topic: String,
    pub decode: PayloadDecodePlan,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PayloadDecodePlan {
    Json,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessPlan {
    pub id: ResourceId,
    pub module: ModulePlan,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModulePlan {
    pub id: ResourceId,
    pub runtime: ModuleRuntime,
    pub source: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModuleRuntime {
    Lua,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EgressPlan {
    pub id: ResourceId,
    pub channel: String,
}

impl DeploymentPlan {
    pub fn new(
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
