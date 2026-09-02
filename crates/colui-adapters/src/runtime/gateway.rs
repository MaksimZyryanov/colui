use super::ComposeOperation;
use super::{build_cli_environment, resolve_endpoint, DockerControl};
use bollard::Docker;
use colui_app::{
    ComposeInvocation, ComposeProcessResult, ComposeRunner, DockerApi, ProfileReader,
    RuntimeConnector, RuntimeFuture, RuntimeStateReader,
};
use colui_domain::{
    AppError, AppErrorCode, ContainerDetails, ContainerId, ContainerInstance, DaemonFingerprint,
    DockerEndpoint, MismatchDetails, RuntimeSessionId, RuntimeSessionState, SessionContext,
    Timestamp,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use tokio::sync::{Mutex, Semaphore};

pub trait DockerFactory: Send + Sync {
    fn connect(&self, endpoint: &DockerEndpoint) -> Result<Arc<dyn DockerControl>, AppError>;
}

struct BollardFactory;
impl DockerFactory for BollardFactory {
    fn connect(&self, endpoint: &DockerEndpoint) -> Result<Arc<dyn DockerControl>, AppError> {
        let docker = match endpoint.as_str() {
            value if value.starts_with("unix://") => {
                Docker::connect_with_unix(value, 120, bollard::API_DEFAULT_VERSION)
            }
            value if value.starts_with("tcp://") || value.starts_with("http://") => {
                Docker::connect_with_http(value, 120, bollard::API_DEFAULT_VERSION)
            }
            _ => {
                return Err(error(
                    AppErrorCode::RuntimeConnectionFailed,
                    "connect_runtime",
                    "unsupported Docker endpoint",
                ))
            }
        }
        .map_err(|e| {
            error(
                AppErrorCode::RuntimeConnectionFailed,
                "connect_runtime",
                &e.to_string(),
            )
        })?;
        Ok(Arc::new(super::DockerApiAdapter::new(docker)))
    }
}

struct FixedFactory {
    docker: Arc<dyn DockerControl>,
}
impl DockerFactory for FixedFactory {
    fn connect(&self, _: &DockerEndpoint) -> Result<Arc<dyn DockerControl>, AppError> {
        Ok(self.docker.clone())
    }
}

struct SessionClient {
    client: Arc<dyn DockerControl>,
    session_id: RuntimeSessionId,
    endpoint: DockerEndpoint,
    fingerprint: DaemonFingerprint,
}
struct Snapshot {
    generation: u64,
    state: RuntimeSessionState,
    client: Option<SessionClient>,
}

pub struct RuntimeGateway {
    factory: Arc<dyn DockerFactory>,
    runner: Mutex<Arc<dyn ComposeRunner>>,
    snapshot: Mutex<Snapshot>,
    gate: Arc<Semaphore>,
    created: AtomicUsize,
}

impl RuntimeGateway {
    pub fn new_for_tests(docker: Box<dyn DockerControl>, runner: Box<dyn ComposeRunner>) -> Self {
        Self::with_factory(
            Arc::new(FixedFactory {
                docker: docker.into(),
            }),
            runner.into(),
        )
    }
    pub fn with_factory(factory: Arc<dyn DockerFactory>, runner: Arc<dyn ComposeRunner>) -> Self {
        Self {
            factory,
            runner: Mutex::new(runner),
            snapshot: Mutex::new(Snapshot {
                generation: 0,
                state: RuntimeSessionState::Disconnected,
                client: None,
            }),
            gate: Arc::new(Semaphore::new(1)),
            created: AtomicUsize::new(0),
        }
    }
    pub fn new(runner: Box<dyn ComposeRunner>) -> Self {
        Self::with_factory(Arc::new(BollardFactory), runner.into())
    }
    pub fn created_client_count(&self) -> usize {
        self.created.load(Ordering::Relaxed)
    }
    pub fn replace_runner_for_tests(&self, runner: Box<dyn ComposeRunner>) {
        let runner = runner.into();
        let mut current = self.runner.try_lock().expect("test runner is idle");
        *current = runner;
    }

    pub async fn invoke_profile<R: ProfileReader + ?Sized>(
        &self,
        reader: &R,
        profile_id: colui_domain::ProfileId,
        operation: ComposeOperation,
    ) -> Result<ComposeProcessResult, AppError> {
        let profile = reader
            .load()
            .await?
            .profiles
            .into_iter()
            .find(|profile| profile.id == profile_id)
            .ok_or_else(|| {
                error(
                    AppErrorCode::ProfileNotFound,
                    "compose",
                    "profile not found",
                )
            })?;
        let args = super::compose_args(&profile, operation);
        let endpoint = match &self.snapshot.lock().await.state {
            RuntimeSessionState::Ready(context) => context.endpoint.clone(),
            _ => unreachable!("compose readiness checked while operation gate is held"),
        };
        let invocation = ComposeInvocation {
            executable: "docker".into(),
            args,
            working_directory: profile.working_directory,
            environment: build_cli_environment(endpoint.as_str(), std::env::vars().collect()),
            deadline: Instant::now() + Duration::from_secs(120),
        };
        self.execute_compose(invocation).await
    }

    /// Raw process invocation exists only behind adapter test support. Application code uses
    /// `invoke_profile`, which derives argv, cwd, and environment from stored profile data.
    #[doc(hidden)]
    #[cfg(feature = "test-support")]
    pub async fn invoke_backend_for_tests(
        &self,
        invocation: ComposeInvocation,
    ) -> Result<ComposeProcessResult, AppError> {
        self.execute_compose(invocation).await
    }

    async fn execute_compose(
        &self,
        invocation: ComposeInvocation,
    ) -> Result<ComposeProcessResult, AppError> {
        if let Some(error) = self.compose_error().await {
            return Err(error);
        }
        let _permit = self.gate.acquire().await.map_err(|_| {
            error(
                AppErrorCode::ComposeFailed,
                "compose",
                "compose gate closed",
            )
        })?;
        if let Some(error) = self.compose_error().await {
            return Err(error);
        }
        let runner = self.runner.lock().await.clone();
        runner.invoke(invocation).await
    }
}

fn error(code: AppErrorCode, operation: &str, message: &str) -> AppError {
    AppError::new(code, operation, None, message)
}
fn timestamp() -> Timestamp {
    Timestamp(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis()
            .to_string(),
    )
}

impl RuntimeConnector for RuntimeGateway {
    fn connect_runtime(
        &self,
        preference: Option<DockerEndpoint>,
    ) -> RuntimeFuture<'_, RuntimeSessionState> {
        Box::pin(async move {
            let _permit = self.gate.acquire().await.map_err(|_| {
                error(
                    AppErrorCode::RuntimeUnavailable,
                    "connect_runtime",
                    "runtime operation gate closed",
                )
            })?;
            let endpoint = match resolve_endpoint(
                preference.as_ref().map(DockerEndpoint::as_str),
                std::env::var("DOCKER_HOST").ok().as_deref(),
            ) {
                Ok(endpoint) => endpoint,
                Err(e) => {
                    let terminal = RuntimeSessionState::Failed(error(
                        AppErrorCode::RuntimeConnectionFailed,
                        "connect_runtime",
                        e,
                    ));
                    let mut snapshot = self.snapshot.lock().await;
                    snapshot.generation += 1;
                    snapshot.client = None;
                    snapshot.state = terminal.clone();
                    return Ok(terminal);
                }
            };
            let generation = {
                let mut snapshot = self.snapshot.lock().await;
                snapshot.generation += 1;
                snapshot.client = None;
                snapshot.state = RuntimeSessionState::Connecting;
                snapshot.generation
            };
            let (terminal, client) = match self.factory.connect(&endpoint) {
                Err(e) => (RuntimeSessionState::Failed(e), None),
                Ok(api_client) => {
                    self.created.fetch_add(1, Ordering::Relaxed);
                    match api_client.info().await {
                        Err(e) => (
                            RuntimeSessionState::Failed(error(
                                AppErrorCode::RuntimeConnectionFailed,
                                "connect_runtime",
                                &e.message,
                            )),
                            None,
                        ),
                        Ok(api_fp) => {
                            let runner = self.runner.lock().await.clone();
                            let cli = runner
                                .invoke(ComposeInvocation {
                                    executable: "docker".into(),
                                    args: vec!["info".into()],
                                    working_directory: std::env::current_dir()
                                        .unwrap_or_else(|_| ".".into()),
                                    environment: build_cli_environment(
                                        endpoint.as_str(),
                                        std::env::vars().collect(),
                                    ),
                                    deadline: Instant::now() + Duration::from_secs(30),
                                })
                                .await;
                            let cli_fp = match cli {
                                Err(e) => Err(error(
                                    AppErrorCode::RuntimeConnectionFailed,
                                    "connect_runtime",
                                    &e.message,
                                )),
                                Ok(result)
                                    if result.timed_out() || result.exit_code() != Some(0) =>
                                {
                                    Err(error(
                                        AppErrorCode::RuntimeConnectionFailed,
                                        "connect_runtime",
                                        "docker info failed",
                                    ))
                                }
                                Ok(result) => super::parse_cli_fingerprint(result.stdout())
                                    .map_err(|e| {
                                        error(
                                            AppErrorCode::RuntimeConnectionFailed,
                                            "connect_runtime",
                                            e,
                                        )
                                    }),
                            };
                            match cli_fp {
                                Err(e) => (RuntimeSessionState::Failed(e), None),
                                Ok(cli_fp) if cli_fp != api_fp => {
                                    let id = RuntimeSessionId::new(uuid::Uuid::new_v4());
                                    let state =
                                        RuntimeSessionState::ContextMismatch(MismatchDetails::new(
                                            endpoint.clone(),
                                            api_fp.clone(),
                                            cli_fp,
                                        ));
                                    (
                                        state,
                                        Some(SessionClient {
                                            client: api_client,
                                            session_id: id,
                                            endpoint,
                                            fingerprint: api_fp,
                                        }),
                                    )
                                }
                                Ok(api_fp) => {
                                    let id = RuntimeSessionId::new(uuid::Uuid::new_v4());
                                    let state = RuntimeSessionState::Ready(SessionContext {
                                        session_id: id,
                                        endpoint: endpoint.clone(),
                                        daemon_fingerprint: api_fp.clone(),
                                        connected_at: timestamp(),
                                    });
                                    (
                                        state,
                                        Some(SessionClient {
                                            client: api_client,
                                            session_id: id,
                                            endpoint,
                                            fingerprint: api_fp,
                                        }),
                                    )
                                }
                            }
                        }
                    }
                }
            };
            let mut snapshot = self.snapshot.lock().await;
            if snapshot.generation == generation {
                snapshot.state = terminal.clone();
                snapshot.client = client;
            }
            Ok(terminal)
        })
    }
    fn disconnect_runtime(&self) -> RuntimeFuture<'_, ()> {
        Box::pin(async {
            let _permit = self.gate.acquire().await.map_err(|_| {
                error(
                    AppErrorCode::RuntimeUnavailable,
                    "disconnect_runtime",
                    "runtime operation gate closed",
                )
            })?;
            let mut snapshot = self.snapshot.lock().await;
            snapshot.generation += 1;
            snapshot.client = None;
            snapshot.state = RuntimeSessionState::Disconnected;
            Ok(())
        })
    }
}

