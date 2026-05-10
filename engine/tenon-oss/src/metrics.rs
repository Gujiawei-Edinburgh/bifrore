use crate::config::MetricsConfig;
use tenon_core::metrics::{EvalMetrics, EvalMetricsSnapshot, LatencyMetricsSnapshot};
use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use std::io::{Read, Write};
use std::net::{Ipv4Addr, TcpListener, TcpStream};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::thread;
use std::thread::JoinHandle;

const METRICS_PORT: u16 = 9100;

#[derive(Default)]
pub struct OssMetrics {
    eval_queue_depth: AtomicU64,
}

impl OssMetrics {
    pub fn record_ingress(&self) {
        ::metrics::counter!("tenon_oss_ingress_messages_total").increment(1);
    }

    pub fn record_eval_queue_enqueued(&self) {
        let depth = self.eval_queue_depth.fetch_add(1, Ordering::Relaxed) + 1;
        ::metrics::gauge!("tenon_oss_eval_queue_depth").set(depth as f64);
    }

    pub fn record_eval_queue_dequeued(&self) {
        let depth = self.eval_queue_depth.fetch_sub(1, Ordering::Relaxed) - 1;
        ::metrics::gauge!("tenon_oss_eval_queue_depth").set(depth as f64);
    }

    pub fn record_eval_queue_drop(&self) {
        ::metrics::counter!("tenon_oss_eval_queue_drops_total").increment(1);
    }

    pub fn record_core_eval(&self, duration_nanos: u64) {
        record_latency_histogram("tenon_oss_core_eval_latency_seconds", duration_nanos);
    }

    pub fn record_worker_pipeline(&self, duration_nanos: u64) {
        record_latency_histogram("tenon_oss_worker_pipeline_latency_seconds", duration_nanos);
    }

    pub fn record_kafka_enqueue(&self) {
        ::metrics::counter!("tenon_oss_kafka_enqueue_total").increment(1);
    }

    pub fn record_kafka_queue_full(&self) {
        ::metrics::counter!("tenon_oss_kafka_queue_full_total").increment(1);
    }

    pub fn record_kafka_enqueue_error(&self) {
        ::metrics::counter!("tenon_oss_kafka_enqueue_errors_total").increment(1);
    }

    pub fn record_noop_sink_message(&self) {
        ::metrics::counter!("tenon_oss_noop_sink_messages_total").increment(1);
    }

    pub fn record_sink_unsupported_destination(&self) {
        ::metrics::counter!("tenon_oss_sink_unsupported_destinations_total").increment(1);
    }
}

pub fn start_metrics_server(
    _config: MetricsConfig,
    eval_metrics: Arc<EvalMetrics>,
) -> Result<JoinHandle<()>, String> {
    let handle = PrometheusBuilder::new()
        .install_recorder()
        .map_err(|err| format!("failed to install prometheus metrics recorder: {err}"))?;
    initialize_oss_metrics();
    let listener = TcpListener::bind((Ipv4Addr::UNSPECIFIED, METRICS_PORT))
        .map_err(|err| format!("failed to bind metrics endpoint 0.0.0.0:9100: {err}"))?;

    let server = thread::Builder::new()
        .name("tenon-oss-metrics".to_string())
        .spawn(move || {
            log::info!("metrics endpoint listening on http://0.0.0.0:9100/metrics");
            for stream in listener.incoming() {
                match stream {
                    Ok(stream) => handle_metrics_connection(
                        stream,
                        &handle,
                        &eval_metrics,
                    ),
                    Err(err) => log::warn!("metrics endpoint accept failed: {err}"),
                }
            }
        })
        .map_err(|err| format!("failed to spawn metrics endpoint: {err}"))?;
    Ok(server)
}

fn initialize_oss_metrics() {
    publish_counter_absolute("tenon_oss_ingress_messages_total", 0);
    publish_counter_absolute("tenon_oss_eval_queue_drops_total", 0);
    publish_counter_absolute("tenon_oss_kafka_enqueue_total", 0);
    publish_counter_absolute("tenon_oss_kafka_queue_full_total", 0);
    publish_counter_absolute("tenon_oss_kafka_enqueue_errors_total", 0);
    publish_counter_absolute("tenon_oss_noop_sink_messages_total", 0);
    publish_counter_absolute("tenon_oss_sink_unsupported_destinations_total", 0);
    ::metrics::gauge!("tenon_oss_eval_queue_depth").set(0.0);
}

fn handle_metrics_connection(
    mut stream: TcpStream,
    handle: &PrometheusHandle,
    eval_metrics: &EvalMetrics,
) {
    let mut buffer = [0_u8; 1024];
    let Ok(read_len) = stream.read(&mut buffer) else {
        return;
    };
    let request = String::from_utf8_lossy(&buffer[..read_len]);
    if request.starts_with("GET /metrics ") || request.starts_with("GET /metrics?") {
        publish_eval_metrics(eval_metrics.snapshot());
        write_response(&mut stream, "200 OK", &handle.render());
    } else if request.starts_with("GET /health ") || request.starts_with("GET /health?") {
        write_response(&mut stream, "200 OK", "OK\n");
    } else {
        write_response(&mut stream, "404 Not Found", "not found\n");
    }
}

fn write_response(stream: &mut TcpStream, status: &str, body: &str) {
    let response = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; version=0.0.4\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
        body.len(),
        body
    );
    let _ = stream.write_all(response.as_bytes());
}

fn publish_eval_metrics(current: EvalMetricsSnapshot) {
    publish_counter_absolute("tenon_evals_total", current.eval_count);
    publish_counter_absolute("tenon_eval_errors_total", current.eval_error_count);
    publish_counter_absolute(
        "tenon_eval_type_errors_total",
        current.eval_type_error_count,
    );
    publish_counter_absolute(
        "tenon_payload_schema_errors_total",
        current.payload_schema_error_count,
    );
    publish_counter_absolute(
        "tenon_payload_decode_errors_total",
        current.payload_decode_error_count,
    );
    publish_counter_absolute(
        "tenon_payload_build_errors_total",
        current.payload_build_error_count,
    );
    publish_latency_snapshot("tenon_topic_match_latency", current.topic_match);
    publish_latency_snapshot("tenon_payload_decode_latency", current.payload_decode);
    publish_latency_snapshot("tenon_msg_ir_build_latency", current.msg_ir_build);
    publish_latency_snapshot("tenon_predicate_latency", current.predicate);
    publish_latency_snapshot("tenon_projection_latency", current.projection);
}

fn publish_latency_snapshot(name: &'static str, current: LatencyMetricsSnapshot) {
    publish_counter_absolute(&format!("{name}_samples_total"), current.count);
    publish_counter_absolute(&format!("{name}_nanos_total"), current.total_nanos);
    ::metrics::gauge!(format!("{name}_max_nanos")).set(current.max_nanos as f64);
}

fn publish_counter_absolute(name: &str, value: u64) {
    ::metrics::counter!(name.to_string()).absolute(value);
}

fn record_latency_histogram(name: &'static str, duration_nanos: u64) {
    ::metrics::histogram!(name).record(duration_nanos as f64 / 1_000_000_000.0);
}
