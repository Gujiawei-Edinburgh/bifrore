use crate::{DaemonError, DaemonResult, DeploymentKey};
use prost::Message;
use std::collections::HashMap;
use std::future::Future;
use tenon_message::plan::DeploymentPlan;

pub trait PlanStore {
    fn save_plan<'a>(
        &'a mut self,
        key: &'a DeploymentKey,
        plan: DeploymentPlan,
    ) -> impl Future<Output = DaemonResult<()>> + Send + 'a;

    fn load_plan<'a>(
        &'a self,
        key: &'a DeploymentKey,
    ) -> impl Future<Output = DaemonResult<Option<DeploymentPlan>>> + Send + 'a;
}

#[derive(Debug, Default)]
pub struct InMemoryPlanStore {
    entries: HashMap<Vec<u8>, Vec<u8>>,
}

impl InMemoryPlanStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl PlanStore for InMemoryPlanStore {
    fn save_plan<'a>(
        &'a mut self,
        key: &'a DeploymentKey,
        plan: DeploymentPlan,
    ) -> impl Future<Output = DaemonResult<()>> + Send + 'a {
        async move {
            self.entries.insert(plan_key(key), plan.encode_to_vec());
            Ok(())
        }
    }

    fn load_plan<'a>(
        &'a self,
        key: &'a DeploymentKey,
    ) -> impl Future<Output = DaemonResult<Option<DeploymentPlan>>> + Send + 'a {
        async move {
            self.entries
                .get(&plan_key(key))
                .map(|bytes| {
                    DeploymentPlan::decode(bytes.as_slice()).map_err(|error| {
                        DaemonError::store(format!("failed to decode deployment plan: {error}"))
                    })
                })
                .transpose()
        }
    }
}

fn plan_key(key: &DeploymentKey) -> Vec<u8> {
    format!("plan/{}", key.as_store_key()).into_bytes()
}
