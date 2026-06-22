use crate::DaemonError;
use crate::DaemonResult;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tenon_message::codec::encode_frame;
use tenon_message::daemon::v1::{
    worker_envelope, ReloadWorkerRequest, StartWorkerRequest, StopWorkerRequest, WorkerEnvelope,
};
use tenon_message::plan::{DeploymentPlan, ResourceId};
use wait_timeout::ChildExt;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkerHandle {
    pub id: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorkerStatus {
    Init,
    Starting,
    Running,
    Stopping,
    Stopped,
    Error,
}

pub trait WorkerManager {
    fn start(&mut self, plan: DeploymentPlan) -> DaemonResult<WorkerHandle>;

    fn reload(&mut self, worker: &WorkerHandle, plan: DeploymentPlan) -> DaemonResult<()>;

    fn stop(&mut self, worker: WorkerHandle) -> DaemonResult<()>;

    fn status(&mut self, worker: &WorkerHandle) -> DaemonResult<WorkerStatus>;
}

#[derive(Debug, Default)]
pub struct NoopWorkerManager {
    next_id: AtomicU64,
}

impl WorkerManager for NoopWorkerManager {
    fn start(&mut self, _plan: DeploymentPlan) -> DaemonResult<WorkerHandle> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        Ok(WorkerHandle {
            id: format!("worker-{id}"),
        })
    }

    fn reload(&mut self, _worker: &WorkerHandle, _plan: DeploymentPlan) -> DaemonResult<()> {
        Ok(())
    }

    fn stop(&mut self, _worker: WorkerHandle) -> DaemonResult<()> {
        Ok(())
    }

    fn status(&mut self, _worker: &WorkerHandle) -> DaemonResult<WorkerStatus> {
        Ok(WorkerStatus::Running)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UdsWorkerManagerConfig {
    pub worker_binary: PathBuf,
    pub socket_dir: PathBuf,
    pub worker_args: Vec<String>,
    pub connect_timeout: Duration,
    pub stop_timeout: Duration,
}

impl UdsWorkerManagerConfig {
    pub fn new(worker_binary: impl Into<PathBuf>, socket_dir: impl Into<PathBuf>) -> Self {
        Self {
            worker_binary: worker_binary.into(),
            socket_dir: socket_dir.into(),
            worker_args: Vec::new(),
            connect_timeout: Duration::from_secs(10),
            stop_timeout: Duration::from_secs(10),
        }
    }
}

#[derive(Debug)]
pub struct UdsWorkerManager {
    config: UdsWorkerManagerConfig,
    next_id: AtomicU64,
    workers: HashMap<String, UdsWorkerProcess>,
}

impl UdsWorkerManager {
    pub fn new(config: UdsWorkerManagerConfig) -> Self {
        Self {
            config,
            next_id: AtomicU64::new(0),
            workers: HashMap::new(),
        }
    }
}

impl WorkerManager for UdsWorkerManager {
    fn start(&mut self, plan: DeploymentPlan) -> DaemonResult<WorkerHandle> {
        let id = self.next_worker_id(&plan)?;
        let socket_path = self.socket_path(&id);
        prepare_socket(&self.config.socket_dir, &socket_path)?;

        let listener = UnixListener::bind(&socket_path).map_err(|error| {
            DaemonError::worker(format!(
                "failed to bind worker socket {}: {error}",
                socket_path.display()
            ))
        })?;

        let child = self.spawn_worker(&id, &socket_path)?;
        let mut stream = match accept_worker(&listener, self.config.connect_timeout) {
            Ok(stream) => stream,
            Err(error) => {
                cleanup_failed_worker(child, &socket_path);
                return Err(error);
            }
        };
        send_worker_envelope(
            &mut stream,
            WorkerEnvelope {
                payload: Some(worker_envelope::Payload::StartWorker(StartWorkerRequest {
                    plan: Some(plan),
                })),
            },
        )?;

        let handle = WorkerHandle { id: id.clone() };
        self.workers.insert(
            id,
            UdsWorkerProcess {
                child,
                stream,
                socket_path,
                status: WorkerStatus::Running,
            },
        );
        Ok(handle)
    }

    fn reload(&mut self, worker: &WorkerHandle, plan: DeploymentPlan) -> DaemonResult<()> {
        let process = self.worker_mut(worker)?;
        send_worker_envelope(
            &mut process.stream,
            WorkerEnvelope {
                payload: Some(worker_envelope::Payload::ReloadWorker(ReloadWorkerRequest {
                    plan: Some(plan),
                })),
            },
        )
    }

    fn stop(&mut self, worker: WorkerHandle) -> DaemonResult<()> {
        let Some(mut process) = self.workers.remove(&worker.id) else {
            return Err(DaemonError::not_found(format!(
                "worker not found: {}",
                worker.id
            )));
        };
        process.status = WorkerStatus::Stopping;
        let send_result = send_worker_envelope(
            &mut process.stream,
            WorkerEnvelope {
                payload: Some(worker_envelope::Payload::StopWorker(StopWorkerRequest {})),
            },
        );
        let stop_result = stop_child(&mut process.child, self.config.stop_timeout);
        cleanup_socket(&process.socket_path);
        send_result.and(stop_result)
    }

    fn status(&mut self, worker: &WorkerHandle) -> DaemonResult<WorkerStatus> {
        let process = self.worker_mut(worker)?;
        match process.child.try_wait() {
            Ok(Some(status)) if status.success() => {
                process.status = WorkerStatus::Stopped;
                Ok(WorkerStatus::Stopped)
            }
            Ok(Some(_)) => {
                process.status = WorkerStatus::Error;
                Ok(WorkerStatus::Error)
            }
            Ok(None) => Ok(process.status),
            Err(error) => Err(DaemonError::worker(format!(
                "failed to query worker {} status: {error}",
                worker.id
            ))),
        }
    }
}

impl UdsWorkerManager {
    fn next_worker_id(&self, plan: &DeploymentPlan) -> DaemonResult<String> {
        let id = plan
            .id
            .as_ref()
            .ok_or_else(|| DaemonError::invalid_state("deployment plan id is missing"))?;
        validate_resource_id(id)?;
        let seq = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        Ok(format!("{}-{}-{seq}", id.name, id.version))
    }

    fn socket_path(&self, worker_id: &str) -> PathBuf {
        self.config.socket_dir.join(format!("{worker_id}.sock"))
    }

    fn spawn_worker(&self, worker_id: &str, socket_path: &Path) -> DaemonResult<Child> {
        let mut command = Command::new(&self.config.worker_binary);
        command.args(&self.config.worker_args);
        command.arg("--worker");
        command.arg("--worker-id");
        command.arg(worker_id);
        command.arg("--worker-uds");
        command.arg(socket_path);
        command.spawn().map_err(|error| {
            DaemonError::worker(format!(
                "failed to spawn worker binary {}: {error}",
                self.config.worker_binary.display()
            ))
        })
    }

    fn worker_mut(&mut self, worker: &WorkerHandle) -> DaemonResult<&mut UdsWorkerProcess> {
        self.workers
            .get_mut(&worker.id)
            .ok_or_else(|| DaemonError::not_found(format!("worker not found: {}", worker.id)))
    }
}

#[derive(Debug)]
struct UdsWorkerProcess {
    child: Child,
    stream: UnixStream,
    socket_path: PathBuf,
    status: WorkerStatus,
}

fn validate_resource_id(id: &ResourceId) -> DaemonResult<()> {
    if id.name.trim().is_empty() {
        return Err(DaemonError::invalid_state("pipeline name is missing"));
    }
    if id.version.trim().is_empty() {
        return Err(DaemonError::invalid_state("pipeline version is missing"));
    }
    Ok(())
}

fn prepare_socket(socket_dir: &Path, socket_path: &Path) -> DaemonResult<()> {
    fs::create_dir_all(socket_dir).map_err(|error| {
        DaemonError::worker(format!(
            "failed to create worker socket directory {}: {error}",
            socket_dir.display()
        ))
    })?;
    cleanup_socket(socket_path);
    Ok(())
}

fn cleanup_socket(socket_path: &Path) {
    let _ = fs::remove_file(socket_path);
}

fn accept_worker(listener: &UnixListener, timeout: Duration) -> DaemonResult<UnixStream> {
    wait_until_readable(listener, timeout)?;
    listener
        .accept()
        .map(|(stream, _)| stream)
        .map_err(|error| {
            DaemonError::worker(format!("failed to accept worker UDS connection: {error}"))
        })
}

fn wait_until_readable(listener: &UnixListener, timeout: Duration) -> DaemonResult<()> {
    let mut poll_fd = libc::pollfd {
        fd: listener.as_raw_fd(),
        events: libc::POLLIN,
        revents: 0,
    };
    let timeout_millis = i32::try_from(timeout.as_millis()).unwrap_or(i32::MAX);

    loop {
        let result = unsafe { libc::poll(&mut poll_fd, 1, timeout_millis) };
        if result > 0 {
            return Ok(());
        }
        if result == 0 {
            return Err(DaemonError::worker("timed out waiting for worker UDS connect"));
        }

        let error = std::io::Error::last_os_error();
        if error.kind() != std::io::ErrorKind::Interrupted {
            return Err(DaemonError::worker(format!(
                "failed to wait for worker UDS connection: {error}"
            )));
        }
    }
}

fn send_worker_envelope(stream: &mut UnixStream, envelope: WorkerEnvelope) -> DaemonResult<()> {
    let frame = encode_frame(&envelope)
        .map_err(|error| DaemonError::worker(format!("failed to encode worker frame: {error}")))?;
    stream
        .write_all(&frame)
        .and_then(|_| stream.flush())
        .map_err(|error| DaemonError::worker(format!("failed to send worker frame: {error}")))
}

fn stop_child(child: &mut Child, timeout: Duration) -> DaemonResult<()> {
    if child
        .wait_timeout(timeout)
        .map_err(|error| DaemonError::worker(format!("failed to wait worker process: {error}")))?
        .is_some()
    {
        return Ok(());
    }

    child
        .kill()
        .map_err(|error| DaemonError::worker(format!("failed to kill timed-out worker: {error}")))?;
    child
        .wait()
        .map_err(|error| DaemonError::worker(format!("failed to reap killed worker: {error}")))?;
    Ok(())
}

fn cleanup_failed_worker(mut child: Child, socket_path: &Path) {
    let _ = child.kill();
    let _ = child.wait();
    cleanup_socket(socket_path);
}
