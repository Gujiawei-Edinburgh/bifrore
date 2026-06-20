use crate::{DaemonError, DaemonResult, ResourceKey};
use prost::Message;
use std::collections::HashMap;
use std::future::Future;
use tenon_message::plan::{DeploymentPlan, MqttClientIds, ResourceId};

const PLAN_LABEL: &str = "deployment plan";
const MQTT_CLIENT_IDS_LABEL: &str = "MQTT client ids";
const LATEST_PIPELINE_ID_LABEL: &str = "latest pipeline ID";

const PIPELINE_KEY_PREFIX: &[u8] = b"pipeline";
const PIPELINE_METADATA_KEY_PREFIX: &[u8] = b"pipeline_metadata";
const MQTT_CLIENT_IDS_KEY_PREFIX: &[u8] = b"mqtt_source_client_ids";

pub trait DaemonStore {
    fn save_pipeline<'a>(
        &'a mut self,
        plan: DeploymentPlan,
    ) -> impl Future<Output = DaemonResult<()>> + Send + 'a;

    fn load_pipeline<'a>(
        &'a self,
        id: &'a ResourceId,
    ) -> impl Future<Output = DaemonResult<Option<DeploymentPlan>>> + Send + 'a;

    fn load_latest_pipeline_id<'a>(
        &'a self,
        name: &'a str,
    ) -> impl Future<Output = DaemonResult<Option<ResourceId>>> + Send + 'a;

    fn save_mqtt_client_ids<'a>(
        &'a mut self,
        key: &'a ResourceKey,
        client_ids: Vec<String>,
    ) -> impl Future<Output = DaemonResult<()>> + Send + 'a;

    fn load_mqtt_client_ids<'a>(
        &'a self,
        key: &'a ResourceKey,
    ) -> impl Future<Output = DaemonResult<Vec<String>>> + Send + 'a;
}

#[derive(Debug, Default)]
pub struct InMemoryDaemonStore {
    entries: HashMap<Vec<u8>, Vec<u8>>,
}

impl InMemoryDaemonStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl DaemonStore for InMemoryDaemonStore {
    fn save_pipeline<'a>(
        &'a mut self,
        plan: DeploymentPlan,
    ) -> impl Future<Output = DaemonResult<()>> + Send + 'a {
        async move {
            let id = required_pipeline_id(plan.id.as_ref())?;
            let key = ResourceKey::from_id(id);
            let storage_key = pipeline_key(&key);
            if self.load_message::<DeploymentPlan>(&storage_key, PLAN_LABEL)?.is_some() {
                return Err(DaemonError::invalid_state(format!(
                    "pipeline resource already exists: {}",
                    key.as_store_key()
                )));
            }
            self.save_message(storage_key, &plan);
            self.save_message(pipeline_metadata_key(&id.name), id);
            Ok(())
        }
    }

    fn load_pipeline<'a>(
        &'a self,
        id: &'a ResourceId,
    ) -> impl Future<Output = DaemonResult<Option<DeploymentPlan>>> + Send + 'a {
        async move {
            required_pipeline_id(Some(id))?;
            self.load_message::<DeploymentPlan>(&pipeline_key(&ResourceKey::from_id(id)), PLAN_LABEL)
        }
    }

    fn load_latest_pipeline_id<'a>(
        &'a self,
        name: &'a str,
    ) -> impl Future<Output = DaemonResult<Option<ResourceId>>> + Send + 'a {
        async move {
            if name.trim().is_empty() {
                return Err(DaemonError::invalid_state("pipeline name is missing"));
            }
            self.load_message::<ResourceId>(&pipeline_metadata_key(name), LATEST_PIPELINE_ID_LABEL)
        }
    }

    fn save_mqtt_client_ids<'a>(
        &'a mut self,
        key: &'a ResourceKey,
        client_ids: Vec<String>,
    ) -> impl Future<Output = DaemonResult<()>> + Send + 'a {
        async move {
            self.save_message(client_ids_key(key), &MqttClientIds { client_ids });
            Ok(())
        }
    }

    fn load_mqtt_client_ids<'a>(
        &'a self,
        key: &'a ResourceKey,
    ) -> impl Future<Output = DaemonResult<Vec<String>>> + Send + 'a {
        async move {
            Ok(self
                .load_message::<MqttClientIds>(&client_ids_key(key), MQTT_CLIENT_IDS_LABEL)?
                .map(|client_ids| client_ids.client_ids)
                .unwrap_or_default())
        }
    }
}

impl InMemoryDaemonStore {
    fn save_message<M>(&mut self, key: Vec<u8>, message: &M)
    where
        M: Message,
    {
        self.entries.insert(key, message.encode_to_vec());
    }

    fn load_message<M>(&self, key: &[u8], label: &str) -> DaemonResult<Option<M>>
    where
        M: Message + Default,
    {
        self.entries
            .get(key)
            .map(|bytes| {
                M::decode(bytes.as_slice())
                    .map_err(|error| DaemonError::store(format!("failed to decode {label}: {error}")))
            })
            .transpose()
    }
}

