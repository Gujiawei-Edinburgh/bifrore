use criterion::{black_box, criterion_group, criterion_main, Criterion};
use prost::Message as ProstMessage;
use serde_json::json;
use std::collections::HashMap;
use std::io::{ErrorKind, Read};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    Arc,
};
use std::thread::JoinHandle;
use std::time::Duration;
use tenon_extension::{
    Context, EmitRecord, InvocationOutcome, Message, MqttMetadata, SourceContext, Topic,
};
use tenon_message::daemon::v1::json_path_segment;
use tenon_message::egress::v1::EgressBatchFrame;
use tenon_message::plan::{
    AccessMode, EgressPlan, JsonAccess, JsonPath, JsonPathSegment, MessageAccessPlan,
    MetadataAccess, ProcessPlan, PropertiesAccess, RawPayloadAccess, ScriptRuntime, SourceAccess,
    TopicAccess,
};
use tenon_worker::{
    EgressConfig, EgressRuntime, LuaProcessor, Processor, WorkerMetrics,
};

fn bench_worker(c: &mut Criterion) {
    c.bench_function("worker_processor_lua_no_access_plan", |b| {
        let mut processor = processor(None);
        let message = message();

        b.iter(|| {
            let outcome = processor
                .process(black_box(&message))
                .expect("processor outcome");
            black_box(outcome);
        });
    });

    c.bench_function("worker_processor_lua_full_access_plan", |b| {
        let mut processor = processor(Some(full_access_plan()));
        let message = message();

        b.iter(|| {
            let outcome = processor
                .process(black_box(&message))
                .expect("processor outcome");
            black_box(outcome);
        });
    });

    c.bench_function("worker_processor_lua_selective_access_plan", |b| {
        let mut processor = processor(Some(selective_access_plan()));
        let message = message();

        b.iter(|| {
            let outcome = processor
                .process(black_box(&message))
                .expect("processor outcome");
            black_box(outcome);
        });
    });

    c.bench_function("worker_processor_lua_selective_raw_payload_unused", |b| {
        let mut processor = raw_payload_unused_processor(Some(raw_payload_unused_access_plan()));
        let message = message();

        b.iter(|| {
            let outcome = processor
                .process(black_box(&message))
                .expect("processor outcome");
            black_box(outcome);
        });
    });

    c.bench_function("worker_processor_to_egress_dispatch_with_uds_drain", |b| {
        let socket_path = socket_path("worker-e2e");
        let metrics = Arc::new(WorkerMetrics::default());
        let runtime = EgressRuntime::start(
            Some(EgressPlan {}),
            EgressConfig {
                socket_path: socket_path.clone(),
                send_timeout: Duration::from_millis(100),
                ..EgressConfig::default()
            },
            Arc::clone(&metrics),
        )
        .expect("egress runtime");
        let drained_records = Arc::new(AtomicU64::new(0));
        let stop_consumer = Arc::new(AtomicBool::new(false));
        let consumer_thread = start_consumer_drain(
            socket_path.clone(),
            Arc::clone(&drained_records),
            Arc::clone(&stop_consumer),
        );
        wait_for_consumer_handoff(&runtime, &metrics, &drained_records);
        let mut processor = processor(Some(selective_access_plan()));
        let message = message();

        b.iter(|| {
            let outcome = processor
                .process(black_box(&message))
                .expect("processor outcome");
            runtime.egress().dispatch(outcome, &metrics);
        });

        stop_consumer.store(true, Ordering::Release);
        runtime.stop().expect("stop egress");
        consumer_thread.join().expect("consumer drain thread");
        let _ = std::fs::remove_file(socket_path);
        black_box(drained_records.load(Ordering::Relaxed));
    });
}

fn processor(access_plan: Option<MessageAccessPlan>) -> LuaProcessor {
    LuaProcessor::new(
        ProcessPlan {
            runtime: ScriptRuntime::Lua as i32,
            source: r#"
                function on_message(ctx, msg)
                  if msg.topic.levels[1] == "sensor"
                    and msg.topic.levels[2] == "room1"
                    and msg.metadata.qos == 1
                    and msg.payload.temp > 30
                  then
                    ctx.emit({
                      topic = msg.topic.raw,
                      temp = msg.payload.temp,
                      hum = msg.payload.hum,
                      pkid = msg.metadata.pkid
                    })
                  end
                end
            "#
            .to_string(),
            access_plan,
        },
        Context::with_empty_memory(SourceContext::new("bench", "r1")),
    )
    .expect("processor")
}

fn raw_payload_unused_processor(access_plan: Option<MessageAccessPlan>) -> LuaProcessor {
    LuaProcessor::new(
        ProcessPlan {
            runtime: ScriptRuntime::Lua as i32,
            source: r#"
                function on_message(ctx, msg)
                  if msg.payload.temp > 30 then
                    ctx.emit({
                      temp = msg.payload.temp,
                      hum = msg.payload.hum
                    })
                  end
                end
            "#
            .to_string(),
            access_plan,
        },
        Context::with_empty_memory(SourceContext::new("bench", "r1")),
    )
    .expect("processor")
}

