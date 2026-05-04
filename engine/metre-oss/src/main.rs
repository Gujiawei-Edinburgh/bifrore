mod config;
mod logger;
mod metrics;
mod sink;

use config::OssConfig;
use metrics::{start_metrics_server, OssMetrics};
use metre_coordinator::EngineCoordinator;
use metre_core::mqtt::{start_mqtt, IncomingDelivery, MessageHandler, MqttConfig};
use metre_core::payload::{
    dynamic_protobuf_registry_from_descriptor_set_file, PayloadDecoder, PayloadFormat,
};
use metre_core::runtime::RuleEngine;
use sink::Dispatcher;
use std::env;
use std::fs;
use std::sync::Arc;
use std::thread;
use std::thread::JoinHandle;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

fn main() {
    if let Err(err) = run() {
        eprintln!("metre-oss failed: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    let _log_guard = logger::init()?;
    let config_path = parse_config_path(env::args().collect())?;
    let config = load_config(&config_path)?;
    let coordinator = EngineCoordinator::from_oss_files(
        config.rule_json_path.clone(),
        config.client_ids_path.clone(),
    );
    let rule_bytes = coordinator.load_rule_bytes()?;
    let metrics_config = config.metrics.clone();
    let mut rule_engine = build_rule_engine(&config)?;
    rule_engine.set_detailed_latency_metrics(metrics_config.detailed_latency);
    let rule_count = rule_engine
        .load_rules_from_json_bytes(&rule_bytes)
        .map_err(|err| err.to_string())?;
    let eval_metrics = rule_engine.metrics_handle();
    let oss_metrics = Arc::new(OssMetrics::default());
    let topic_filters = rule_engine.topic_filters();
    let rule_metadata = rule_engine.rule_metadata();
    let node_id = config
        .mqtt
        .node_id
        .clone()
        .unwrap_or_else(generate_default_node_id);
    let requested_client_count = config.mqtt.client_count.max(1);
    let client_ids = coordinator.resolve_client_ids(&node_id, requested_client_count);
    coordinator.persist_client_ids(&client_ids)?;
    let eval_queue_capacity = config.mqtt.queue_capacity.max(1) as usize;

    let mqtt_config = MqttConfig {
        host: config.mqtt.host,
        port: config.mqtt.port,
        node_id,
        client_count: client_ids.len().max(1) as u16,
        client_ids,
        io_threads: config.mqtt.io_threads,
        eval_threads: 1,
        queue_capacity: config.mqtt.queue_capacity,
        username: config.mqtt.username,
        password: config.mqtt.password,
        clean_start: config.mqtt.clean_start,
        session_expiry_interval: config.mqtt.session_expiry_interval,
        group_name: config.mqtt.group_name,
        ordered: config.mqtt.ordered,
        ordered_prefix: config.mqtt.ordered_prefix,
        keep_alive_secs: config.mqtt.keep_alive_secs,
    };
    let metrics_worker = start_metrics_server(metrics_config, Arc::clone(&eval_metrics))?;
    let dispatcher = Arc::new(Dispatcher::new(
        config.sinks,
        rule_metadata,
        Arc::clone(&oss_metrics),
    )?);
    let (handler, eval_worker) = build_handler(
        rule_engine,
        dispatcher,
        eval_queue_capacity,
        Arc::clone(&oss_metrics),
    );

    log::info!(
        "starting metre-oss config={} rules={} topics={} clients={}",
        config_path,
        rule_count,
        topic_filters.len(),
        mqtt_config.client_count
    );
    let adapter = start_mqtt(mqtt_config, topic_filters, handler)
        .map_err(|err| format!("{err:?}"))?;
    log::info!("metre-oss started; press Ctrl+C to stop");
    let _adapter = adapter;
    let _eval_worker = eval_worker;
    metrics_worker
        .join()
        .map_err(|_| "metrics endpoint thread panicked".to_string())
}

fn parse_config_path(args: Vec<String>) -> Result<String, String> {
    let index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "-c" | "--config" => {
                let Some(path) = args.get(index + 1) else {
                    return Err("missing value for -c/--config".to_string());
                };
                return Ok(path.clone());
            }
            "-h" | "--help" => {
                print_usage(&args[0]);
                std::process::exit(0);
            }
            value => return Err(format!("unknown argument: {value}")),
        }
    }
    Err("missing required -c <config.json>".to_string())
}

fn print_usage(program: &str) {
    println!("Usage: {program} -c config.json");
}

fn load_config(path: &str) -> Result<OssConfig, String> {
    let content = fs::read(path).map_err(|err| format!("failed to read config {path}: {err}"))?;
    serde_json::from_slice::<OssConfig>(&content)
        .map(OssConfig::normalize)
        .map_err(|err| format!("failed to parse config {path}: {err}"))
}

fn build_rule_engine(config: &OssConfig) -> Result<RuleEngine, String> {
    match config.payload.format.as_str() {
        "json" | "JSON" => Ok(RuleEngine::new(PayloadDecoder::from_format(PayloadFormat::Json))),
        "protobuf" | "Protobuf" | "PROTOBUF" => {
            let Some(path) = config.payload.protobuf_descriptor_set_path.as_ref() else {
                return Err("payload.protobuf_descriptor_set_path is required for protobuf".to_string());
            };
            let decoder = dynamic_protobuf_registry_from_descriptor_set_file(path)
                .map_err(|err| err.to_string())?;
            Ok(RuleEngine::new(decoder))
        }
        other => Err(format!("unsupported payload.format: {other}")),
    }
}

fn build_handler(
    mut engine: RuleEngine,
    dispatcher: Arc<Dispatcher>,
    queue_capacity: usize,
    metrics: Arc<OssMetrics>,
) -> (MessageHandler, JoinHandle<()>) {
    let (eval_tx, eval_rx) = flume::bounded::<IncomingDelivery>(queue_capacity);
    let worker_metrics = Arc::clone(&metrics);
    let eval_worker = thread::Builder::new()
        .name("metre-oss-eval".to_string())
        .spawn(move || {
            while let Ok(delivery) = eval_rx.recv() {
                let pipeline_start = Instant::now();
                worker_metrics.record_eval_queue_dequeued();
                let message = delivery.message.clone();
                let core_eval_start = Instant::now();
                let results = engine.evaluate(&message);
                worker_metrics.record_core_eval(core_eval_start.elapsed().as_nanos() as u64);
                delivery.ack();
                for result in results {
                    dispatcher.dispatch(result.rule_index, &result.message);
                }
                worker_metrics.record_worker_pipeline(pipeline_start.elapsed().as_nanos() as u64);
            }
        })
        .expect("failed to spawn metre-oss eval worker");

    let handler = Arc::new(move |delivery: IncomingDelivery| {
        metrics.record_ingress();
        match eval_tx.try_send(delivery) {
            Ok(()) => metrics.record_eval_queue_enqueued(),
            Err(flume::TrySendError::Full(_)) => {
                metrics.record_eval_queue_drop();
                log::warn!("dropping incoming message because eval queue is full");
            }
            Err(flume::TrySendError::Disconnected(_)) => {
                metrics.record_eval_queue_drop();
                log::warn!("dropping incoming message because eval queue is closed");
            }
        }
    });
    (handler, eval_worker)
}

fn generate_default_node_id() -> String {
    let pid = std::process::id();
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    format!("metre_oss_{}_{}", pid, millis)
}
