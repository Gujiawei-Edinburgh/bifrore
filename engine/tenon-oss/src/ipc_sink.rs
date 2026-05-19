use crate::metrics::OssMetrics;
use crate::paths;
use tenon_core::message::Message;
use flume::{Receiver, Sender};
use std::fs;
use std::io::{ErrorKind, Write};
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

const PROTOCOL_VERSION: u8 = 1;
const FRAME_TYPE_BATCH: u8 = 2;
const IPC_ITEM_HEADER_LEN: usize = 18;

pub struct IpcSink {
    tx: Sender<IpcMessage>,
    connected: Arc<AtomicBool>,
    metrics: Arc<OssMetrics>,
    worker: Option<JoinHandle<()>>,
    socket_path: PathBuf,
}

impl IpcSink {
    pub fn new(
        path: String,
        queue_capacity: usize,
        batch_max_messages: usize,
        batch_max_bytes: usize,
        flush_interval_millis: u64,
        metrics: Arc<OssMetrics>,
    ) -> Result<Self, String> {
        if path.trim().is_empty() {
            return Err("IPC sink path must not be empty".to_string());
        }
        let socket_path = paths::expand_user_path(&path)?;
        if let Some(parent) = socket_path.parent() {
            fs::create_dir_all(parent).map_err(|err| {
                format!(
                    "failed to create IPC socket dir {}: {err}",
                    parent.display()
                )
            })?;
        }
        if socket_path.exists() {
            fs::remove_file(&socket_path).map_err(|err| {
                format!(
                    "failed to remove existing IPC socket {}: {err}",
                    socket_path.display()
                )
            })?;
        }

        let listener = UnixListener::bind(&socket_path).map_err(|err| {
            format!(
                "failed to bind IPC sink at {}: {err}",
                socket_path.display()
            )
        })?;
        listener
            .set_nonblocking(true)
            .map_err(|err| format!("failed to set IPC listener nonblocking: {err}"))?;
        let (tx, rx) = flume::bounded(queue_capacity.max(1));
        let connected = Arc::new(AtomicBool::new(false));
        let worker_connected = Arc::clone(&connected);
        let worker_metrics = Arc::clone(&metrics);
        let worker_socket_path = socket_path.clone();
        let worker = thread::Builder::new()
            .name("tenon-ipc".to_string())
            .spawn(move || {
                run_ipc_worker(
                    listener,
                    rx,
                    worker_connected,
                    worker_metrics,
                    batch_max_messages.max(1),
                    batch_max_bytes.max(IPC_ITEM_HEADER_LEN),
                    Duration::from_millis(flush_interval_millis.max(1)),
                );
                let _ = fs::remove_file(worker_socket_path);
            })
            .map_err(|err| format!("failed to spawn IPC sink worker: {err}"))?;

        log::info!(
            "IPC sink listening path={} queue_capacity={} batch_max_messages={} batch_max_bytes={} flush_interval_millis={}",
            socket_path.display(),
            queue_capacity.max(1),
            batch_max_messages.max(1),
            batch_max_bytes.max(IPC_ITEM_HEADER_LEN),
            flush_interval_millis.max(1)
        );

        Ok(Self {
            tx,
            connected,
            metrics,
            worker: Some(worker),
            socket_path,
        })
    }

    pub fn send(&self, destination: &str, rule_index: usize, message: &Message) {
        if !self.connected.load(Ordering::Relaxed) {
            self.metrics.record_ipc_disconnected_drop();
            return;
        }
        if destination.as_bytes().len() > u8::MAX as usize {
            self.metrics.record_ipc_encode_drop();
            log::warn!(
                "dropping IPC message because destination is too long destination_len={} max_bytes={}",
                destination.as_bytes().len(),
                u8::MAX
            );
            return;
        }
        if message.topic.as_bytes().len() > u8::MAX as usize {
            self.metrics.record_ipc_encode_drop();
            log::warn!(
                "dropping IPC message because topic is too long destination={} topic_len={} max_bytes={}",
                destination,
                message.topic.as_bytes().len(),
                u8::MAX
            );
            return;
        }
        let ipc_message = IpcMessage {
            rule_index: rule_index as u32,
            packet_id: message.packet_id,
            destination: destination.as_bytes().to_vec(),
            topic: message.topic.as_bytes().to_vec(),
            payload: message.payload.clone(),
        };
        match self.tx.try_send(ipc_message) {
            Ok(()) => self.metrics.record_ipc_enqueued(),
            Err(flume::TrySendError::Full(_)) => self.metrics.record_ipc_queue_drop(),
            Err(flume::TrySendError::Disconnected(_)) => self.metrics.record_ipc_disconnected_drop(),
        }
    }
}

