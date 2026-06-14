pub use crate::daemon::v1::{
    auth_plan, AuthPlan, DeliveryMode, DeploymentPlan, EgressPlan, ExecutionMode, ModulePlan,
    ModuleRuntime, MqttBrokerPlan, MqttSourcePlan, MqttSubscriptionPlan, NoAuth,
    PayloadDecodePlan, ProcessPlan, ResourceId, ResourceKind, UsernamePasswordAuth,
};

impl std::fmt::Display for ResourceKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Unspecified => "Unspecified",
            Self::MqttSource => "MqttSource",
            Self::Module => "Module",
            Self::Egress => "Egress",
            Self::Process => "Process",
            Self::Pipeline => "Pipeline",
        })
    }
}
