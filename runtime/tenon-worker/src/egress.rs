use crate::{WorkerError, WorkerMetrics, WorkerResult};
use flume::{Receiver, Sender, TrySendError};
use mio::net::{UnixListener, UnixStream};
use mio::{Events, Interest, Poll, Token};
use prost::Message as ProstMessage;
#[cfg(test)]
use serde_json::Value;
#[cfg(test)]
use std::io::Read;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};
use tenon_extension::{EmitRecord, InvocationOutcome};
#[cfg(test)]
use tenon_extension::ExtensionValue;
use tenon_message::egress::v1::{EgressBatchFrame, EgressRecord};
use tenon_message::plan::{DeliveryMode, EgressPlan};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EgressConfig {
    pub queue_capacity: usize,
    pub socket_path: PathBuf,
    pub send_timeout: Duration,
    pub batch_budget: BatchBudget,
}

impl Default for EgressConfig {
    fn default() -> Self {
        Self {
            queue_capacity: 4096,
            socket_path: default_socket_path(),
            send_timeout: Duration::from_millis(5),
            batch_budget: BatchBudget::default(),
        }
    }
}

fn default_socket_path() -> PathBuf {
    std::env::temp_dir().join(format!("tenon-worker-egress-{}.sock", std::process::id()))
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BatchBudget {
    pub max_records: usize,
    pub max_record_bytes: usize,
    pub max_batch_bytes: usize,
    pub max_batch_wait: Duration,
}

impl Default for BatchBudget {
    fn default() -> Self {
        Self {
            max_records: 64,
            max_record_bytes: 256 * 1024,
            max_batch_bytes: 256 * 1024,
            max_batch_wait: Duration::ZERO,
        }
    }
}

#[derive(Clone)]
pub struct Egress {
    sender: Sender<EmitRecord>,
}

pub struct EgressRuntime {
    egress: Egress,
    drain_thread: Option<JoinHandle<()>>,
    socket_path: PathBuf,
}

impl EgressRuntime {
    pub fn start(
        plan: Option<EgressPlan>,
        config: EgressConfig,
        metrics: Arc<WorkerMetrics>,
    ) -> WorkerResult<Self> {
        let plan = plan.ok_or_else(|| WorkerError::pipeline("egress plan is missing"))?;
        let delivery = DeliveryMode::try_from(plan.delivery)
            .map_err(|_| WorkerError::pipeline("invalid egress delivery mode"))?;
        match delivery {
            DeliveryMode::Single | DeliveryMode::Broadcast => {}
            DeliveryMode::Unspecified => {
                return Err(WorkerError::pipeline("egress delivery mode is unspecified"));
            }
        }
        let listener = bind_egress_listener(&config.socket_path)?;

        let (sender, receiver) = flume::bounded(config.queue_capacity.max(1));
        let socket_path = config.socket_path.clone();
        let drain_thread = Some(start_egress_drain_loop(
            receiver,
            listener,
            metrics,
            config.batch_budget,
            config.send_timeout,
        ));
        Ok(Self {
            egress: Egress { sender },
            drain_thread,
            socket_path,
        })
    }

    pub fn egress(&self) -> Egress {
        self.egress.clone()
    }

    pub fn stop(mut self) -> WorkerResult<()> {
        drop(self.egress);
        if let Some(drain_thread) = self.drain_thread.take() {
            drain_thread
                .join()
                .map_err(|_| WorkerError::pipeline("egress drain thread panicked"))?;
        }
        let _ = std::fs::remove_file(&self.socket_path);
        Ok(())
    }
}

impl Egress {
    pub fn disabled_for_test() -> Self {
        let (sender, _receiver) = flume::bounded(1);
        Self {
            sender,
        }
    }

    pub fn dispatch(&self, outcome: InvocationOutcome, metrics: &WorkerMetrics) {
        for record in outcome.emits {
            match self.sender.try_send(record) {
                Ok(()) => metrics.record_egress_enqueued_record(),
                Err(TrySendError::Full(_)) => {
                    metrics.record_egress_dropped_record();
                    log::warn!("dropping emitted record because egress queue is full");
                }
                Err(TrySendError::Disconnected(_)) => {
                    metrics.record_egress_dropped_record();
                    log::warn!("dropping emitted record because egress stage is stopped");
                }
            }
        }
    }
}

#[derive(Debug)]
struct BatchBuilder {
    budget: BatchBudget,
    records: Vec<Vec<u8>>,
    payload_bytes: usize,
}

impl BatchBuilder {
    fn new(budget: BatchBudget) -> Self {
        Self {
            budget,
            records: Vec::new(),
            payload_bytes: 0,
        }
    }

    fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    fn is_full(&self) -> bool {
        self.records.len() >= self.budget.max_records
            || self.payload_bytes >= self.budget.max_batch_bytes
    }

    fn try_push(&mut self, record: EmitRecord) -> Result<(), BatchPushError> {
        let bytes = serde_json::to_vec(&record.payload)
            .map_err(|error| BatchPushError::Encode(error.to_string()))?;
        let len = bytes.len();
        if len > self.budget.max_record_bytes {
            return Err(BatchPushError::Oversized { len });
        }
        if self.records.len() >= self.budget.max_records
            || self.payload_bytes + len > self.budget.max_batch_bytes
        {
            if self.records.is_empty() {
                return Err(BatchPushError::Oversized { len });
            }
            return Err(BatchPushError::WouldExceed(record));
        }
        self.payload_bytes += len;
        self.records.push(bytes);
        Ok(())
    }

    fn build_frame(&self) -> Vec<u8> {
        encode_batch_frame(&self.records)
    }
}

#[derive(Debug)]
enum BatchPushError {
    Oversized { len: usize },
    WouldExceed(EmitRecord),
    Encode(String),
}

fn start_egress_drain_loop(
    receiver: Receiver<EmitRecord>,
    listener: UnixListener,
    metrics: Arc<WorkerMetrics>,
    budget: BatchBudget,
    send_timeout: Duration,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut transport = match EgressTransport::new(listener) {
            Ok(transport) => transport,
            Err(error) => {
                log::error!("egress transport failed to start: {error}");
                drain_and_drop(receiver, &metrics);
                return;
            }
        };
        let mut pending = None;
        loop {
            transport.accept_pending_consumers();
            let Some(first) = pending.take().or_else(|| receiver.recv().ok()) else {
                break;
            };
            let mut builder = BatchBuilder::new(budget.clone());
            match builder.try_push(first) {
                Ok(()) => {}
                Err(error) => {
                    handle_batch_push_error(error, &metrics, &mut pending);
                    continue;
                }
            }

            fill_batch(&receiver, &metrics, &budget, &mut builder, &mut pending);

            if !builder.is_empty() {
                let frame = builder.build_frame();
                let record_count = builder.records.len();
                match transport.send_frame(&frame, send_timeout) {
                    SendFrameResult::Sent => {
                        metrics.record_egress_delivered_records(record_count);
                    }
                    SendFrameResult::Dropped => {
                        metrics.record_egress_dropped_records(record_count);
                    }
                }
            }
        }
    })
}

