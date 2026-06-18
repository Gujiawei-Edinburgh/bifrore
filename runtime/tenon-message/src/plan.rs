pub use crate::daemon::v1::{
    auth_plan, AuthPlan, DeliveryMode, DeploymentPlan, EgressPlan, ExecutionMode, MqttBrokerPlan,
    MqttClientIds, MqttSourcePlan, MqttSubscriptionPlan, NoAuth,
    resource, PayloadDecodePlan, ProcessPlan, Resource, ResourceId, ResourceKind, ResourceReferences,
    ScriptModule, ScriptRuntime, StoredDeploymentPlan, UsernamePasswordAuth,
};

impl std::fmt::Display for ResourceKind {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(match self {
            Self::Unspecified => "Unspecified",
            Self::MqttSource => "MqttSource",
            Self::Egress => "Egress",
            Self::Process => "Process",
            Self::Pipeline => "Pipeline",
        })
    }
}
