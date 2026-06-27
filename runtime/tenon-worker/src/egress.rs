use crate::{WorkerError, WorkerMetrics, WorkerResult};
use flume::{Receiver, Sender, TrySendError};
use std::sync::Arc;
use std::thread::JoinHandle;
use tenon_extension::{EmitRecord, InvocationOutcome};
use tenon_message::plan::{DeliveryMode, EgressPlan};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EgressConfig {
    pub queue_capacity: usize,
}

impl Default for EgressConfig {
    fn default() -> Self {
        Self {
            queue_capacity: 4096,
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
        let drain_thread = Some(start_egress_drain_loop(receiver, metrics));
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

fn start_egress_drain_loop(
    receiver: Receiver<EmitRecord>,
    metrics: Arc<WorkerMetrics>,
) -> JoinHandle<()> {
    std::thread::spawn(move || {
        while let Ok(record) = receiver.recv() {
            drop(record);
            metrics.record_egress_delivered_record();
        }
    })
}
