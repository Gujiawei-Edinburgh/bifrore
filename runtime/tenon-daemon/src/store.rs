use crate::{DaemonError, DaemonResult, ResourceKey};
use prost::Message;
use std::collections::HashMap;
use std::future::Future;
use tenon_message::plan::{
    DeploymentPlan, EgressPlan, MqttClientIds, MqttSourcePlan, ProcessPlan, ResourceId,
    ResourceKind, ResourceReferences, StoredDeploymentPlan,
};

const PLAN_LABEL: &str = "deployment plan";
const MQTT_SOURCE_LABEL: &str = "MQTT source";
const PROCESS_LABEL: &str = "process";
const EGRESS_LABEL: &str = "egress";
const MQTT_CLIENT_IDS_LABEL: &str = "MQTT client ids";
const RESOURCE_REFERENCES_LABEL: &str = "resource references";

const PLAN_KEY_PREFIX: &[u8] = b"plan";
const RESOURCE_KEY_PREFIX: &[u8] = b"resource";
const MQTT_CLIENT_IDS_KEY_PREFIX: &[u8] = b"mqtt_source_client_ids";
const RESOURCE_REFERENCES_KEY_PREFIX: &[u8] = b"resource_refs";

pub trait DaemonStore {
    fn save_plan<'a>(
        &'a mut self,
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

    fn load_referencing_plans<'a>(
        &'a self,
        key: &'a ResourceKey,
    ) -> impl Future<Output = DaemonResult<Vec<ResourceId>>> + Send + 'a;
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
        plan: DeploymentPlan,
    ) -> impl Future<Output = DaemonResult<()>> + Send + 'a {
        async move {
            let key = resource_key_from_required_id(plan.id.as_ref(), ResourceKind::Pipeline)?;
            let stored_plan = stored_plan_from_deployment_plan(&plan)?;
            self.save_message(plan_key(&key), &stored_plan);
            self.save_plan_resources(&plan)?;
            self.save_plan_reverse_refs(&stored_plan)?;
            Ok(())
        }
    }

    fn load_plan<'a>(
        &'a self,
        key: &'a ResourceKey,
    ) -> impl Future<Output = DaemonResult<Option<DeploymentPlan>>> + Send + 'a {
        async move {
            let Some(stored_plan) =
                self.load_message::<StoredDeploymentPlan>(&plan_key(key), PLAN_LABEL)?
            else {
                return Ok(None);
            };
            self.resolve_stored_plan(stored_plan).map(Some)
        }
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

    fn load_referencing_plans<'a>(
        &'a self,
        key: &'a ResourceKey,
    ) -> impl Future<Output = DaemonResult<Vec<ResourceId>>> + Send + 'a {
        async move {
            Ok(self
                .load_message::<ResourceReferences>(
                    &resource_references_key(key),
                    RESOURCE_REFERENCES_LABEL,
                )?
                .map(|references| references.pipeline_refs)
                .unwrap_or_default())
        }
    }
}

impl InMemoryDaemonStore {
    fn resolve_stored_plan(&self, stored_plan: StoredDeploymentPlan) -> DaemonResult<DeploymentPlan> {
        let sources = stored_plan
            .source_refs
            .iter()
            .map(|source_id| {
                self.load_required_resource::<MqttSourcePlan>(
                    Some(source_id),
                    ResourceKind::MqttSource,
                    MQTT_SOURCE_LABEL,
                )
            })
            .collect::<DaemonResult<Vec<_>>>()?;
        let process = self.load_required_resource::<ProcessPlan>(
            stored_plan.process_ref.as_ref(),
            ResourceKind::Process,
            PROCESS_LABEL,
        )?;
        let egress = self.load_required_resource::<EgressPlan>(
            stored_plan.egress_ref.as_ref(),
            ResourceKind::Egress,
            EGRESS_LABEL,
        )?;

        Ok(DeploymentPlan {
            id: stored_plan.id,
            execution: stored_plan.execution,
            sources,
            process: Some(process),
            egress: Some(egress),
        })
    }

    fn load_required_resource<M>(
        &self,
        id: Option<&ResourceId>,
        expected_kind: ResourceKind,
        label: &str,
    ) -> DaemonResult<M>
    where
        M: Message + Default,
    {
        let key = resource_key_from_required_id(id, expected_kind)?;
        self.load_message::<M>(&resource_key(&key), label)?
            .ok_or_else(|| DaemonError::not_found(format!("missing {label} {}", key.as_store_key())))
    }

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