fn bind_egress_listener(socket_path: &Path) -> WorkerResult<UnixListener> {
    if socket_path.exists() {
        std::fs::remove_file(socket_path).map_err(|error| {
            WorkerError::pipeline(format!(
                "failed to remove stale egress socket {}: {error}",
                socket_path.display()
            ))
        })?;
    }
    if let Some(parent) = socket_path.parent() {
        std::fs::create_dir_all(parent).map_err(|error| {
            WorkerError::pipeline(format!(
                "failed to create egress socket directory {}: {error}",
                parent.display()
            ))
        })?;
    }
    UnixListener::bind(socket_path).map_err(|error| {
        WorkerError::pipeline(format!(
            "failed to bind egress socket {}: {error}",
            socket_path.display()
        ))
    })
}

fn drain_and_drop(receiver: Receiver<EmitRecord>, metrics: &WorkerMetrics) {
    while receiver.recv().is_ok() {
        metrics.record_egress_dropped_record();
    }
}

const LISTENER: Token = Token(0);
const CONSUMER: Token = Token(1);

struct EgressTransport {
    listener: UnixListener,
    poll: Poll,
    events: Events,
    consumer: Option<UnixStream>,
}

impl EgressTransport {
    fn new(mut listener: UnixListener) -> io::Result<Self> {
        let poll = Poll::new()?;
        poll.registry()
            .register(&mut listener, LISTENER, Interest::READABLE)?;
        Ok(Self {
            listener,
            poll,
            events: Events::with_capacity(8),
            consumer: None,
        })
    }

    fn accept_pending_consumers(&mut self) {
        loop {
            match self.listener.accept() {
                Ok((mut stream, _addr)) => {
                    if self.consumer.is_some() {
                        log::warn!("rejecting egress consumer because one is already connected");
                        continue;
                    }
                    if let Err(error) = self.poll.registry().register(
                        &mut stream,
                        CONSUMER,
                        Interest::WRITABLE,
                    ) {
                        log::error!("rejecting egress consumer because registration failed: {error}");
                        continue;
                    }
                    self.consumer = Some(stream);
                    log::info!("egress consumer connected");
                }
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                Err(error) => {
                    log::error!("failed to accept egress consumer: {error}");
                    break;
                }
            }
        }
    }

