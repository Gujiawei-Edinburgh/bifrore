mod config;
mod sink;

use config::OssConfig;
use metre_coordinator::EngineCoordinator;
use metre_core::mqtt::{start_mqtt, IncomingDelivery, MessageHandler, MqttConfig};
use metre_core::payload::{
    dynamic_protobuf_registry_from_descriptor_set_file, PayloadDecoder, PayloadFormat,
};
use metre_core::runtime::RuleEngine;
use sink::Dispatcher;
use std::env;
use std::fs;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

fn main() {
    if let Err(err) = run() {
        eprintln!("metre-oss failed: {err}");
        std::process::exit(1);
    }
}

fn run() -> Result<(), String> {
    SimpleLogger::init();
    let config_path = parse_config_path(env::args().collect())?;
    let config = load_config(&config_path)?;
    let coordinator = EngineCoordinator::from_oss_files(
        config.rule_json_path.clone(),
        config.client_ids_path.clone(),
    );
    let rule_bytes = coordinator.load_rule_bytes()?;
    let mut rule_engine = build_rule_engine(&config)?;
    let rule_count = rule_engine
        .load_rules_from_json_bytes(&rule_bytes)
        .map_err(|err| err.to_string())?;
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
    let dispatcher = Arc::new(Dispatcher::new(
        config.sinks.log_enabled,
        config.sinks.kafka_enabled,
        rule_metadata,
    ));
    let engine = Arc::new(Mutex::new(rule_engine));
    let handler = build_handler(Arc::clone(&engine), dispatcher);

    log::info!(
        "starting metre-oss config={} rules={} topics={} clients={}",
        config_path,
        rule_count,
        topic_filters.len(),
        mqtt_config.client_count
    );
    let adapter = start_mqtt(mqtt_config, topic_filters, handler).map_err(|err| format!("{err:?}"))?;
    log::info!("metre-oss started; press Ctrl+C to stop");
    loop {
        thread::sleep(Duration::from_secs(3600));
        let _ = &adapter;
    }
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
    serde_json::from_slice(&content).map_err(|err| format!("failed to parse config {path}: {err}"))
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

fn build_handler(engine: Arc<Mutex<RuleEngine>>, dispatcher: Arc<Dispatcher>) -> MessageHandler {
    Arc::new(move |delivery: IncomingDelivery| {
        let message = delivery.message.clone();
        let results = match engine.lock() {
            Ok(mut engine) => engine.evaluate(&message),
            Err(err) => {
                log::error!("rule engine lock poisoned: {err}");
                delivery.ack();
                return;
            }
        };
        delivery.ack();
        for result in results {
            dispatcher.dispatch(result.rule_index, &result.message);
        }
    })
}

fn generate_default_node_id() -> String {
    let pid = std::process::id();
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    format!("metre_oss_{}_{}", pid, millis)
}

struct SimpleLogger;

impl SimpleLogger {
    fn init() {
        static LOGGER: SimpleLogger = SimpleLogger;
        if log::set_logger(&LOGGER).is_ok() {
            log::set_max_level(log::LevelFilter::Info);
        }
    }
}

impl log::Log for SimpleLogger {
    fn enabled(&self, metadata: &log::Metadata) -> bool {
        metadata.level() <= log::Level::Info
    }

    fn log(&self, record: &log::Record) {
        if self.enabled(record.metadata()) {
            eprintln!("[{}][{}] {}", record.level(), record.target(), record.args());
        }
    }

    fn flush(&self) {}
}
