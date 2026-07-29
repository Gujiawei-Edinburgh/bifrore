use flume::Sender;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerComponent {
    Mqtt,
    Processor,
    Egress,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerFailure {
    pub component: WorkerComponent,
    pub recoverable: bool,
    pub message: String,
}

impl WorkerFailure {
    pub fn recoverable(component: WorkerComponent, message: impl Into<String>) -> Self {
        Self {
            component,
            recoverable: true,
            message: message.into(),
        }
    }

    pub fn fatal(component: WorkerComponent, message: impl Into<String>) -> Self {
        Self {
            component,
            recoverable: false,
            message: message.into(),
        }
    }
}

pub struct WorkerSupervisor {
    failure: Arc<Mutex<Option<WorkerFailure>>>,
    stop_tx: Sender<()>,
    thread: Option<JoinHandle<()>>,
}

impl WorkerSupervisor {
    pub fn start() -> (Sender<WorkerFailure>, Self) {
        let (failure_tx, failure_rx) = flume::unbounded();
        let (stop_tx, stop_rx) = flume::bounded(1);
        let failure = Arc::new(Mutex::new(None));
        let failure_state = Arc::clone(&failure);
        let thread = thread::Builder::new()
            .name("tenon-worker-supervisor".to_string())
            .spawn(move || {
                loop {
                    match flume::Selector::new()
                        .recv(&failure_rx, SupervisorEvent::Failure)
                        .recv(&stop_rx, SupervisorEvent::Stop)
                        .wait()
                    {
                        SupervisorEvent::Failure(Ok(event)) => {
                            if event.recoverable {
                                log::warn!(
                                    "recoverable worker failure component={:?}: {}",
                                    event.component,
                                    event.message
                                );
                            } else if let Ok(mut current) = failure_state.lock() {
                                if current.is_none() {
                                    log::error!(
                                        "worker failure component={:?}: {}",
                                        event.component,
                                        event.message
                                    );
                                    *current = Some(event);
                                }
                            }
                        }
                        SupervisorEvent::Failure(Err(_))
                        | SupervisorEvent::Stop(Err(_))
                        | SupervisorEvent::Stop(Ok(())) => break,
                    }
                }
            })
            .expect("failed to start worker supervisor");

        (
            failure_tx,
            Self {
                failure,
                stop_tx,
                thread: Some(thread),
            },
        )
    }

    pub fn failure(&self) -> Option<WorkerFailure> {
        self.failure.lock().ok().and_then(|failure| failure.clone())
    }

    pub fn stop(mut self) {
        let _ = self.stop_tx.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for WorkerSupervisor {
    fn drop(&mut self) {
        let _ = self.stop_tx.send(());
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

enum SupervisorEvent {
    Failure(Result<WorkerFailure, flume::RecvError>),
    Stop(Result<(), flume::RecvError>),
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn records_terminal_failure() {
        let (failure_tx, supervisor) = WorkerSupervisor::start();
        failure_tx
            .send(WorkerFailure::fatal(WorkerComponent::Processor, "panic"))
            .expect("send failure");

        for _ in 0..20 {
            if supervisor.failure().is_some() {
                break;
            }
            std::thread::sleep(Duration::from_millis(1));
        }

        assert_eq!(
            supervisor.failure(),
            Some(WorkerFailure::fatal(WorkerComponent::Processor, "panic"))
        );
        supervisor.stop();
    }

    #[test]
    fn ignores_recoverable_failure_for_terminal_state() {
        let (failure_tx, supervisor) = WorkerSupervisor::start();
        failure_tx
            .send(WorkerFailure::recoverable(
                WorkerComponent::Mqtt,
                "reconnect",
            ))
            .expect("send failure");
        std::thread::sleep(Duration::from_millis(5));

        assert!(supervisor.failure().is_none());
        supervisor.stop();
    }
}