    fn send_frame(&mut self, frame: &[u8], timeout: Duration) -> SendFrameResult {
        self.accept_pending_consumers();
        let Some(mut stream) = self.consumer.take() else {
            log::warn!("dropping egress batch because no consumer is connected");
            return SendFrameResult::Dropped;
        };

        let result = write_frame(&mut stream, &mut self.poll, &mut self.events, frame, timeout);
        match result {
            WriteFrameResult::Complete => {
                self.consumer = Some(stream);
                SendFrameResult::Sent
            }
            WriteFrameResult::NoProgressTimeout => {
                self.consumer = Some(stream);
                log::warn!("dropping egress batch because consumer is not writable");
                SendFrameResult::Dropped
            }
            WriteFrameResult::PartialFailure | WriteFrameResult::Disconnected => {
                log::warn!("closing egress consumer after incomplete frame write");
                SendFrameResult::Dropped
            }
        }
    }
}

enum SendFrameResult {
    Sent,
    Dropped,
}

enum WriteFrameResult {
    Complete,
    NoProgressTimeout,
    PartialFailure,
    Disconnected,
}

fn write_frame(
    stream: &mut UnixStream,
    poll: &mut Poll,
    events: &mut Events,
    frame: &[u8],
    timeout: Duration,
) -> WriteFrameResult {
    let deadline = Instant::now() + timeout;
    let mut written = 0;
    loop {
        match stream.write(&frame[written..]) {
            Ok(0) => {
                return if written == 0 {
                    WriteFrameResult::Disconnected
                } else {
                    WriteFrameResult::PartialFailure
                };
            }
            Ok(len) => {
                written += len;
                if written == frame.len() {
                    return WriteFrameResult::Complete;
                }
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                let now = Instant::now();
                if now >= deadline {
                    return if written == 0 {
                        WriteFrameResult::NoProgressTimeout
                    } else {
                        WriteFrameResult::PartialFailure
                    };
                }
                let wait = deadline.saturating_duration_since(now);
                match poll.poll(events, Some(wait)) {
                    Ok(()) => {
                        if !events.iter().any(|event| {
                            event.token() == CONSUMER && event.is_writable()
                        }) {
                            let now = Instant::now();
                            if now >= deadline {
                                return if written == 0 {
                                    WriteFrameResult::NoProgressTimeout
                                } else {
                                    WriteFrameResult::PartialFailure
                                };
                            }
                        }
                    }
                    Err(error) if error.kind() == io::ErrorKind::Interrupted => {}
                    Err(_) => {
                        return if written == 0 {
                            WriteFrameResult::Disconnected
                        } else {
                            WriteFrameResult::PartialFailure
                        };
                    }
                }
            }
            Err(_) => {
                return if written == 0 {
                    WriteFrameResult::Disconnected
                } else {
                    WriteFrameResult::PartialFailure
                };
            }
        }
    }
}

fn fill_batch(
    receiver: &Receiver<EmitRecord>,
    metrics: &WorkerMetrics,
    budget: &BatchBudget,
    builder: &mut BatchBuilder,
    pending: &mut Option<EmitRecord>,
) {
    if budget.max_batch_wait.is_zero() {
        fill_batch_without_wait(receiver, metrics, builder, pending);
    } else {
        fill_batch_until_deadline(receiver, metrics, budget, builder, pending);
    }
}

fn fill_batch_without_wait(
    receiver: &Receiver<EmitRecord>,
    metrics: &WorkerMetrics,
    builder: &mut BatchBuilder,
    pending: &mut Option<EmitRecord>,
) {
    while !builder.is_full() {
        let Ok(record) = receiver.try_recv() else {
            break;
        };
        match builder.try_push(record) {
            Ok(()) => {}
            Err(error) => {
                handle_batch_push_error(error, metrics, pending);
                break;
            }
        }
    }
}

fn fill_batch_until_deadline(
    receiver: &Receiver<EmitRecord>,
    metrics: &WorkerMetrics,
    budget: &BatchBudget,
    builder: &mut BatchBuilder,
    pending: &mut Option<EmitRecord>,
) {
    let deadline = Instant::now() + budget.max_batch_wait;
    while !builder.is_full() {
        let now = Instant::now();
        if now >= deadline {
            break;
        }
        let remaining = deadline.saturating_duration_since(now);
        let Ok(record) = receiver.recv_timeout(remaining) else {
            break;
        };
        match builder.try_push(record) {
            Ok(()) => {}
            Err(error) => {
                handle_batch_push_error(error, metrics, pending);
                break;
            }
        }
    }
}