impl Drop for IpcSink {
    fn drop(&mut self) {
        self.connected.store(false, Ordering::Relaxed);
        let (replacement_tx, _replacement_rx) = flume::bounded(1);
        let old_tx = std::mem::replace(&mut self.tx, replacement_tx);
        drop(old_tx);
        if let Some(worker) = self.worker.take() {
            let _ = worker.join();
        }
        let _ = fs::remove_file(&self.socket_path);
    }
}

#[derive(Debug)]
struct IpcMessage {
    rule_index: u32,
    packet_id: u16,
    destination: Vec<u8>,
    topic: Vec<u8>,
    payload: Vec<u8>,
}

impl IpcMessage {
    fn encoded_len(&self) -> usize {
        IPC_ITEM_HEADER_LEN + self.destination.len() + self.topic.len() + self.payload.len()
    }
}

fn run_ipc_worker(
    listener: UnixListener,
    rx: Receiver<IpcMessage>,
    connected: Arc<AtomicBool>,
    metrics: Arc<OssMetrics>,
    batch_max_messages: usize,
    batch_max_bytes: usize,
    flush_interval: Duration,
) {
    loop {
        if rx.is_disconnected() {
            break;
        }
        match listener.accept() {
            Ok((mut stream, _addr)) => {
                let _ = stream.set_write_timeout(Some(Duration::from_secs(5)));
                if connected.swap(true, Ordering::Relaxed) {
                    metrics.record_ipc_rejected_connection();
                    log::warn!("rejecting extra IPC consumer");
                    continue;
                }
                metrics.record_ipc_connection();
                log::debug!("IPC consumer connected");
                drain_stale_messages(&rx, &metrics);
                serve_consumer(
                    &mut stream,
                    &rx,
                    &connected,
                    &metrics,
                    batch_max_messages,
                    batch_max_bytes,
                    flush_interval,
                );
                connected.store(false, Ordering::Relaxed);
                metrics.record_ipc_disconnect();
                log::debug!("IPC consumer disconnected");
            }
            Err(err) => {
                if err.kind() == ErrorKind::WouldBlock {
                    thread::sleep(Duration::from_millis(100));
                    continue;
                }
                log::warn!("IPC sink accept failed: {err}");
                if rx.is_disconnected() {
                    break;
                }
            }
        }
    }
}

fn serve_consumer(
    stream: &mut UnixStream,
    rx: &Receiver<IpcMessage>,
    connected: &AtomicBool,
    metrics: &OssMetrics,
    batch_max_messages: usize,
    batch_max_bytes: usize,
    flush_interval: Duration,
) {
    let mut batch = Vec::with_capacity(batch_max_messages);
    let mut batch_bytes = 0usize;
    let mut first_message_at: Option<Instant> = None;

    loop {
        if batch.is_empty() {
            match rx.recv() {
                Ok(message) => {
                    batch_bytes = message.encoded_len();
                    batch.push(message);
                    first_message_at = Some(Instant::now());
                }
                Err(_) => return,
            }
        }

        while batch.len() < batch_max_messages && batch_bytes < batch_max_bytes {
            let remaining = first_message_at
                .map(|started| flush_interval.saturating_sub(started.elapsed()))
                .unwrap_or(flush_interval);
            match rx.recv_timeout(remaining) {
                Ok(message) => {
                    batch_bytes += message.encoded_len();
                    batch.push(message);
                }
                Err(flume::RecvTimeoutError::Timeout) => break,
                Err(flume::RecvTimeoutError::Disconnected) => break,
            }
        }

        if let Err(err) = write_batch(stream, &batch) {
            metrics.record_ipc_write_error();
            log::warn!("IPC sink write failed: {err}");
            connected.store(false, Ordering::Relaxed);
            metrics.record_ipc_dequeued(batch.len() as u64);
            return;
        }
        metrics.record_ipc_batches();
        metrics.record_ipc_messages_written(batch.len() as u64);
        metrics.record_ipc_dequeued(batch.len() as u64);
        batch.clear();
        batch_bytes = 0;
        first_message_at = None;
    }
}

