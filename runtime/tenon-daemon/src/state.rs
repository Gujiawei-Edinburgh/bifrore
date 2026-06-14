use crate::{DaemonError, DaemonResult, DeploymentKey};
use prost::Message;
use std::collections::HashMap;
use std::future::Future;
use tenon_message::plan::DeploymentPlan;
use tenon_message::state::{
    state_mutation, StateEntry, StateMutation, StateSnapshot,
};

pub trait StateStore {
    fn save_plan<'a>(
        &'a mut self,
        key: &'a DeploymentKey,
        plan: DeploymentPlan,
    ) -> impl Future<Output = DaemonResult<()>> + Send + 'a;

    fn load_plan<'a>(
        &'a self,
        key: &'a DeploymentKey,
    ) -> impl Future<Output = DaemonResult<Option<DeploymentPlan>>> + Send + 'a;

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
    entries: HashMap<Vec<u8>, Vec<u8>>,
}

impl MemoryStateStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl StateStore for MemoryStateStore {
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
                        DaemonError::state(format!("failed to decode deployment plan: {error}"))
                    })
                })
                .transpose()
        }
    }

    fn load<'a>(
        &'a self,
        scope: &'a str,
        keys: &'a [String],
    ) -> impl Future<Output = DaemonResult<StateSnapshot>> + Send + 'a {
        async move {
            let entries = if keys.is_empty() {
                let prefix = state_scope_prefix(scope);
                self.entries
                    .iter()
                    .filter_map(|(entry_key, value_json)| {
                        entry_key.strip_prefix(prefix.as_slice()).and_then(|state_key| {
                            String::from_utf8(state_key.to_vec()).ok().map(|key| {
                                StateEntry {
                                    key,
                                    value_json: value_json.clone(),
                                }
                            })
                        })
                    })
                    .collect()
            } else {
                keys.iter()
                    .filter_map(|key| {
                        self.entries.get(&state_key(scope, key)).map(|value_json| {
                            StateEntry {
                                key: key.clone(),
                                value_json: value_json.clone(),
                            }
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
            for mutation in mutations {
                match mutation.op {
                    Some(state_mutation::Op::Set(set)) => {
                        if set.key.is_empty() {
                            return Err(DaemonError::state("state key is empty"));
                        }
                        self.entries.insert(state_key(scope, &set.key), set.value_json);
                    }
                    Some(state_mutation::Op::Delete(delete)) => {
                        if delete.key.is_empty() {
                            return Err(DaemonError::state("state key is empty"));
                        }
                        self.entries.remove(&state_key(scope, &delete.key));
                    }
                    None => return Err(DaemonError::state("state mutation op is missing")),
                }
            }
            Ok(())
        }
    }
}

fn plan_key(key: &DeploymentKey) -> Vec<u8> {
    format!("plan/{}", key.as_store_key()).into_bytes()
}

fn state_scope_prefix(scope: &str) -> Vec<u8> {
    format!("state/{scope}/").into_bytes()
}

fn state_key(scope: &str, key: &str) -> Vec<u8> {
    let mut bytes = state_scope_prefix(scope);
    bytes.extend_from_slice(key.as_bytes());
    bytes
}