fn handle_batch_push_error(
    error: BatchPushError,
    metrics: &WorkerMetrics,
    pending: &mut Option<EmitRecord>,
) {
    match error {
        BatchPushError::Oversized { len } => {
            metrics.record_egress_dropped_record();
            log::warn!("dropping oversized emitted record bytes={len}");
        }
        BatchPushError::WouldExceed(record) => {
            *pending = Some(record);
        }
        BatchPushError::Encode(error) => {
            metrics.record_egress_dropped_record();
            log::warn!("dropping emitted record because JSON encoding failed: {error}");
        }
    }
}

const FRAME_VERSION: u8 = 1;

fn encode_batch_frame(records: &[Vec<u8>]) -> Vec<u8> {
    let mut payload_blob = Vec::with_capacity(records.iter().map(Vec::len).sum());
    let mut offsets = Vec::with_capacity(records.len());
    let mut offset = 0u32;
    for record in records {
        offsets.push(EgressRecord {
            offset,
            length: record.len() as u32,
        });
        payload_blob.extend_from_slice(record);
        offset += record.len() as u32;
    }
    let body = EgressBatchFrame {
        version: FRAME_VERSION.into(),
        records: offsets,
        payload_blob,
    }
    .encode_to_vec();

    let mut frame = Vec::with_capacity(4 + body.len());
    frame.extend_from_slice(&(body.len() as u32).to_le_bytes());
    frame.extend_from_slice(&body);
    frame
}

#[cfg(test)]
fn read_batch_frame<R: Read>(reader: &mut R) -> io::Result<Vec<ExtensionValue>> {
    let mut len = [0u8; 4];
    reader.read_exact(&mut len)?;
    let frame_len = u32::from_le_bytes(len) as usize;
    let mut body = vec![0u8; frame_len];
    reader.read_exact(&mut body)?;
    decode_batch_frame_body(&body)
}

#[cfg(test)]
fn decode_batch_frame_body(body: &[u8]) -> io::Result<Vec<ExtensionValue>> {
    let frame = EgressBatchFrame::decode(body)
        .map_err(|error| invalid_frame(format!("invalid protobuf frame: {error}")))?;
    if frame.version != u32::from(FRAME_VERSION) {
        return Err(invalid_frame("unsupported frame version"));
    }
    let payload = frame.payload_blob.as_slice();
    let mut values = Vec::with_capacity(frame.records.len());
    for record in frame.records {
        let offset = record.offset as usize;
        let len = record.length as usize;
        let end = offset
            .checked_add(len)
            .ok_or_else(|| invalid_frame("record offset overflow"))?;
        if end > payload.len() {
            return Err(invalid_frame("record out of payload bounds"));
        }
        let value = serde_json::from_slice::<Value>(&payload[offset..end])
            .map_err(|error| invalid_frame(format!("invalid record JSON: {error}")))?;
        values.push(value);
    }
    Ok(values)
}

