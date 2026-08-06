use crate::DaemonError;
use crate::DaemonResult;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use tenon_message::daemon::v1::{
    worker_envelope, worker_response_envelope, GetWorkerStatsRequest, ReloadWorkerRequest,
    StartWorkerRequest, StopWorkerRequest, WorkerEnvelope, WorkerHelloRequest,
    WorkerResponseEnvelope, WorkerState, WorkerStats,
};
use tenon_message::plan::{DeploymentPlan, MqttSourceClientIds, ResourceId};
use wait_timeout::ChildExt;
use tenon_transport::{
    Requester, Transport, UdsRequester, UdsTransportConfig, UdsTransportProvider,
};

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

#[derive(Debug, Clone, PartialEq)]
pub struct WorkerDeployment {
    pub plan: DeploymentPlan,
    pub source_client_ids: Vec<MqttSourceClientIds>,
}

#[allow(async_fn_in_trait)]
pub trait WorkerManager {
    async fn start(&mut self, deployment: WorkerDeployment) -> DaemonResult<WorkerHandle>;

    async fn reload(&mut self, worker: &WorkerHandle, plan: DeploymentPlan) -> DaemonResult<()>;

    async fn stop(&mut self, worker: WorkerHandle) -> DaemonResult<()>;

    async fn status(&mut self, worker: &WorkerHandle) -> DaemonResult<WorkerStatus>;

    async fn stats(&mut self, worker: &WorkerHandle) -> DaemonResult<WorkerStats>;
}

#[derive(Debug, Default)]
pub struct NoopWorkerManager {
    next_id: AtomicU64,
}

