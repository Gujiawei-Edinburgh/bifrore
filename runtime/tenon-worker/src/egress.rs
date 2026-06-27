use crate::{WorkerError, WorkerMetrics, WorkerResult};
use flume::{Receiver, Sender, TrySendError};
use prost::Message as ProstMessage;
#[cfg(test)]
use serde_json::Value;
#[cfg(test)]
use std::io::{self, Read};
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
    pub batch_budget: BatchBudget,
}

impl Default for EgressConfig {
    fn default() -> Self {
        Self {
            queue_capacity: 4096,
            batch_budget: BatchBudget::default(),
        }
    }
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

        let (sender, receiver) = flume::bounded(config.queue_capacity.max(1));
        let drain_thread = Some(start_egress_drain_loop(
            receiver,
            metrics,
            config.batch_budget,
        ));
        Ok(Self {
            egress: Egress { sender },
            drain_thread,
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
    metrics: Arc<WorkerMetrics>,
    budget: BatchBudget,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        let mut pending = None;
        loop {
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
                drop(frame);
                metrics.record_egress_delivered_records(builder.records.len());
            }
        }
    })
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
}