#[cfg(test)]
fn invalid_frame(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Cursor;
    use std::os::unix::net::UnixStream as StdUnixStream;
    use std::sync::atomic::{AtomicU64, Ordering};

    static SOCKET_SEQ: AtomicU64 = AtomicU64::new(0);

    fn test_socket_path(name: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "tenon-worker-egress-test-{}-{}-{name}.sock",
            std::process::id(),
            SOCKET_SEQ.fetch_add(1, Ordering::Relaxed)
        ))
    }

    #[test]
    fn batch_builder_encodes_user_readable_batch_frame() {
        let mut builder = BatchBuilder::new(BatchBudget::default());
        builder
            .try_push(EmitRecord::new(json!({"index": 1})))
            .expect("first record");
        builder
            .try_push(EmitRecord::new(json!({"index": 2})))
            .expect("second record");

        let frame = builder.build_frame();
        let records = read_batch_frame(&mut Cursor::new(frame)).expect("batch frame");

        assert_eq!(records, vec![json!({"index": 1}), json!({"index": 2})]);
    }

    #[test]
    fn batch_builder_rejects_oversized_record() {
        let mut builder = BatchBuilder::new(BatchBudget {
            max_record_bytes: 8,
            ..BatchBudget::default()
        });

        let error = builder
            .try_push(EmitRecord::new(json!({"too_large": true})))
            .expect_err("oversized record");

        assert!(matches!(error, BatchPushError::Oversized { .. }));
        assert!(builder.is_empty());
    }

    #[test]
    fn batch_builder_keeps_order_when_budget_splits() {
        let mut builder = BatchBuilder::new(BatchBudget {
            max_records: 1,
            ..BatchBudget::default()
        });
        builder
            .try_push(EmitRecord::new(json!({"index": 1})))
            .expect("first record");

        let error = builder
            .try_push(EmitRecord::new(json!({"index": 2})))
            .expect_err("budget split");

        match error {
            BatchPushError::WouldExceed(record) => {
                assert_eq!(record.payload, json!({"index": 2}));
            }
            other => panic!("unexpected error: {other:?}"),
        }
    }

    #[test]
    fn zero_wait_fill_drains_only_available_records() {
        let (sender, receiver) = flume::bounded(8);
        sender
            .send(EmitRecord::new(json!({"index": 1})))
            .expect("first send");
        sender
            .send(EmitRecord::new(json!({"index": 2})))
            .expect("second send");
        let mut builder = BatchBuilder::new(BatchBudget::default());
        builder
            .try_push(EmitRecord::new(json!({"index": 0})))
            .expect("seed record");
        let mut pending = None;
        let metrics = WorkerMetrics::default();

        fill_batch(&receiver, &metrics, &BatchBudget::default(), &mut builder, &mut pending);

        let records = read_batch_frame(&mut Cursor::new(builder.build_frame())).expect("frame");
        assert_eq!(
            records,
            vec![json!({"index": 0}), json!({"index": 1}), json!({"index": 2})]
        );
        assert!(pending.is_none());
    }

    #[test]
    fn user_rejects_partial_batch_frame() {
        let mut builder = BatchBuilder::new(BatchBudget::default());
        builder
            .try_push(EmitRecord::new(json!({"index": 1})))
            .expect("record");
        let mut frame = builder.build_frame();
        frame.truncate(frame.len() - 1);

        let error = read_batch_frame(&mut Cursor::new(frame)).expect_err("partial frame");

        assert_eq!(error.kind(), io::ErrorKind::UnexpectedEof);
    }

    #[test]
    fn uds_egress_delivers_batch_frame_to_consumer() {
        let socket_path = test_socket_path("deliver");
        let metrics = Arc::new(WorkerMetrics::default());
        let runtime = EgressRuntime::start(
            Some(EgressPlan {
                delivery: DeliveryMode::Single as i32,
            }),
            EgressConfig {
                socket_path: socket_path.clone(),
                send_timeout: Duration::from_millis(100),
                ..EgressConfig::default()
            },
            Arc::clone(&metrics),
        )
        .expect("egress runtime");
        let mut consumer = StdUnixStream::connect(&socket_path).expect("connect egress socket");

        runtime.egress().dispatch(
            InvocationOutcome {
                emits: vec![
                    EmitRecord::new(json!({"index": 1})),
                    EmitRecord::new(json!({"index": 2})),
                ],
            },
            &metrics,
        );

        let records = read_batch_frame(&mut consumer).expect("batch frame");

        assert_eq!(records, vec![json!({"index": 1}), json!({"index": 2})]);
        runtime.stop().expect("stop egress");
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.egress_enqueued_records, 2);
        assert_eq!(snapshot.egress_delivered_records, 2);
        assert_eq!(snapshot.egress_dropped_records, 0);
        assert!(!socket_path.exists());
    }

    #[test]
    fn uds_egress_drops_batch_without_consumer() {
        let socket_path = test_socket_path("drop");
        let metrics = Arc::new(WorkerMetrics::default());
        let runtime = EgressRuntime::start(
            Some(EgressPlan {
                delivery: DeliveryMode::Single as i32,
            }),
            EgressConfig {
                socket_path: socket_path.clone(),
                send_timeout: Duration::from_millis(1),
                ..EgressConfig::default()
            },
            Arc::clone(&metrics),
        )
        .expect("egress runtime");

        runtime.egress().dispatch(
            InvocationOutcome {
                emits: vec![EmitRecord::new(json!({"index": 1}))],
            },
            &metrics,
        );

        runtime.stop().expect("stop egress");
        let snapshot = metrics.snapshot();
        assert_eq!(snapshot.egress_enqueued_records, 1);
        assert_eq!(snapshot.egress_delivered_records, 0);
        assert_eq!(snapshot.egress_dropped_records, 1);
        assert!(!socket_path.exists());
    }
}
