use crate::{DaemonError, DaemonResult, ResourceKey};
use prost::Message;
use std::collections::HashMap;
use std::future::Future;
use tenon_message::plan::{
    DeploymentPlan, EgressPlan, MqttClientIds, MqttSourcePlan, ProcessPlan, ResourceId,
    ResourceKind,
};

const PLAN_LABEL: &str = "deployment plan";
const MQTT_SOURCE_LABEL: &str = "MQTT source";
const PROCESS_LABEL: &str = "process";
const EGRESS_LABEL: &str = "egress";
const MQTT_CLIENT_IDS_LABEL: &str = "MQTT client ids";

const PLAN_KEY_PREFIX: &[u8] = b"plan";
const RESOURCE_KEY_PREFIX: &[u8] = b"resource";
const MQTT_CLIENT_IDS_KEY_PREFIX: &[u8] = b"mqtt_source_client_ids";

pub trait DaemonStore {
    fn save_plan<'a>(
        &'a mut self,
        key: &'a ResourceKey,
        plan: DeploymentPlan,
    ) -> impl Future<Output = DaemonResult<()>> + Send + 'a;

    fn load_plan<'a>(
        &'a self,
        key: &'a ResourceKey,
    ) -> impl Future<Output = DaemonResult<Option<DeploymentPlan>>> + Send + 'a;

    fn load_mqtt_source<'a>(
        &'a self,
        key: &'a ResourceKey,
    ) -> impl Future<Output = DaemonResult<Option<MqttSourcePlan>>> + Send + 'a;

    fn load_process<'a>(
        &'a self,
        key: &'a ResourceKey,
    ) -> impl Future<Output = DaemonResult<Option<ProcessPlan>>> + Send + 'a;

    fn load_egress<'a>(
        &'a self,
        key: &'a ResourceKey,
    ) -> impl Future<Output = DaemonResult<Option<EgressPlan>>> + Send + 'a;

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
    fn save_plan<'a>(
        &'a mut self,
        key: &'a ResourceKey,
        plan: DeploymentPlan,
    ) -> impl Future<Output = DaemonResult<()>> + Send + 'a {
        async move {
            self.save_message(plan_key(key), &plan);
            self.save_plan_resources(&plan)?;
            Ok(())
        }
    }

    fn load_plan<'a>(
        &'a self,
        key: &'a ResourceKey,
    ) -> impl Future<Output = DaemonResult<Option<DeploymentPlan>>> + Send + 'a {
        async move { self.load_message(&plan_key(key), PLAN_LABEL) }
    }

    fn load_mqtt_source<'a>(
        &'a self,
        key: &'a ResourceKey,
    ) -> impl Future<Output = DaemonResult<Option<MqttSourcePlan>>> + Send + 'a {
        async move { self.load_message(&resource_key(key), MQTT_SOURCE_LABEL) }
    }

    fn load_process<'a>(
        &'a self,
        key: &'a ResourceKey,
    ) -> impl Future<Output = DaemonResult<Option<ProcessPlan>>> + Send + 'a {
        async move { self.load_message(&resource_key(key), PROCESS_LABEL) }
    }

    fn load_egress<'a>(
        &'a self,
        key: &'a ResourceKey,
    ) -> impl Future<Output = DaemonResult<Option<EgressPlan>>> + Send + 'a {
        async move { self.load_message(&resource_key(key), EGRESS_LABEL) }
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
    fn save_plan_resources(&mut self, plan: &DeploymentPlan) -> DaemonResult<()> {
        for source in &plan.sources {
            let key = resource_key_from_required_id(source.id.as_ref(), ResourceKind::MqttSource)?;
            self.save_message(resource_key(&key), source);
        }

        if let Some(process) = &plan.process {
            let key = resource_key_from_required_id(process.id.as_ref(), ResourceKind::Process)?;
            self.save_message(resource_key(&key), process);
        }

        if let Some(egress) = &plan.egress {
            let key = resource_key_from_required_id(egress.id.as_ref(), ResourceKind::Egress)?;
            self.save_message(resource_key(&key), egress);
        }

        Ok(())
    }

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

fn resource_key_from_required_id(
    id: Option<&ResourceId>,
    expected_kind: ResourceKind,
) -> DaemonResult<ResourceKey> {
    let id = id.ok_or_else(|| {
        DaemonError::invalid_state(format!("{expected_kind} resource id is missing"))
    })?;
    if ResourceKind::try_from(id.kind) != Ok(expected_kind) {
        return Err(DaemonError::invalid_state(format!(
            "expected {expected_kind} resource id, got kind {}",
            id.kind
        )));
    }
    Ok(ResourceKey::from_id(id))
}

fn plan_key(key: &ResourceKey) -> Vec<u8> {
    store_key(PLAN_KEY_PREFIX, key)
}

fn resource_key(key: &ResourceKey) -> Vec<u8> {
    store_key(RESOURCE_KEY_PREFIX, key)
}

fn client_ids_key(key: &ResourceKey) -> Vec<u8> {
    store_key(MQTT_CLIENT_IDS_KEY_PREFIX, key)
}

fn store_key(prefix: &[u8], key: &ResourceKey) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(prefix);
    bytes.push(b':');
    bytes.extend_from_slice(key.as_store_key().as_bytes());
    bytes
}
