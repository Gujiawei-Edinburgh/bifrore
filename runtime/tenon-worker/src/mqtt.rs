use crate::auth::resolve_auth;
use crate::{WorkerError, WorkerResult};
use std::collections::HashMap;
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::Duration;
use tenon_extension::{AuthResult, ExtensionValue, Message, MqttMetadata, SourceContext, Topic};
use tenon_message::plan::{MqttSourcePlan, ResourceId};

#[derive(Debug, Clone)]
pub struct MqttAdapterConfig {
    pub pipeline_id: ResourceId,
    pub source: MqttSourcePlan,
    pub client_ids: Vec<String>,
    pub group_name: String,
    pub io_threads: usize,
    pub queue_capacity: usize,
    pub clean_start: bool,
    pub session_expiry_interval: u32,
    pub keep_alive_secs: u16,
}

impl MqttAdapterConfig {
    fn shared_subscription(&self, topic_filter: &str) -> String {
        format!("$share/{}/{}", self.group_name, topic_filter)
    }

    fn source_context(&self) -> SourceContext {
        SourceContext::new(&self.pipeline_id.name, &self.pipeline_id.version)
    }
}

pub struct IncomingDelivery {
    pub message: Message,
    ack: Option<DeferredAck>,
}

impl IncomingDelivery {
    pub fn ack(self) {
        if let Some(ack) = self.ack {
            if let Err(error) = ack.client.try_ack(&ack.publish) {
                log::warn!("failed to send MQTT ack after process: {error}");
            }
        }
    }

    #[cfg(test)]
    pub(crate) fn without_ack(message: Message) -> Self {
        Self { message, ack: None }
    }
}

type DeliveryHandler = Arc<dyn Fn(IncomingDelivery) + Send + Sync + 'static>;

pub struct MqttAdapterHandle {
    stop: Option<tokio::sync::oneshot::Sender<()>>,
    runtime_thread: Option<JoinHandle<()>>,
}

impl MqttAdapterHandle {
    pub fn stop(mut self) -> WorkerResult<()> {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
        if let Some(runtime_thread) = self.runtime_thread.take() {
            runtime_thread
                .join()
                .map_err(|_| WorkerError::mqtt("MQTT runtime thread panicked"))?;
        }
        Ok(())
    }
}

struct DeferredAck {
    client: rumqttc::v5::AsyncClient,
    publish: rumqttc::v5::mqttbytes::v5::Publish,
}

pub fn start_mqtt(
    config: MqttAdapterConfig,
    handler: DeliveryHandler,
) -> WorkerResult<MqttAdapterHandle> {
    use rumqttc::v5::{AsyncClient, MqttOptions};
    use std::sync::mpsc;

    let broker = config
        .source
        .broker
        .clone()
        .ok_or_else(|| WorkerError::mqtt("MQTT broker plan is missing"))?;
    let topics: Vec<String> = config
        .source
        .subscriptions
        .iter()
        .map(|subscription| subscription.topic.clone())
        .collect();
    if topics.is_empty() {
        return Err(WorkerError::mqtt("MQTT source has no subscriptions"));
    }

    if config.client_ids.is_empty() {
        return Err(WorkerError::mqtt("MQTT source client IDs are missing"));
    }
    let client_count = config.client_ids.len();
    let io_threads = config.io_threads.max(1);
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    let (ready_tx, ready_rx) = mpsc::channel::<WorkerResult<()>>();
    let handler_runtime = handler.clone();

    let runtime_thread = std::thread::spawn(move || {
        let runtime = match tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(io_threads)
            .thread_name("tenon-worker-mqtt")
            .build()
        {
            Ok(runtime) => runtime,
            Err(error) => {
                let _ = ready_tx.send(Err(WorkerError::mqtt(format!(
                    "failed to create MQTT runtime: {error}"
                ))));
                return;
            }
        };

        runtime.block_on(async move {
            let (shutdown_tx, _) = tokio::sync::watch::channel(false);
            let mut tasks = Vec::with_capacity(client_count as usize);
            let mut subscribed_clients = 0usize;

            let auth = match resolve_auth(config.source.auth.as_ref(), &config.source_context()) {
                Ok(auth) => auth,
                Err(error) => {
                    let _ = ready_tx.send(Err(error));
                    return;
                }
            };

            for client_id in config.client_ids.iter() {
                let mut mqtt_options =
                    MqttOptions::new(client_id.clone(), broker.host.clone(), broker.port as u16);
                mqtt_options.set_keep_alive(Duration::from_secs(config.keep_alive_secs.into()));
                mqtt_options.set_clean_start(config.clean_start);
                mqtt_options.set_manual_acks(true);
                let mut connect_properties = rumqttc::v5::mqttbytes::v5::ConnectProperties::new();
                connect_properties.session_expiry_interval = Some(config.session_expiry_interval);
                mqtt_options.set_connect_properties(connect_properties);
                if let Err(error) = apply_auth(&mut mqtt_options, auth.as_ref()) {
                    let _ = ready_tx.send(Err(error));
                    return;
                }

                let (client, event_loop) =
                    AsyncClient::new(mqtt_options, config.queue_capacity.max(1));
                if subscribe_topics(&client, &config, &topics).await.is_err() {
                    continue;
                }

                subscribed_clients += 1;
                let handler = handler_runtime.clone();
                let shutdown_rx = shutdown_tx.subscribe();
                let source_context = config.source_context();
                tasks.push(tokio::spawn(run_event_loop(
                    client.clone(),
                    event_loop,
                    source_context,
                    handler,
                    shutdown_rx,
                )));
            }

            if subscribed_clients == 0 {
                let _ = ready_tx.send(Err(WorkerError::mqtt("no MQTT clients subscribed")));
            } else {
                let _ = ready_tx.send(Ok(()));
            }

            let _ = stop_rx.await;
            let _ = shutdown_tx.send(true);
            for task in tasks {
                let _ = task.await;
            }
        });
    });

    match ready_rx.recv_timeout(Duration::from_secs(10)) {
        Ok(Ok(())) => Ok(MqttAdapterHandle {
            stop: Some(stop_tx),
            runtime_thread: Some(runtime_thread),
        }),
        Ok(Err(error)) => {
            let _ = stop_tx.send(());
            let _ = runtime_thread.join();
            Err(error)
        }
        Err(_) => {
            let _ = stop_tx.send(());
            let _ = runtime_thread.join();
            Err(WorkerError::mqtt("timeout waiting MQTT startup readiness"))
        }
    }
}