impl RuntimeGateway {
    async fn current_client(&self) -> Result<(Arc<dyn DockerControl>, u64), AppError> {
        let snapshot = self.snapshot.lock().await;
        snapshot
            .client
            .as_ref()
            .map(|client| {
                let _ = (&client.session_id, &client.endpoint, &client.fingerprint);
                (client.client.clone(), snapshot.generation)
            })
            .ok_or_else(|| {
                error(
                    AppErrorCode::RuntimeUnavailable,
                    "runtime_read",
                    "runtime unavailable",
                )
            })
    }
    async fn compose_error(&self) -> Option<AppError> {
        match self.snapshot.lock().await.state.clone() {
            RuntimeSessionState::Ready(_) => None,
            RuntimeSessionState::ContextMismatch(_) => Some(error(
                AppErrorCode::RuntimeContextMismatch,
                "compose",
                "runtime context is not verified",
            )),
            RuntimeSessionState::Failed(e) => Some(e),
            _ => Some(error(
                AppErrorCode::RuntimeUnavailable,
                "compose",
                "runtime unavailable",
            )),
        }
    }
}
impl RuntimeStateReader for RuntimeGateway {
    fn session_state(&self) -> RuntimeFuture<'_, RuntimeSessionState> {
        Box::pin(async { Ok(self.snapshot.lock().await.state.clone()) })
    }
}
impl DockerApi for RuntimeGateway {
    fn list_containers(&self) -> RuntimeFuture<'_, Vec<ContainerInstance>> {
        Box::pin(async {
            let (client, generation) = self.current_client().await?;
            let result = client.list().await?;
            if self.snapshot.lock().await.generation != generation {
                return Err(error(
                    AppErrorCode::RuntimeUnavailable,
                    "runtime_read",
                    "runtime session changed",
                ));
            }
            Ok(result)
        })
    }
    fn inspect_container(&self, id: &ContainerId) -> RuntimeFuture<'_, ContainerDetails> {
        let id = id.clone();
        Box::pin(async move {
            let (client, generation) = self.current_client().await?;
            let result = client.inspect(&id).await?;
            if self.snapshot.lock().await.generation != generation {
                return Err(error(
                    AppErrorCode::RuntimeUnavailable,
                    "runtime_read",
                    "runtime session changed",
                ));
            }
            Ok(result)
        })
    }
}