impl WorkerManager for NoopWorkerManager {
    async fn start(&mut self, _deployment: WorkerDeployment) -> DaemonResult<WorkerHandle> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed) + 1;
        Ok(WorkerHandle {
            id: format!("worker-{id}"),
        })
    }

    async fn reload(
        &mut self,
        _worker: &WorkerHandle,
        _plan: DeploymentPlan,
    ) -> DaemonResult<()> {
        Ok(())
    }

    async fn stop(&mut self, _worker: WorkerHandle) -> DaemonResult<()> {
        Ok(())
    }

    async fn status(&mut self, _worker: &WorkerHandle) -> DaemonResult<WorkerStatus> {
        Ok(WorkerStatus::Running)
    }

    async fn stats(&mut self, _worker: &WorkerHandle) -> DaemonResult<WorkerStats> {
        Ok(WorkerStats::default())
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
    async fn start(&mut self, deployment: WorkerDeployment) -> DaemonResult<WorkerHandle> {
        let id = self.next_worker_id(&deployment.plan)?;
        let socket_path = self.socket_path(&id);
        prepare_socket(&self.config.socket_dir, &socket_path)?;

        let child = self.spawn_worker(&id, &socket_path)?;
        let requester = match connect_worker(&socket_path, self.config.connect_timeout).await {
            Ok(requester) => requester,
            Err(error) => {
                cleanup_failed_worker(child, &socket_path);
                return Err(error);
            }
        };
        if let Err(error) = handshake_worker(&requester, &id, self.config.connect_timeout).await {
            cleanup_failed_worker(child, &socket_path);
            return Err(error);
        }
        let response = request_worker_response(
            &requester,
            WorkerEnvelope {
                payload: Some(worker_envelope::Payload::StartWorker(StartWorkerRequest {
                    plan: Some(deployment.plan),
                    source_client_ids: deployment.source_client_ids,
                })),
            },
            self.config.connect_timeout,
        )
        .await?;
        check_worker_response(
            response,
            WorkerResponseKind::Start,
            WorkerState::Running,
        )?;

        let handle = WorkerHandle { id: id.clone() };
        self.workers.insert(
            id,
            UdsWorkerProcess {
                child,
                requester,
                socket_path,
                status: WorkerStatus::Running,
            },
        );
        Ok(handle)
    }

    async fn reload(&mut self, worker: &WorkerHandle, plan: DeploymentPlan) -> DaemonResult<()> {
        let timeout = self.config.connect_timeout;
        let request = WorkerEnvelope {
            payload: Some(worker_envelope::Payload::ReloadWorker(ReloadWorkerRequest {
                plan: Some(plan),
            })),
        };
        let response = match self.request_worker(worker, request.clone(), timeout).await {
            Ok(response) => response,
            Err(_) => {
                self.reconnect_worker(worker).await?;
                self.request_worker(worker, request, timeout).await?
            }
        };
        check_worker_response(
            response,
            WorkerResponseKind::Reload,
            WorkerState::Running,
        )
    }

    async fn stop(&mut self, worker: WorkerHandle) -> DaemonResult<()> {
        let Some(mut process) = self.workers.remove(&worker.id) else {
            return Err(DaemonError::not_found(format!(
                "worker not found: {}",
                worker.id
            )));
        };
        process.status = WorkerStatus::Stopping;
        let response_result = request_worker_response(
            &process.requester,
            WorkerEnvelope {
                payload: Some(worker_envelope::Payload::StopWorker(StopWorkerRequest {})),
            },
            self.config.stop_timeout,
        )
        .await
        .and_then(|response| {
            check_worker_response(
                response,
                WorkerResponseKind::Stop,
                WorkerState::Stopped,
            )
        });
        let stop_result = stop_child(&mut process.child, self.config.stop_timeout);
        cleanup_socket(&process.socket_path);
        response_result.and(stop_result)
    }

    async fn status(&mut self, worker: &WorkerHandle) -> DaemonResult<WorkerStatus> {
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

    async fn stats(&mut self, worker: &WorkerHandle) -> DaemonResult<WorkerStats> {
        let timeout = self.config.connect_timeout;
        let request = WorkerEnvelope {
            payload: Some(worker_envelope::Payload::GetWorkerStats(
                GetWorkerStatsRequest {},
            )),
        };
        let response = match self.request_worker(worker, request.clone(), timeout).await {
            Ok(response) => response,
            Err(_) => {
                self.reconnect_worker(worker).await?;
                self.request_worker(worker, request, timeout).await?
            }
        };
        let Some(worker_response_envelope::Payload::WorkerStats(response)) = response.payload else {
            return Err(DaemonError::worker("worker stats response payload is missing"));
        };
        if !response.error.is_empty() {
            return Err(DaemonError::worker(response.error));
        }
        response
            .stats
            .ok_or_else(|| DaemonError::worker("worker stats are missing"))
    }
}

impl UdsWorkerManager {
    async fn request_worker(
        &mut self,
        worker: &WorkerHandle,
        request: WorkerEnvelope,
        timeout: Duration,
    ) -> DaemonResult<WorkerResponseEnvelope> {
        let process = self.worker_mut(worker)?;
        request_worker_response(&process.requester, request, timeout).await
    }

    async fn reconnect_worker(&mut self, worker: &WorkerHandle) -> DaemonResult<()> {
        let socket_path = self.worker_mut(worker)?.socket_path.clone();
        let requester = connect_worker(&socket_path, self.config.connect_timeout).await?;
        handshake_worker(&requester, &worker.id, self.config.connect_timeout).await?;
        self.worker_mut(worker)?.requester = requester;
        Ok(())
    }

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
    requester: UdsRequester<WorkerEnvelope, WorkerResponseEnvelope>,
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

async fn connect_worker(
    path: &Path,
    timeout: Duration,
) -> DaemonResult<UdsRequester<WorkerEnvelope, WorkerResponseEnvelope>> {
    let path = path.to_owned();
    let display_path = path.clone();
    let provider = Transport::provide::<UdsTransportProvider>(UdsTransportConfig {
        connect_timeout: timeout,
        ..UdsTransportConfig::default()
    });
    provider.connect::<WorkerEnvelope, WorkerResponseEnvelope>(path).await.map_err(|error| {
        DaemonError::worker(format!(
            "failed to connect to worker UDS {}: {error}",
            display_path.display()
        ))
    })
}

async fn handshake_worker(
    requester: &UdsRequester<WorkerEnvelope, WorkerResponseEnvelope>,
    worker_id: &str,
    timeout: Duration,
) -> DaemonResult<()> {
    let response = request_worker_response(
        requester,
        WorkerEnvelope {
            payload: Some(worker_envelope::Payload::WorkerHello(WorkerHelloRequest {
                worker_id: worker_id.to_string(),
                protocol_version: WORKER_PROTOCOL_VERSION,
            })),
        },
        timeout,
    )
    .await?;
    check_worker_hello(response, worker_id)
}

fn check_worker_hello(
    envelope: WorkerResponseEnvelope,
    expected_worker_id: &str,
) -> DaemonResult<()> {
    let Some(worker_response_envelope::Payload::WorkerHello(response)) = envelope.payload else {
        return Err(DaemonError::worker("worker hello response payload is missing"));
    };
    if !response.error.is_empty() {
        return Err(DaemonError::worker(response.error));
    }
    if response.worker_id != expected_worker_id {
        return Err(DaemonError::worker(format!(
            "worker identity mismatch: expected={expected_worker_id} actual={}",
            response.worker_id
        )));
    }
    if response.protocol_version != WORKER_PROTOCOL_VERSION {
        return Err(DaemonError::worker("worker protocol version mismatch"));
    }
    Ok(())
}

enum WorkerResponseKind {
    Start,
    Reload,
    Stop,
}

const WORKER_PROTOCOL_VERSION: u32 = 1;

async fn request_worker_response(
    requester: &UdsRequester<WorkerEnvelope, WorkerResponseEnvelope>,
    request: WorkerEnvelope,
    timeout: Duration,
) -> DaemonResult<WorkerResponseEnvelope> {
    let result = tokio::time::timeout(timeout, async move {
        requester
            .request(request)
            .await
            .map_err(|error| DaemonError::worker(format!("worker request failed: {error}")))
    })
    .await;
    result.map_err(|_| DaemonError::worker("timed out waiting for worker response"))?
}

fn check_worker_response(
    envelope: WorkerResponseEnvelope,
    expected: WorkerResponseKind,
    expected_state: WorkerState,
) -> DaemonResult<()> {
    let payload = envelope
        .payload
        .ok_or_else(|| DaemonError::worker("worker response payload is missing"))?;
    let (state, error) = match (expected, payload) {
        (WorkerResponseKind::Start, worker_response_envelope::Payload::StartWorker(response)) => {
            (response.state, response.error)
        }
        (WorkerResponseKind::Reload, worker_response_envelope::Payload::ReloadWorker(response)) => {
            (response.state, response.error)
        }
        (WorkerResponseKind::Stop, worker_response_envelope::Payload::StopWorker(response)) => {
            (response.state, response.error)
        }
        _ => return Err(DaemonError::worker("unexpected worker response payload")),
    };
    if !error.is_empty() {
        return Err(DaemonError::worker(error));
    }
    let actual_state = WorkerState::try_from(state)
        .map_err(|_| DaemonError::worker("worker returned an unknown state"))?;
    if actual_state != expected_state {
        return Err(DaemonError::worker(format!(
            "worker returned unexpected state: expected={expected_state:?} actual={actual_state:?}"
        )));
    }
    Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DaemonErrorKind;
    use std::future::Future;
    use tenon_message::daemon::v1::{
        worker_envelope, worker_response_envelope, ReloadWorkerResponse, StopWorkerResponse,
        WorkerEnvelope, WorkerResponseEnvelope, WorkerState,
    };
    use tenon_message::plan::MqttSourceClientIds;
    use tenon_transport::{test_support, Responder, UdsResponder};

    fn block_on<F: Future>(future: F) -> F::Output {
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("create test runtime")
            .block_on(future)
    }

    #[test]
    fn uds_requester_sends_worker_envelope() {
        block_on(async {
            let (daemon_stream, mut worker_stream) = worker_pair();
            let plan = plan();
            let deployment = deployment(plan.clone());
            let responder = tokio::spawn(async move {
                let envelope = worker_stream.receive().await.expect("decode frame");
                worker_stream
                    .respond(WorkerResponseEnvelope::default())
                    .await
                    .expect("response");
                envelope
            });
            daemon_stream
                .request(WorkerEnvelope {
                    payload: Some(worker_envelope::Payload::StartWorker(StartWorkerRequest {
                        plan: Some(plan.clone()),
                        source_client_ids: deployment.source_client_ids.clone(),
                    })),
                })
                .await
                .expect("request");
            let envelope = responder.await.expect("responder");
            match envelope.payload {
                Some(worker_envelope::Payload::StartWorker(request)) => {
                    assert_eq!(request.plan, Some(plan));
                    assert_eq!(request.source_client_ids, deployment.source_client_ids);
                }
                other => panic!("unexpected worker envelope: {other:?}"),
            }
        });
    }

    #[test]
    fn uds_worker_manager_reload_sends_reload_frame() {
        let mut manager = UdsWorkerManager::new(UdsWorkerManagerConfig::new(
            "/bin/unused",
            unique_socket_dir("reload"),
        ));
        let handle = WorkerHandle {
            id: "worker-reload".to_string(),
        };
        let plan = plan();
        let expected_plan = plan.clone();
        block_on(async {
            let (daemon_stream, mut worker_stream) = worker_pair();
            let responder = tokio::spawn(async move {
            let envelope: WorkerEnvelope = worker_stream.receive().await.expect("decode frame");
            match envelope.payload {
                Some(worker_envelope::Payload::ReloadWorker(request)) => {
                    assert_eq!(request.plan, Some(expected_plan));
                    send_worker_response(
                        &mut worker_stream,
                        WorkerResponseEnvelope {
                            payload: Some(worker_response_envelope::Payload::ReloadWorker(
                                ReloadWorkerResponse {
                                    state: WorkerState::Running as i32,
                                    error: String::new(),
                                },
                            )),
                        },
                    ).await;
                }
                other => panic!("unexpected worker envelope: {other:?}"),
            }
            let envelope: WorkerEnvelope = worker_stream.receive().await.expect("decode frame");
            assert!(matches!(
                envelope.payload,
                Some(worker_envelope::Payload::StopWorker(_))
            ));
            send_worker_response(
                &mut worker_stream,
                WorkerResponseEnvelope {
                    payload: Some(worker_response_envelope::Payload::StopWorker(
                        StopWorkerResponse {
                            state: WorkerState::Stopped as i32,
                            error: String::new(),
                        },
                    )),
                },
            ).await;
            });

            insert_test_worker(&mut manager, handle.id.as_str(), daemon_stream).await;
            manager
                .reload(&handle, plan.clone())
                .await
                .expect("reload worker");
            manager.stop(handle).await.expect("stop worker");
            responder.await.expect("worker responder");
        });
    }

    #[test]
    fn uds_worker_manager_stop_sends_stop_frame_and_removes_worker() {
        let mut manager = UdsWorkerManager::new(UdsWorkerManagerConfig::new(
            "/bin/unused",
            unique_socket_dir("stop"),
        ));
        let handle = WorkerHandle {
            id: "worker-stop".to_string(),
        };
        block_on(async {
            let (daemon_stream, mut worker_stream) = worker_pair();
            let responder = tokio::spawn(async move {
            let envelope: WorkerEnvelope = worker_stream.receive().await.expect("decode frame");
            assert!(matches!(
                envelope.payload,
                Some(worker_envelope::Payload::StopWorker(_))
            ));
            send_worker_response(
                &mut worker_stream,
                WorkerResponseEnvelope {
                    payload: Some(worker_response_envelope::Payload::StopWorker(
                        StopWorkerResponse {
                            state: WorkerState::Stopped as i32,
                            error: String::new(),
                        },
                    )),
                },
            ).await;
        });

            insert_test_worker(&mut manager, handle.id.as_str(), daemon_stream).await;
            manager.stop(handle.clone()).await.expect("stop worker");
            responder.await.expect("worker responder");
        });
        assert!(manager.worker_mut(&handle).is_err());
    }

    #[test]
    fn uds_worker_manager_rejects_plan_without_id_before_spawning() {
        let mut manager = UdsWorkerManager::new(UdsWorkerManagerConfig::new(
            "/bin/unused",
            unique_socket_dir("missing-id"),
        ));

        let error = block_on(manager.start(deployment(DeploymentPlan::default())))
            .expect_err("missing id should fail before spawn");

        assert_eq!(error.kind, DaemonErrorKind::InvalidState);
        assert_eq!(error.message, "deployment plan id is missing");
    }

    #[test]
    fn uds_worker_manager_rejects_reload_for_unknown_worker() {
        let mut manager = UdsWorkerManager::new(UdsWorkerManagerConfig::new(
            "/bin/unused",
            unique_socket_dir("unknown-reload"),
        ));
        let worker = WorkerHandle {
            id: "missing-worker".to_string(),
        };

        let error = block_on(manager.reload(&worker, plan()))
            .expect_err("unknown worker should fail");

        assert_eq!(error.kind, DaemonErrorKind::NotFound);
    }

    async fn insert_test_worker(
        manager: &mut UdsWorkerManager,
        id: &str,
        requester: UdsRequester<WorkerEnvelope, WorkerResponseEnvelope>,
    ) -> WorkerHandle {
        let handle = WorkerHandle { id: id.to_string() };
        manager.workers.insert(
            handle.id.clone(),
            UdsWorkerProcess {
                child: spawn_sleep_child(),
                requester,
                socket_path: unique_socket_path(id),
                status: WorkerStatus::Running,
            },
        );
        handle
    }

    fn spawn_sleep_child() -> Child {
        Command::new("sleep")
            .arg("60")
            .spawn()
            .expect("spawn sleep child")
    }

    async fn send_worker_response(
        stream: &mut UdsResponder<WorkerEnvelope, WorkerResponseEnvelope>,
        response: WorkerResponseEnvelope,
    ) {
        stream.respond(response).await.expect("write response");
    }

    fn worker_pair() -> (
        UdsRequester<WorkerEnvelope, WorkerResponseEnvelope>,
        UdsResponder<WorkerEnvelope, WorkerResponseEnvelope>,
    ) {
        let provider = Transport::provide::<UdsTransportProvider>(UdsTransportConfig::default());
        test_support::uds_pair(&provider).expect("create UDS transport pair")
    }

    fn plan() -> DeploymentPlan {
        DeploymentPlan {
            id: Some(ResourceId {
                name: "sensor-pipeline".to_string(),
                version: "r1".to_string(),
            }),
            ..DeploymentPlan::default()
        }
    }

    fn deployment(plan: DeploymentPlan) -> WorkerDeployment {
        WorkerDeployment {
            plan,
            source_client_ids: vec![MqttSourceClientIds {
                source_index: 0,
                client_ids: vec!["client-0".to_string()],
            }],
        }
    }

    fn unique_socket_dir(label: &str) -> PathBuf {
        let dir = PathBuf::from("target").join(format!(
            "tdw-{label}-{}-{}",
            std::process::id(),
            unique_seq()
        ));
        fs::create_dir_all(&dir).expect("create socket dir");
        dir
    }

    fn unique_socket_path(label: &str) -> PathBuf {
        unique_socket_dir(label).join("worker.sock")
    }

    fn unique_seq() -> u64 {
        static NEXT_ID: AtomicU64 = AtomicU64::new(0);
        NEXT_ID.fetch_add(1, Ordering::Relaxed)
    }
}