fn full_access_plan() -> MessageAccessPlan {
    MessageAccessPlan {
        source: Some(SourceAccess {
            mode: AccessMode::Full as i32,
            name: false,
            version: false,
        }),
        topic: Some(TopicAccess {
            mode: AccessMode::Full as i32,
            raw: false,
            levels: false,
            level_indexes: Vec::new(),
        }),
        payload: Some(JsonAccess {
            mode: AccessMode::Full as i32,
            paths: Vec::new(),
        }),
        raw_payload: Some(RawPayloadAccess {
            mode: AccessMode::Full as i32,
            ranges: Vec::new(),
        }),
        metadata: Some(MetadataAccess {
            mode: AccessMode::Full as i32,
            pkid: false,
            qos: false,
            retain: false,
            dup: false,
        }),
        properties: Some(PropertiesAccess {
            mode: AccessMode::Full as i32,
            keys: Vec::new(),
        }),
    }
}

fn selective_access_plan() -> MessageAccessPlan {
    MessageAccessPlan {
        source: None,
        topic: Some(TopicAccess {
            mode: AccessMode::Selective as i32,
            raw: true,
            levels: true,
            level_indexes: Vec::new(),
        }),
        payload: Some(JsonAccess {
            mode: AccessMode::Selective as i32,
            paths: vec![json_path_field("hum"), json_path_field("temp")],
        }),
        raw_payload: Some(RawPayloadAccess {
            mode: AccessMode::None as i32,
            ranges: Vec::new(),
        }),
        metadata: Some(MetadataAccess {
            mode: AccessMode::Selective as i32,
            pkid: true,
            qos: true,
            retain: false,
            dup: false,
        }),
        properties: None,
    }
}

fn raw_payload_unused_access_plan() -> MessageAccessPlan {
    MessageAccessPlan {
        source: None,
        topic: None,
        payload: Some(JsonAccess {
            mode: AccessMode::Selective as i32,
            paths: vec![json_path_field("hum"), json_path_field("temp")],
        }),
        raw_payload: Some(RawPayloadAccess {
            mode: AccessMode::None as i32,
            ranges: Vec::new(),
        }),
        metadata: None,
        properties: None,
    }
}

fn json_path_field(field: &str) -> JsonPath {
    JsonPath {
        segments: vec![JsonPathSegment {
            kind: Some(json_path_segment::Kind::Field(field.to_string())),
        }],
    }
}

fn message() -> Message {
    Message::new(
        SourceContext::new("bench", "r1"),
        Topic::new("sensor/room1"),
        json!({"temp": 31, "hum": 10}),
        br#"{"temp":31,"hum":10}"#.to_vec(),
        MqttMetadata::new(1, 1, false, false),
        HashMap::new(),
    )
}

fn socket_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "tenon-worker-bench-{}-{name}.sock",
        std::process::id()
    ))
}

fn connect_consumer(socket_path: &PathBuf) -> UnixStream {
    for _ in 0..100 {
        match UnixStream::connect(socket_path) {
            Ok(stream) => return stream,
            Err(_) => std::thread::sleep(Duration::from_millis(1)),
        }
    }
    UnixStream::connect(socket_path).expect("connect egress socket")
}

fn start_consumer_drain(
    socket_path: PathBuf,
    drained_records: Arc<AtomicU64>,
    stop_consumer: Arc<AtomicBool>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut consumer = connect_consumer(&socket_path);
        consumer
            .set_read_timeout(Some(Duration::from_millis(100)))
            .expect("set read timeout");
        while !stop_consumer.load(Ordering::Acquire) {
            match read_batch_frame(&mut consumer) {
                Ok(frame) => {
                    drained_records.fetch_add(frame.records.len() as u64, Ordering::Relaxed);
                }
                Err(error)
                    if matches!(error.kind(), ErrorKind::WouldBlock | ErrorKind::TimedOut) => {}
                Err(_) => break,
            }
        }
    })
}

fn wait_for_consumer_handoff(
    runtime: &EgressRuntime,
    metrics: &WorkerMetrics,
    drained_records: &AtomicU64,
) {
    let initial = drained_records.load(Ordering::Acquire);
    for _ in 0..100 {
        runtime.egress().dispatch(
            InvocationOutcome {
                emits: vec![EmitRecord::new(json!({"warmup": true}))],
            },
            metrics,
        );
        if drained_records.load(Ordering::Acquire) > initial {
            return;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    panic!("egress consumer handoff did not complete");
}

fn read_batch_frame(reader: &mut UnixStream) -> std::io::Result<EgressBatchFrame> {
    let mut len = [0u8; 4];
    reader.read_exact(&mut len)?;
    let frame_len = u32::from_le_bytes(len) as usize;
    let mut body = vec![0u8; frame_len];
    reader.read_exact(&mut body)?;
    EgressBatchFrame::decode(body.as_slice()).map_err(|error| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("invalid egress batch frame: {error}"),
        )
    })
}

criterion_group!(benches, bench_worker);
criterion_main!(benches);
