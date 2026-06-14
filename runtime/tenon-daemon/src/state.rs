use crate::{DaemonError, DaemonResult};
use std::collections::HashMap;
use std::future::Future;
use tenon_message::plan::DeploymentPlan;
use tenon_message::state::{
    state_mutation, StateEntry, StateMutation, StateSnapshot,
};

pub trait StateStore {
    fn save_plan(
        &mut self,
        plan: DeploymentPlan,
    ) -> impl Future<Output = DaemonResult<()>> + Send + '_;

    fn load_plan(
        &self,
    ) -> impl Future<Output = DaemonResult<Option<DeploymentPlan>>> + Send + '_;

    fn load<'a>(
        &'a self,
        scope: &'a str,
        keys: &'a [String],
    ) -> impl Future<Output = DaemonResult<StateSnapshot>> + Send + 'a;

    fn commit<'a>(
        &'a mut self,
        scope: &'a str,
        mutations: Vec<StateMutation>,
    ) -> impl Future<Output = DaemonResult<()>> + Send + 'a;
}

#[derive(Debug, Default)]
pub struct MemoryStateStore {
    plan: Option<DeploymentPlan>,
    scopes: HashMap<String, HashMap<String, Vec<u8>>>,
}

impl MemoryStateStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl StateStore for MemoryStateStore {
    fn save_plan(
        &mut self,
        plan: DeploymentPlan,
    ) -> impl Future<Output = DaemonResult<()>> + Send + '_ {
        async move {
            self.plan = Some(plan);
            Ok(())
        }
    }

    fn load_plan(
        &self,
    ) -> impl Future<Output = DaemonResult<Option<DeploymentPlan>>> + Send + '_ {
        async move { Ok(self.plan.clone()) }
    }

    fn load<'a>(
        &'a self,
        scope: &'a str,
        keys: &'a [String],
    ) -> impl Future<Output = DaemonResult<StateSnapshot>> + Send + 'a {
        async move {
            let values = self.scopes.get(scope);
            let entries = if keys.is_empty() {
                values
                    .into_iter()
                    .flat_map(|scope_values| scope_values.iter())
                    .map(|(key, value_json)| StateEntry {
                        key: key.clone(),
                        value_json: value_json.clone(),
                    })
                    .collect()
            } else {
                keys.iter()
                    .filter_map(|key| {
                        values
                            .and_then(|scope_values| scope_values.get(key))
                            .map(|value_json| StateEntry {
                                key: key.clone(),
                                value_json: value_json.clone(),
                            })
                    })
                    .collect()
            };

            Ok(StateSnapshot { entries })
        }
    }

    fn commit<'a>(
        &'a mut self,
        scope: &'a str,
        mutations: Vec<StateMutation>,
    ) -> impl Future<Output = DaemonResult<()>> + Send + 'a {
        async move {
            let scope_values = self.scopes.entry(scope.to_string()).or_default();
            for mutation in mutations {
                match mutation.op {
                    Some(state_mutation::Op::Set(set)) => {
                        if set.key.is_empty() {
                            return Err(DaemonError::state("state key is empty"));
                        }
                        scope_values.insert(set.key, set.value_json);
                    }
                    Some(state_mutation::Op::Delete(delete)) => {
                        if delete.key.is_empty() {
                            return Err(DaemonError::state("state key is empty"));
                        }
                        scope_values.remove(&delete.key);
                    }
                    None => return Err(DaemonError::state("state mutation op is missing")),
                }
            }
            Ok(())
        }
    }
}