fn apply_auth(
    mqtt_options: &mut rumqttc::v5::MqttOptions,
    auth: Option<&AuthResult>,
) -> WorkerResult<()> {
    let Some(auth) = auth else {
        return Ok(());
    };
    match auth {
        AuthResult::UsernamePassword { username, password } => {
            mqtt_options.set_credentials(username.clone(), password.clone());
            Ok(())
        }
        AuthResult::Custom {
            username,
            password,
            properties,
        } => {
            if let (Some(username), Some(password)) = (username, password) {
                mqtt_options.set_credentials(username.clone(), password.clone());
            }
            let mut connect_properties = mqtt_options
                .connect_properties()
                .unwrap_or_else(rumqttc::v5::mqttbytes::v5::ConnectProperties::new);
            connect_properties.user_properties.extend(properties.clone());
            mqtt_options.set_connect_properties(connect_properties);
            Ok(())
        }
        AuthResult::ClientCertificate {
            cert_path,
            key_path,
            ca_path,
        } => {
            let ca_path = ca_path.as_deref().ok_or_else(|| {
                WorkerError::mqtt(
                    "client-certificate MQTT auth requires ca_path for server certificate verification",
                )
            })?;
            let ca = read_auth_file(ca_path)?;
            let certificate = read_auth_file(cert_path)?;
            let private_key = read_auth_file(key_path)?;
            mqtt_options.set_transport(rumqttc::Transport::tls(
                ca,
                Some((certificate, private_key)),
                None,
            ));
            Ok(())
        }
    }
}

fn read_auth_file(path: &str) -> WorkerResult<Vec<u8>> {
    std::fs::read(path).map_err(|error| {
        WorkerError::mqtt(format!("failed to read MQTT TLS credential {path}: {error}"))
    })
}

async fn subscribe_topics(
    client: &rumqttc::v5::AsyncClient,
    config: &MqttAdapterConfig,
    topics: &[String],
) -> WorkerResult<()> {
    for topic in topics {
        let shared = config.shared_subscription(topic);
        client
            .subscribe(shared, rumqttc::v5::mqttbytes::QoS::AtLeastOnce)
            .await
            .map_err(|error| WorkerError::mqtt(error.to_string()))?;
    }
    Ok(())
}

async fn run_event_loop(
    client: rumqttc::v5::AsyncClient,
    mut event_loop: rumqttc::v5::EventLoop,
    source_context: SourceContext,
    handler: DeliveryHandler,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) {
    loop {
        tokio::select! {
            changed = shutdown_rx.changed() => {
                if changed.is_ok() && *shutdown_rx.borrow() {
                    break;
                }
            }
            event = event_loop.poll() => {
                match event {
                    Ok(rumqttc::v5::Event::Incoming(rumqttc::v5::mqttbytes::v5::Packet::Publish(publish))) => {
                        let message = build_message(source_context.clone(), &publish);
                        handler(IncomingDelivery {
                            message,
                            ack: Some(DeferredAck {
                                client: client.clone(),
                                publish,
                            }),
                        });
                    }
                    Ok(_) => {}
                    Err(error) => {
                        log::error!("MQTT event loop error: {error}");
                        break;
                    }
                }
            }
        }
    }
}

fn build_message(
    source_context: SourceContext,
    publish: &rumqttc::v5::mqttbytes::v5::Publish,
) -> Message {
    let topic = String::from_utf8_lossy(&publish.topic).into_owned();
    let raw_payload = publish.payload.to_vec();
    let payload = decode_json_payload(&raw_payload);
    let metadata = MqttMetadata::new(
        publish.pkid,
        publish.qos as u8,
        publish.retain,
        publish.dup,
    );
    Message::new(
        source_context,
        Topic::new(topic),
        payload,
        raw_payload,
        metadata,
        properties(publish),
    )
}

fn decode_json_payload(raw_payload: &[u8]) -> ExtensionValue {
    serde_json::from_slice(raw_payload).unwrap_or(ExtensionValue::Null)
}

fn properties(publish: &rumqttc::v5::mqttbytes::v5::Publish) -> HashMap<String, String> {
    let mut properties = HashMap::new();
    if let Some(properties_plan) = publish.properties.as_ref() {
        if let Some(content_type) = properties_plan.content_type.as_ref() {
            properties.insert("content-type".to_string(), content_type.clone());
        }
        for (key, value) in properties_plan.user_properties.iter() {
            properties.insert(key.clone(), value.clone());
        }
    }
    properties
}
