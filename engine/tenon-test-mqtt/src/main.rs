use rumqttc::{AsyncClient, Event, MqttOptions, Packet, QoS};
use std::env;
use std::time::Duration;
use tokio::time::timeout;

#[tokio::main]
async fn main() {
    if let Err(err) = run().await {
        eprintln!("tenon-test-mqtt failed: {err}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let args: Vec<String> = env::args().collect();
    let config = Config::parse(&args)?;
    publish_once(config).await
}

async fn publish_once(config: Config) -> Result<(), String> {
    let mut options = MqttOptions::new(config.client_id, config.host, config.port);
    options.set_keep_alive(Duration::from_secs(5));
    options.set_clean_session(true);
    if let Some((username, password)) = config.username.zip(config.password) {
        options.set_credentials(username, password);
    }

    let (client, mut event_loop) = AsyncClient::new(options, 10);
    let pump = tokio::spawn(async move {
        loop {
            match event_loop.poll().await {
                Ok(Event::Incoming(Packet::PubAck(_))) => return Ok(()),
                Ok(_) => {}
                Err(err) => return Err(format!("MQTT event loop error: {err}")),
            }
        }
    });

    client
        .publish(config.topic, QoS::AtLeastOnce, false, config.payload)
        .await
        .map_err(|err| format!("failed to publish MQTT message: {err}"))?;

    match timeout(config.timeout, pump).await {
        Ok(Ok(Ok(()))) => Ok(()),
        Ok(Ok(Err(err))) => Err(err),
        Ok(Err(err)) => Err(format!("MQTT event task failed: {err}")),
        Err(_) => Err("timed out waiting for MQTT publish ack".to_string()),
    }
}

#[derive(Debug)]
struct Config {
    host: String,
    port: u16,
    client_id: String,
    topic: String,
    payload: Vec<u8>,
    username: Option<String>,
    password: Option<String>,
    timeout: Duration,
}

impl Config {
    fn parse(args: &[String]) -> Result<Self, String> {
        let mut config = Self {
            host: "127.0.0.1".to_string(),
            port: 1883,
            client_id: format!("tenon-test-mqtt-{}", std::process::id()),
            topic: "data".to_string(),
            payload: b"{}".to_vec(),
            username: None,
            password: None,
            timeout: Duration::from_secs(10),
        };

        let mut index = 1;
        while index < args.len() {
            match args[index].as_str() {
                "--host" => {
                    config.host = next_arg(args, &mut index, "--host")?;
                }
                "--port" => {
                    config.port = next_arg(args, &mut index, "--port")?
                        .parse()
                        .map_err(|_| "--port must be a u16".to_string())?;
                }
                "--client-id" => {
                    config.client_id = next_arg(args, &mut index, "--client-id")?;
                }
                "--topic" => {
                    config.topic = next_arg(args, &mut index, "--topic")?;
                }
                "--payload" => {
                    config.payload = next_arg(args, &mut index, "--payload")?.into_bytes();
                }
                "--username" => {
                    config.username = Some(next_arg(args, &mut index, "--username")?);
                }
                "--password" => {
                    config.password = Some(next_arg(args, &mut index, "--password")?);
                }
                "--timeout-secs" => {
                    let timeout_secs: u64 = next_arg(args, &mut index, "--timeout-secs")?
                        .parse()
                        .map_err(|_| "--timeout-secs must be a positive integer".to_string())?;
                    config.timeout = Duration::from_secs(timeout_secs.max(1));
                }
                "-h" | "--help" => {
                    print_usage(&args[0]);
                    std::process::exit(0);
                }
                other => return Err(format!("unknown argument: {other}")),
            }
            index += 1;
        }

        Ok(config)
    }
}

fn next_arg(args: &[String], index: &mut usize, name: &str) -> Result<String, String> {
    *index += 1;
    args.get(*index)
        .cloned()
        .ok_or_else(|| format!("missing value for {name}"))
}

fn print_usage(program: &str) {
    println!("Usage: {program} [--host HOST] [--port PORT] [--client-id ID] --topic TOPIC --payload PAYLOAD");
}
