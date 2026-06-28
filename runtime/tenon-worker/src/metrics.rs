use std::sync::atomic::{AtomicU64, Ordering};

#[derive(Debug, Default)]
pub struct WorkerMetrics {
    processed_messages: AtomicU64,
    processor_errors: AtomicU64,
    emitted_records: AtomicU64,
    egress_enqueued_records: AtomicU64,
    egress_delivered_records: AtomicU64,
    egress_dropped_records: AtomicU64,
    egress_dropped_queue_full_records: AtomicU64,
    egress_dropped_stopped_records: AtomicU64,
    egress_dropped_no_consumer_records: AtomicU64,
    egress_dropped_slow_consumer_records: AtomicU64,
    egress_dropped_incomplete_frame_records: AtomicU64,
    egress_dropped_oversized_records: AtomicU64,
    egress_dropped_encode_error_records: AtomicU64,
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

    pub fn record_egress_enqueued_record(&self) {
        self.egress_enqueued_records
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_egress_delivered_record(&self) {
        self.egress_delivered_records
            .fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_egress_delivered_records(&self, count: usize) {
        self.egress_delivered_records
            .fetch_add(count as u64, Ordering::Relaxed);
    }

    pub fn record_egress_dropped_record(&self, reason: EgressDropReason) {
        self.record_egress_dropped_records(reason, 1);
    }

    pub fn record_egress_dropped_records(&self, reason: EgressDropReason, count: usize) {
        self.egress_dropped_records
            .fetch_add(count as u64, Ordering::Relaxed);
        reason.counter(self).fetch_add(count as u64, Ordering::Relaxed);
    }

    pub fn snapshot(&self) -> WorkerMetricsSnapshot {
        WorkerMetricsSnapshot {
            processed_messages: self.processed_messages.load(Ordering::Relaxed),
            processor_errors: self.processor_errors.load(Ordering::Relaxed),
            emitted_records: self.emitted_records.load(Ordering::Relaxed),
            egress_enqueued_records: self.egress_enqueued_records.load(Ordering::Relaxed),
            egress_delivered_records: self.egress_delivered_records.load(Ordering::Relaxed),
            egress_dropped_records: self.egress_dropped_records.load(Ordering::Relaxed),
            egress_dropped_queue_full_records: self
                .egress_dropped_queue_full_records
                .load(Ordering::Relaxed),
            egress_dropped_stopped_records: self
                .egress_dropped_stopped_records
                .load(Ordering::Relaxed),
            egress_dropped_no_consumer_records: self
                .egress_dropped_no_consumer_records
                .load(Ordering::Relaxed),
            egress_dropped_slow_consumer_records: self
                .egress_dropped_slow_consumer_records
                .load(Ordering::Relaxed),
            egress_dropped_incomplete_frame_records: self
                .egress_dropped_incomplete_frame_records
                .load(Ordering::Relaxed),
            egress_dropped_oversized_records: self
                .egress_dropped_oversized_records
                .load(Ordering::Relaxed),
            egress_dropped_encode_error_records: self
                .egress_dropped_encode_error_records
                .load(Ordering::Relaxed),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EgressDropReason {
    QueueFull,
    Stopped,
    NoConsumer,
    SlowConsumer,
    IncompleteFrame,
    Oversized,
    EncodeError,
}

impl EgressDropReason {
    fn counter(self, metrics: &WorkerMetrics) -> &AtomicU64 {
        match self {
            Self::QueueFull => &metrics.egress_dropped_queue_full_records,
            Self::Stopped => &metrics.egress_dropped_stopped_records,
            Self::NoConsumer => &metrics.egress_dropped_no_consumer_records,
            Self::SlowConsumer => &metrics.egress_dropped_slow_consumer_records,
            Self::IncompleteFrame => &metrics.egress_dropped_incomplete_frame_records,
            Self::Oversized => &metrics.egress_dropped_oversized_records,
            Self::EncodeError => &metrics.egress_dropped_encode_error_records,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct WorkerMetricsSnapshot {
    pub processed_messages: u64,
    pub processor_errors: u64,
    pub emitted_records: u64,
    pub egress_enqueued_records: u64,
    pub egress_delivered_records: u64,
    pub egress_dropped_records: u64,
    pub egress_dropped_queue_full_records: u64,
    pub egress_dropped_stopped_records: u64,
    pub egress_dropped_no_consumer_records: u64,
    pub egress_dropped_slow_consumer_records: u64,
    pub egress_dropped_incomplete_frame_records: u64,
    pub egress_dropped_oversized_records: u64,
    pub egress_dropped_encode_error_records: u64,
}