    fn save_plan_reverse_refs(&mut self, stored_plan: &StoredDeploymentPlan) -> DaemonResult<()> {
        let pipeline_id =
            resource_id_from_required_id(stored_plan.id.as_ref(), ResourceKind::Pipeline)?;

        for source_ref in &stored_plan.source_refs {
            self.add_reverse_ref(Some(source_ref), ResourceKind::MqttSource, pipeline_id)?;
        }
        self.add_reverse_ref(
            stored_plan.process_ref.as_ref(),
            ResourceKind::Process,
            pipeline_id,
        )?;
        self.add_reverse_ref(
            stored_plan.egress_ref.as_ref(),
            ResourceKind::Egress,
            pipeline_id,
        )?;

        Ok(())
    }

    fn add_reverse_ref(
        &mut self,
        resource_id: Option<&ResourceId>,
        expected_kind: ResourceKind,
        pipeline_id: &ResourceId,
    ) -> DaemonResult<()> {
        let key = resource_key_from_required_id(resource_id, expected_kind)?;
        let storage_key = resource_references_key(&key);
        let mut references = self
            .load_message::<ResourceReferences>(&storage_key, RESOURCE_REFERENCES_LABEL)?
            .unwrap_or_default();

        if !references.pipeline_refs.iter().any(|id| id == pipeline_id) {
            references.pipeline_refs.push(pipeline_id.clone());
            self.save_message(storage_key, &references);
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

fn stored_plan_from_deployment_plan(plan: &DeploymentPlan) -> DaemonResult<StoredDeploymentPlan> {
    let source_refs = plan
        .sources
        .iter()
        .map(|source| {
            resource_id_from_required_id(source.id.as_ref(), ResourceKind::MqttSource)
                .map(ToOwned::to_owned)
        })
        .collect::<DaemonResult<Vec<_>>>()?;
    let process_ref = Some(
        resource_id_from_required_id(
            plan.process.as_ref().and_then(|process| process.id.as_ref()),
            ResourceKind::Process,
        )?
        .clone(),
    );
    let egress_ref = Some(
        resource_id_from_required_id(
            plan.egress.as_ref().and_then(|egress| egress.id.as_ref()),
            ResourceKind::Egress,
        )?
        .clone(),
    );

    Ok(StoredDeploymentPlan {
        id: plan.id.clone(),
        execution: plan.execution,
        source_refs,
        process_ref,
        egress_ref,
    })
}

fn resource_id_from_required_id(
    id: Option<&ResourceId>,
    expected_kind: ResourceKind,
) -> DaemonResult<&ResourceId> {
    let id = id.ok_or_else(|| {
        DaemonError::invalid_state(format!("{expected_kind} resource id is missing"))
    })?;
    if ResourceKind::try_from(id.kind) != Ok(expected_kind) {
        return Err(DaemonError::invalid_state(format!(
            "expected {expected_kind} resource id, got kind {}",
            id.kind
        )));
    }
    Ok(id)
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

fn resource_references_key(key: &ResourceKey) -> Vec<u8> {
    store_key(RESOURCE_REFERENCES_KEY_PREFIX, key)
}

fn store_key(prefix: &[u8], key: &ResourceKey) -> Vec<u8> {
    let mut bytes = Vec::new();
    bytes.extend_from_slice(prefix);
    bytes.push(b':');
    bytes.extend_from_slice(key.as_store_key().as_bytes());
    bytes
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;
    use tenon_message::plan::{
        DeliveryMode, ExecutionMode, MqttBrokerPlan, ModulePlan, ModuleRuntime,
        PayloadDecodePlan, MqttSubscriptionPlan,
    };

    #[test]
    fn builds_readable_byte_keys() {
        let key = key(ResourceKind::MqttSource, "sensor", "v1");

        assert_eq!(
            plan_key(&key),
            b"plan:mqtt_source:sensor:v1".to_vec()
        );
        assert_eq!(
            resource_key(&key),
            b"resource:mqtt_source:sensor:v1".to_vec()
        );
        assert_eq!(
            client_ids_key(&key),
            b"mqtt_source_client_ids:mqtt_source:sensor:v1".to_vec()
        );
        assert_eq!(
            resource_references_key(&key),
            b"resource_refs:mqtt_source:sensor:v1".to_vec()
        );
    }

    #[test]
    fn saves_plan_component_resources_and_reverse_refs() {
        block_on(async {
            let mut store = InMemoryDaemonStore::new();
            let plan = plan();
            let plan_key = key(ResourceKind::Pipeline, "sensor-pipeline", "v2");

            store
                .save_plan(plan.clone())
                .await
                .expect("save plan");

            let stored_plan = StoredDeploymentPlan::decode(
                store
                    .entries
                    .get(&super::plan_key(&plan_key))
                    .expect("stored plan bytes")
                    .as_slice(),
            )
            .expect("stored plan");
            assert_eq!(stored_plan.source_refs.len(), 1);
            let loaded_plan = store
                .load_plan(&plan_key)
                .await
                .expect("load plan")
                .expect("plan");
            assert_eq!(loaded_plan, plan);

            let source_id = loaded_plan.sources[0].id.as_ref().expect("source id");
            let process_id = loaded_plan
                .process
                .as_ref()
                .and_then(|process| process.id.as_ref())
                .expect("process id");
            let egress_id = loaded_plan
                .egress
                .as_ref()
                .and_then(|egress| egress.id.as_ref())
                .expect("egress id");

            assert_eq!(stored_plan.source_refs[0], *source_id);
            assert_eq!(stored_plan.process_ref.as_ref(), Some(process_id));
            assert_eq!(stored_plan.egress_ref.as_ref(), Some(egress_id));

            assert_eq!(
                store
                    .load_mqtt_source(&ResourceKey::from_id(source_id))
                    .await
                    .expect("load source")
                    .expect("source")
                    .client_count,
                3
            );
            assert_eq!(
                store
                    .load_process(&ResourceKey::from_id(process_id))
                    .await
                    .expect("load process")
                    .expect("process")
                    .module
                    .expect("module")
                    .source,
                "function on_message(ctx, msg) end"
            );
            assert_eq!(
                store
                    .load_egress(&ResourceKey::from_id(egress_id))
                    .await
                    .expect("load egress")
                    .expect("egress")
                    .delivery,
                DeliveryMode::Single as i32
            );
            assert_eq!(
                store
                    .load_referencing_plans(&ResourceKey::from_id(source_id))
                    .await
                    .expect("load source references"),
                vec![loaded_plan.id.clone().expect("plan id")]
            );
            assert_eq!(
                store
                    .load_referencing_plans(&ResourceKey::from_id(process_id))
                    .await
                    .expect("load process references"),
                vec![loaded_plan.id.clone().expect("plan id")]
            );
            assert_eq!(
                store
                    .load_referencing_plans(&ResourceKey::from_id(egress_id))
                    .await
                    .expect("load egress references"),
                vec![loaded_plan.id.clone().expect("plan id")]
            );
        });
    }

    #[test]
    fn reverse_refs_are_deduplicated() {
        block_on(async {
            let mut store = InMemoryDaemonStore::new();
            let plan = plan();
            let source_id = plan.sources[0].id.clone().expect("source id");

            store.save_plan(plan.clone()).await.expect("save plan");
            store.save_plan(plan.clone()).await.expect("save plan again");

            assert_eq!(
                store
                    .load_referencing_plans(&ResourceKey::from_id(&source_id))
                    .await
                    .expect("load source references"),
                vec![plan.id.expect("plan id")]
            );
        });
    }

    #[test]
    fn saves_and_loads_mqtt_client_ids() {
        block_on(async {
            let mut store = InMemoryDaemonStore::new();
            let source_key = key(ResourceKind::MqttSource, "sensor-source", "v1");

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

    #[test]
    fn rejects_plan_with_wrong_resource_kind() {
        block_on(async {
            let mut store = InMemoryDaemonStore::new();
            let mut plan = plan();
            plan.sources[0].id = Some(id(ResourceKind::Process, "sensor-source", "v1"));

            let error = store
                .save_plan(plan)
                .await
                .expect_err("save should reject wrong kind");

            assert_eq!(error.kind, crate::DaemonErrorKind::InvalidState);
            assert!(error.message.contains("expected MqttSource resource id"));
        });
    }

    fn plan() -> DeploymentPlan {
        DeploymentPlan {
            id: Some(id(ResourceKind::Pipeline, "sensor-pipeline", "v2")),
            execution: ExecutionMode::IntraProc as i32,
            sources: vec![MqttSourcePlan {
                id: Some(id(ResourceKind::MqttSource, "sensor-source", "v1")),
                broker: Some(MqttBrokerPlan {
                    host: "127.0.0.1".to_string(),
                    port: 1883,
                }),
                auth: None,
                subscriptions: vec![MqttSubscriptionPlan {
                    topic: "sensor/+/data".to_string(),
                    decode: PayloadDecodePlan::Json as i32,
                }],
                client_count: 3,
            }],
            process: Some(ProcessPlan {
                id: Some(id(ResourceKind::Process, "sensor-process", "v5")),
                module: Some(ModulePlan {
                    id: Some(id(ResourceKind::Module, "sensor-module", "v5")),
                    runtime: ModuleRuntime::Lua as i32,
                    source: "function on_message(ctx, msg) end".to_string(),
                }),
            }),
            egress: Some(EgressPlan {
                id: Some(id(ResourceKind::Egress, "sensor-egress", "v1")),
                delivery: DeliveryMode::Single as i32,
            }),
        }
    }

    fn key(kind: ResourceKind, name: &str, version: &str) -> ResourceKey {
        ResourceKey {
            kind: kind as i32,
            name: name.to_string(),
            version: version.to_string(),
        }
    }

    fn id(kind: ResourceKind, name: &str, version: &str) -> ResourceId {
        ResourceId {
            kind: kind as i32,
            name: name.to_string(),
            version: version.to_string(),
        }
    }
}
