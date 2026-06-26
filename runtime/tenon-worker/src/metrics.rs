use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct WorkerMetrics {
    processed_messages: AtomicU64,
    processor_errors: AtomicU64,
    emitted_records: AtomicU64,
}

impl WorkerMetrics {
    pub fn record_processed_message(&self) {
        self.processed_messages.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_processor_error(&self) {
        self.processor_errors.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_emitted_records(&self, count: usize) {
        self.emitted_records
            .fetch_add(count as u64, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> WorkerMetricsSnapshot {
        WorkerMetricsSnapshot {
            processed_messages: self.processed_messages.load(Ordering::Relaxed),
            processor_errors: self.processor_errors.load(Ordering::Relaxed),
            emitted_records: self.emitted_records.load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorkerMetricsSnapshot {
    pub processed_messages: u64,
    pub processor_errors: u64,
    pub emitted_records: u64,
}