fn drain_stale_messages(rx: &Receiver<IpcMessage>, metrics: &OssMetrics) {
    let mut drained = 0u64;
    while rx.try_recv().is_ok() {
        drained += 1;
    }
    metrics.record_ipc_dequeued(drained);
}

fn write_batch(stream: &mut UnixStream, messages: &[IpcMessage]) -> std::io::Result<()> {
    let mut body = Vec::new();
    body.push(PROTOCOL_VERSION);
    body.push(FRAME_TYPE_BATCH);
    buffer.extend_from_slice(&0u16.to_le_bytes()); // header flags and reserved
    body.extend_from_slice(&(messages.len() as u16).to_le_bytes());
    body.extend_from_slice(&0u16.to_le_bytes());

    for message in messages {
        encode_message_item(&mut body, message);
    }

    let body_len = body.len() as u32;
    stream.write_all(&body_len.to_le_bytes())?;
    stream.write_all(&body)?;
    stream.flush()
}

fn encode_message_item(buffer: &mut Vec<u8>, message: &IpcMessage) {
    let item_len = message.encoded_len() as u32;
    buffer.extend_from_slice(&item_len.to_le_bytes());
    buffer.extend_from_slice(&message.rule_index.to_le_bytes());
    buffer.extend_from_slice(&message.packet_id.to_le_bytes());
    buffer.push(message.destination.len() as u8);
    buffer.push(message.topic.len() as u8);
    buffer.extend_from_slice(&0u16.to_le_bytes()); // item flags and reserved
    buffer.extend_from_slice(&(message.payload.len() as u32).to_le_bytes());
    buffer.extend_from_slice(&message.destination);
    buffer.extend_from_slice(&message.topic);
    buffer.extend_from_slice(&message.payload);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_batch_frame_little_endian() {
        let message = IpcMessage {
            rule_index: 7,
            packet_id: 42,
            destination: b"custom".to_vec(),
            topic: b"data".to_vec(),
            payload: br#"{"temp":30}"#.to_vec(),
        };
        let mut body = Vec::new();
        body.push(PROTOCOL_VERSION);
        body.push(FRAME_TYPE_BATCH);
        body.push(0);
        body.push(0);
        body.extend_from_slice(&1u16.to_le_bytes());
        body.extend_from_slice(&0u16.to_le_bytes());
        encode_message_item(&mut body, &message);

        assert_eq!(body[0], 1);
        assert_eq!(body[1], 2);
        assert_eq!(u16::from_le_bytes([body[4], body[5]]), 1);

        let item = &body[8..];
        assert_eq!(u32::from_le_bytes(item[0..4].try_into().unwrap()), message.encoded_len() as u32);
        assert_eq!(u32::from_le_bytes(item[4..8].try_into().unwrap()), 7);
        assert_eq!(u16::from_le_bytes(item[8..10].try_into().unwrap()), 42);
        assert_eq!(item[10], 6);
        assert_eq!(item[11], 4);
        assert_eq!(item[12], 0);
        assert_eq!(item[13], 0);
        assert_eq!(u32::from_le_bytes(item[14..18].try_into().unwrap()), 11);
        assert_eq!(&item[18..24], b"custom");
        assert_eq!(&item[24..28], b"data");
        assert_eq!(&item[28..], br#"{"temp":30}"#);
    }
}