fn required_pipeline_id(id: Option<&ResourceId>) -> DaemonResult<&ResourceId> {
    let id = id.ok_or_else(|| DaemonError::invalid_state("pipeline id is missing"))?;
    if id.name.trim().is_empty() {
        return Err(DaemonError::invalid_state("pipeline name is missing"));
    }
    if id.version.trim().is_empty() {
        return Err(DaemonError::invalid_state("pipeline version is missing"));
    }
    Ok(id)
}

fn pipeline_key(key: &ResourceKey) -> Vec<u8> {
    store_key(PIPELINE_KEY_PREFIX, key.as_store_key().as_bytes())
}

fn pipeline_metadata_key(name: &str) -> Vec<u8> {
    store_key(PIPELINE_METADATA_KEY_PREFIX, name.as_bytes())
}

fn client_ids_key(key: &ResourceKey) -> Vec<u8> {
    store_key(MQTT_CLIENT_IDS_KEY_PREFIX, key.as_store_key().as_bytes())
}

fn store_key(prefix: &[u8], suffix: &[u8]) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(prefix);
    bytes.push(b':');
    bytes.extend_from_slice(suffix);
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use tenon_message::plan::{
        auth_plan, DeliveryMode, EgressPlan, ExecutionMode, MqttBrokerPlan, MqttSourcePlan,
        MqttSubscriptionPlan, NoAuth, PayloadDecodePlan, ProcessPlan, ScriptRuntime,
    };

    #[test]
    fn builds_readable_byte_keys() {
        let key = key("sensor", "v1");

        assert_eq!(pipeline_key(&key), b"pipeline:sensor:v1".to_vec());
        assert_eq!(pipeline_metadata_key("sensor"), b"pipeline_metadata:sensor".to_vec());
        assert_eq!(
            client_ids_key(&key),
            b"mqtt_source_client_ids:sensor:v1".to_vec()
        );
    }

    #[test]
    fn saves_and_loads_pipeline_snapshot() {
        block_on(async {
            let mut store = InMemoryDaemonStore::new();
            let plan = plan();
            let plan_id = plan.id.clone().expect("plan id");

            store.save_pipeline(plan.clone()).await.expect("save plan");

            assert_eq!(
                store
                    .load_pipeline(&plan_id)
                    .await
                    .expect("load plan"),
                Some(plan.clone())
            );
            assert_eq!(
                store
                    .load_latest_pipeline_id("sensor-pipeline")
                    .await
                    .expect("load latest pipeline"),
                Some(plan_id)
            );
        });
    }

    #[test]
    fn rejects_duplicate_pipeline_revision() {
        block_on(async {
            let mut store = InMemoryDaemonStore::new();
            let plan = plan();

            store.save_pipeline(plan.clone()).await.expect("save plan");
            let error = store
                .save_pipeline(plan)
                .await
                .expect_err("duplicate plan should be rejected");

            assert_eq!(error.kind, crate::DaemonErrorKind::InvalidState);
            assert!(error.message.contains("already exists"));
        });
    }

    #[test]
    fn saves_and_loads_mqtt_client_ids() {
        block_on(async {
            let mut store = InMemoryDaemonStore::new();
            let source_key = key("sensor-source", "v1");

            assert!(
                store
                    .load_mqtt_client_ids(&source_key)
                    .await
                    .expect("load missing ids")
                    .is_empty()
            );

            store
                .save_mqtt_client_ids(
                    &source_key,
                    vec!["client-0".to_string(), "client-1".to_string()],
                )
                .await
                .expect("save ids");

            assert_eq!(
                store
                    .load_mqtt_client_ids(&source_key)
                    .await
                    .expect("load ids"),
                vec!["client-0".to_string(), "client-1".to_string()]
            );
        });
    }

    fn key(name: &str, version: &str) -> ResourceKey {
        ResourceKey {
            name: name.to_string(),
            version: version.to_string(),
        }
    }

    fn id(name: &str, version: &str) -> ResourceId {
        ResourceId {
            name: name.to_string(),
            version: version.to_string(),
        }
    }

    fn plan() -> DeploymentPlan {
        DeploymentPlan {
            id: Some(id("sensor-pipeline", "v1")),
            execution: ExecutionMode::IntraProc as i32,
            sources: vec![MqttSourcePlan {
                broker: Some(MqttBrokerPlan {
                    host: "127.0.0.1".to_string(),
                    port: 1883,
                }),
                auth: Some(tenon_message::plan::AuthPlan {
                    kind: Some(auth_plan::Kind::None(NoAuth {})),
                }),
                subscriptions: vec![MqttSubscriptionPlan {
                    topic: "sensors/+/data".to_string(),
                    decode: PayloadDecodePlan::Json as i32,
                }],
                client_count: 3,
            }],
            process: Some(ProcessPlan {
                runtime: ScriptRuntime::Lua as i32,
                source: "function on_message(ctx, msg) end".to_string(),
            }),
            egress: Some(EgressPlan {
                delivery: DeliveryMode::Single as i32,
            }),
        }
    }
}
