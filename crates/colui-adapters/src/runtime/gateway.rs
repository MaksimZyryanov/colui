use super::ComposeOperation;
use super::{build_cli_environment, resolve_endpoint, DockerControl};
use bollard::Docker;
use colui_app::{
    ComposeInvocation, ComposeProcessResult, ComposeRunner, DockerApi, LifecycleFuture,
    LifecycleOperation, LifecycleResult, LifecycleRuntime, RuntimeConnector, RuntimeDiagnostics,
    RuntimeDiagnosticsReader, RuntimeFuture, RuntimeStateReader,
};
use colui_domain::{
    AppError, AppErrorCode, ContainerDetails, ContainerId, ContainerObservation, DaemonFingerprint,
    DockerEndpoint, MismatchDetails, RuntimeInventory, RuntimeSessionId, RuntimeSessionState,
    SessionContext, Timestamp,
};
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc,
};
use std::time::{Duration, Instant};
use tokio::sync::{watch, Mutex, Semaphore};

type TransitionResult = Result<RuntimeSessionState, AppError>;

#[derive(Clone, Copy, Eq, PartialEq)]
enum TransitionKind {
    Connect,
    Disconnect,
    Reconnect,
}

struct ActiveTransition {
    id: u64,
    kind: TransitionKind,
    cancelled: Arc<AtomicBool>,
    completed: watch::Sender<Option<TransitionResult>>,
    reconnect_takeover: Option<watch::Sender<Option<TransitionResult>>>,
}

#[derive(Default)]
struct TransitionState {
    next_id: u64,
    active: Option<ActiveTransition>,
}

pub struct ComposeExecutionGate(Semaphore);

impl ComposeExecutionGate {
    pub fn new() -> Self {
        Self(Semaphore::new(1))
    }

    pub(crate) async fn acquire(&self) -> Result<tokio::sync::SemaphorePermit<'_>, AppError> {
        self.0.acquire().await.map_err(|_| {
            error(
                AppErrorCode::ComposeFailed,
                "compose",
                "compose gate closed",
            )
        })
    }
}

impl Default for ComposeExecutionGate {
    fn default() -> Self {
        Self::new()
    }
}

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
        .map_err(|_| {
            error(
                AppErrorCode::RuntimeConnectionFailed,
                "connect_runtime",
                "Runtime connection failed",
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
    connected_at: Timestamp,
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
    transition: Mutex<TransitionState>,
    gate: Arc<ComposeExecutionGate>,
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
    pub fn new_for_tests_with_gate(
        docker: Box<dyn DockerControl>,
        runner: Box<dyn ComposeRunner>,
        gate: Arc<ComposeExecutionGate>,
    ) -> Self {
        Self::with_factory_and_gate(
            Arc::new(FixedFactory {
                docker: docker.into(),
            }),
            runner.into(),
            gate,
        )
    }
    pub fn with_factory(factory: Arc<dyn DockerFactory>, runner: Arc<dyn ComposeRunner>) -> Self {
        Self::with_factory_and_gate(factory, runner, Arc::new(ComposeExecutionGate::new()))
    }

    pub fn with_factory_and_gate(
        factory: Arc<dyn DockerFactory>,
        runner: Arc<dyn ComposeRunner>,
        gate: Arc<ComposeExecutionGate>,
    ) -> Self {
        Self {
            factory,
            runner: Mutex::new(runner),
            snapshot: Mutex::new(Snapshot {
                generation: 0,
                state: RuntimeSessionState::Disconnected,
                client: None,
            }),
            transition: Mutex::new(TransitionState::default()),
            gate,
            created: AtomicUsize::new(0),
        }
    }

    pub fn with_runner(runner: Arc<dyn ComposeRunner>) -> Self {
        Self::with_factory(Arc::new(BollardFactory), runner)
    }
    pub fn with_runner_and_gate(
        runner: Arc<dyn ComposeRunner>,
        gate: Arc<ComposeExecutionGate>,
    ) -> Self {
        Self::with_factory_and_gate(Arc::new(BollardFactory), runner, gate)
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

    async fn invoke_loaded_profile(
        &self,
        profile: colui_domain::ProjectProfile,
        operation: ComposeOperation,
    ) -> Result<ComposeProcessResult, AppError> {
        let _permit = self.gate.acquire().await?;
        let state = self.snapshot.lock().await.state.clone();
        let endpoint = match state {
            RuntimeSessionState::Ready(context) => context.endpoint,
            _ => {
                return Err(self
                    .compose_error()
                    .await
                    .expect("non-ready state has compose error"))
            }
        };
        let args = super::compose_args(&profile, operation);
        let invocation = ComposeInvocation {
            executable: "docker".into(),
            args,
            working_directory: profile.working_directory,
            environment: build_cli_environment(endpoint.as_str(), std::env::vars().collect()),
            deadline: Instant::now() + Duration::from_secs(120),
        };
        let runner = self.runner.lock().await.clone();
        runner.invoke(invocation).await
    }

    /// Raw process invocation exists only behind adapter test support.
    #[doc(hidden)]
    #[cfg(feature = "test-support")]
    pub async fn invoke_backend_for_tests(
        &self,
        invocation: ComposeInvocation,
    ) -> Result<ComposeProcessResult, AppError> {
        if let Some(error) = self.compose_error().await {
            return Err(error);
        }
        let _permit = self.gate.acquire().await?;
        if let Some(error) = self.compose_error().await {
            return Err(error);
        }
        let runner = self.runner.lock().await.clone();
        runner.invoke(invocation).await
    }
}

impl LifecycleRuntime for RuntimeGateway {
    fn run_profile(
        &self,
        profile: colui_domain::ProjectProfile,
        operation: LifecycleOperation,
    ) -> LifecycleFuture<'_, LifecycleResult> {
        Box::pin(async move {
            let profile_id = profile.id.clone();
            let compose_operation = match operation {
                LifecycleOperation::Apply => ComposeOperation::Up,
                LifecycleOperation::Stop => ComposeOperation::Stop,
                LifecycleOperation::TearDown => ComposeOperation::Down,
                LifecycleOperation::Restart => ComposeOperation::Restart,
            };
            let result = self
                .invoke_loaded_profile(profile, compose_operation)
                .await?;
            if result.timed_out() {
                return Err(AppError::new(
                    AppErrorCode::OperationTimeout,
                    "lifecycle",
                    Some(profile_id),
                    "compose operation timed out",
                ));
            }
            if result.exit_code() != Some(0) {
                return Err(AppError::new(
                    AppErrorCode::ComposeFailed,
                    "lifecycle",
                    Some(profile_id),
                    "compose exited unsuccessfully",
                ));
            }
            Ok(LifecycleResult {
                profile_id,
                success: true,
                inventory: RuntimeInventory::unavailable(),
            })
        })
    }
}

fn error(code: AppErrorCode, operation: &str, message: &str) -> AppError {
    AppError::new(code, operation, None, message)
}
fn timestamp() -> Timestamp {
    Timestamp(chrono::Utc::now().to_rfc3339())
}

enum TransitionDecision {
    Join(watch::Receiver<Option<TransitionResult>>),
    Wait(watch::Receiver<Option<TransitionResult>>),
    Own { id: u64 },
}

fn install_transition(state: &mut TransitionState, kind: TransitionKind) -> TransitionDecision {
    state.next_id = state.next_id.wrapping_add(1);
    let id = state.next_id;
    let (completed, _) = watch::channel(None);
    state.active = Some(ActiveTransition {
        id,
        kind,
        cancelled: Arc::new(AtomicBool::new(false)),
        completed,
        reconnect_takeover: None,
    });
    TransitionDecision::Own { id }
}

async fn wait_transition(
    mut completed: watch::Receiver<Option<TransitionResult>>,
) -> TransitionResult {
    loop {
        if let Some(result) = completed.borrow().clone() {
            return result;
        }
        completed.changed().await.map_err(|_| {
            error(
                AppErrorCode::RuntimeUnavailable,
                "runtime_transition",
                "runtime transition cancelled",
            )
        })?;
    }
}

impl RuntimeConnector for RuntimeGateway {
    fn connect_runtime(
        &self,
        preference: Option<DockerEndpoint>,
    ) -> RuntimeFuture<'_, RuntimeSessionState> {
        Box::pin(async move { self.connect(preference).await })
    }
    fn disconnect_runtime(&self) -> RuntimeFuture<'_, ()> {
        Box::pin(async move { self.disconnect().await })
    }
    fn reconnect_runtime(
        &self,
        preference: Option<DockerEndpoint>,
    ) -> RuntimeFuture<'_, RuntimeSessionState> {
        Box::pin(async move { self.reconnect(preference).await })
    }
}

impl RuntimeGateway {
    async fn connect(&self, preference: Option<DockerEndpoint>) -> TransitionResult {
        loop {
            let decision = {
                let mut transitions = self.transition.lock().await;
                match transitions.active.as_ref() {
                    Some(active) if active.kind == TransitionKind::Connect => {
                        TransitionDecision::Join(active.completed.subscribe())
                    }
                    Some(active) if active.kind == TransitionKind::Reconnect => {
                        TransitionDecision::Join(active.completed.subscribe())
                    }
                    Some(active) => TransitionDecision::Wait(active.completed.subscribe()),
                    None => {
                        if let RuntimeSessionState::Ready(context) =
                            self.snapshot.lock().await.state.clone()
                        {
                            return Ok(RuntimeSessionState::Ready(context));
                        }
                        install_transition(&mut transitions, TransitionKind::Connect)
                    }
                }
            };
            match decision {
                TransitionDecision::Join(receiver) => return wait_transition(receiver).await,
                TransitionDecision::Wait(receiver) => {
                    let _ = wait_transition(receiver).await;
                }
                TransitionDecision::Own { id, .. } => {
                    let established = self.establish(preference.clone(), false).await;
                    return self.finish_connect(id, preference, established).await;
                }
            }
        }
    }

    async fn disconnect(&self) -> Result<(), AppError> {
        let decision = {
            let mut transitions = self.transition.lock().await;
            match transitions.active.as_ref() {
                Some(active)
                    if active.kind == TransitionKind::Connect
                        && active.reconnect_takeover.is_none() =>
                {
                    active.cancelled.store(true, Ordering::Release);
                    TransitionDecision::Join(active.completed.subscribe())
                }
                Some(active) if active.kind == TransitionKind::Disconnect => {
                    TransitionDecision::Join(active.completed.subscribe())
                }
                Some(_) => {
                    return Err(error(
                        AppErrorCode::OperationConflict,
                        "disconnect_runtime",
                        "reconnect is in progress",
                    ))
                }
                None => install_transition(&mut transitions, TransitionKind::Disconnect),
            }
        };
        match decision {
            TransitionDecision::Join(receiver) | TransitionDecision::Wait(receiver) => {
                wait_transition(receiver).await?;
            }
            TransitionDecision::Own { id, .. } => {
                let result = self
                    .gate
                    .acquire()
                    .await
                    .map(|_permit| RuntimeSessionState::Disconnected);
                self.finish_transition(id, result, None).await?;
            }
        }
        Ok(())
    }

    async fn reconnect(&self, preference: Option<DockerEndpoint>) -> TransitionResult {
        loop {
            let decision = {
                let mut transitions = self.transition.lock().await;
                match transitions.active.as_mut() {
                    Some(active) if active.kind == TransitionKind::Connect => {
                        if let Some(takeover) = &active.reconnect_takeover {
                            TransitionDecision::Join(takeover.subscribe())
                        } else {
                            active.cancelled.store(true, Ordering::Release);
                            let (takeover, receiver) = watch::channel(None);
                            active.reconnect_takeover = Some(takeover);
                            TransitionDecision::Join(receiver)
                        }
                    }
                    Some(active) if active.kind == TransitionKind::Disconnect => {
                        TransitionDecision::Wait(active.completed.subscribe())
                    }
                    Some(active) => TransitionDecision::Join(active.completed.subscribe()),
                    None => install_transition(&mut transitions, TransitionKind::Reconnect),
                }
            };
            match decision {
                TransitionDecision::Join(receiver) => return wait_transition(receiver).await,
                TransitionDecision::Wait(receiver) => {
                    let _ = wait_transition(receiver).await;
                }
                TransitionDecision::Own { id, .. } => {
                    let established = self.establish(preference, true).await;
                    return self.finish_establish(id, established).await;
                }
            }
        }
    }

    async fn establish(
        &self,
        preference: Option<DockerEndpoint>,
        invalidate_current: bool,
    ) -> Result<(RuntimeSessionState, Option<SessionClient>), AppError> {
        let _permit = self.gate.acquire().await.map_err(|_| {
            error(
                AppErrorCode::RuntimeUnavailable,
                "connect_runtime",
                "runtime operation gate closed",
            )
        })?;
        if invalidate_current {
            let mut snapshot = self.snapshot.lock().await;
            snapshot.generation += 1;
            snapshot.client = None;
        }
        let endpoint = match resolve_endpoint(
            preference.as_ref().map(DockerEndpoint::as_str),
            std::env::var("DOCKER_HOST").ok().as_deref(),
        ) {
            Ok(endpoint) => endpoint,
            Err(message) => {
                return Ok((
                    RuntimeSessionState::Failed(error(
                        AppErrorCode::RuntimeConnectionFailed,
                        "connect_runtime",
                        message,
                    )),
                    None,
                ))
            }
        };
        let api_client = match self.factory.connect(&endpoint) {
            Ok(client) => client,
            Err(error) => return Ok((RuntimeSessionState::Failed(error), None)),
        };
        self.created.fetch_add(1, Ordering::Relaxed);
        let api_fp = match api_client.info().await {
            Ok(fingerprint) => fingerprint,
            Err(source) => {
                return Ok((
                    RuntimeSessionState::Failed(error(
                        AppErrorCode::RuntimeConnectionFailed,
                        "connect_runtime",
                        &source.message,
                    )),
                    None,
                ))
            }
        };
        let runner = self.runner.lock().await.clone();
        let cli = runner
            .invoke(ComposeInvocation {
                executable: "docker".into(),
                args: vec!["info".into()],
                working_directory: std::env::current_dir().unwrap_or_else(|_| ".".into()),
                environment: build_cli_environment(endpoint.as_str(), std::env::vars().collect()),
                deadline: Instant::now() + Duration::from_secs(30),
            })
            .await;
        let cli_fp = match cli {
            Err(source) => Err(error(
                AppErrorCode::RuntimeConnectionFailed,
                "connect_runtime",
                &source.message,
            )),
            Ok(result) if result.timed_out() || result.exit_code() != Some(0) => Err(error(
                AppErrorCode::RuntimeConnectionFailed,
                "connect_runtime",
                "docker info failed",
            )),
            Ok(result) => super::parse_cli_fingerprint(result.stdout()).map_err(|message| {
                error(
                    AppErrorCode::RuntimeConnectionFailed,
                    "connect_runtime",
                    message,
                )
            }),
        };
        let cli_fp = match cli_fp {
            Ok(fingerprint) => fingerprint,
            Err(error) => return Ok((RuntimeSessionState::Failed(error), None)),
        };
        let session_id = RuntimeSessionId::new(uuid::Uuid::new_v4());
        let connected_at = timestamp();
        let client = SessionClient {
            client: api_client,
            session_id,
            endpoint: endpoint.clone(),
            fingerprint: api_fp.clone(),
            connected_at: connected_at.clone(),
        };
        let state = if cli_fp == api_fp {
            RuntimeSessionState::Ready(SessionContext {
                session_id,
                endpoint,
                daemon_fingerprint: api_fp,
                connected_at,
            })
        } else {
            RuntimeSessionState::ContextMismatch(MismatchDetails::new(endpoint, api_fp, cli_fp))
        };
        Ok((state, Some(client)))
    }

    async fn finish_connect(
        &self,
        id: u64,
        preference: Option<DockerEndpoint>,
        established: Result<(RuntimeSessionState, Option<SessionClient>), AppError>,
    ) -> TransitionResult {
        let takeover = {
            let mut transitions = self.transition.lock().await;
            let active = transitions
                .active
                .as_mut()
                .filter(|active| active.id == id)
                .expect("connect owner must retain active transition");
            if active.cancelled.load(Ordering::Acquire) {
                if let Some(takeover) = active.reconnect_takeover.take() {
                    active
                        .completed
                        .send_replace(Some(Ok(RuntimeSessionState::Disconnected)));
                    active.kind = TransitionKind::Reconnect;
                    active.cancelled.store(false, Ordering::Release);
                    active.completed = takeover;
                    true
                } else {
                    false
                }
            } else {
                false
            }
        };
        if takeover {
            drop(established);
            let fresh = self.establish(preference, true).await;
            let _ = self.finish_establish(id, fresh).await;
            Ok(RuntimeSessionState::Disconnected)
        } else {
            self.finish_establish(id, established).await
        }
    }

    async fn finish_establish(
        &self,
        id: u64,
        established: Result<(RuntimeSessionState, Option<SessionClient>), AppError>,
    ) -> TransitionResult {
        let cancelled = self
            .transition
            .lock()
            .await
            .active
            .as_ref()
            .is_some_and(|active| active.id == id && active.cancelled.load(Ordering::Acquire));
        if cancelled {
            self.finish_transition(id, Ok(RuntimeSessionState::Disconnected), None)
                .await
        } else {
            match established {
                Ok((state, client)) => self.finish_transition(id, Ok(state), client).await,
                Err(error) => self.finish_transition(id, Err(error), None).await,
            }
        }
    }

    async fn finish_transition(
        &self,
        id: u64,
        mut result: TransitionResult,
        mut client: Option<SessionClient>,
    ) -> TransitionResult {
        let mut transitions = self.transition.lock().await;
        if transitions.active.as_ref().is_some_and(|active| {
            active.id == id
                && active.kind == TransitionKind::Connect
                && active.cancelled.load(Ordering::Acquire)
        }) {
            result = Ok(RuntimeSessionState::Disconnected);
            client = None;
        }
        let active = transitions
            .active
            .take()
            .filter(|active| active.id == id)
            .expect("transition owner must finish its active transition");
        if let Ok(state) = &result {
            let mut snapshot = self.snapshot.lock().await;
            snapshot.generation += 1;
            snapshot.state = state.clone();
            snapshot.client = client;
        }
        active.completed.send_replace(Some(result.clone()));
        result
    }

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
    fn api_read_context(&self) -> RuntimeFuture<'_, SessionContext> {
        Box::pin(async {
            let snapshot = self.snapshot.lock().await;
            let client = snapshot.client.as_ref().ok_or_else(|| {
                error(
                    AppErrorCode::RuntimeUnavailable,
                    "runtime_read",
                    "runtime unavailable",
                )
            })?;
            Ok(SessionContext {
                session_id: client.session_id,
                endpoint: client.endpoint.clone(),
                daemon_fingerprint: client.fingerprint.clone(),
                connected_at: client.connected_at.clone(),
            })
        })
    }
}
impl RuntimeDiagnosticsReader for RuntimeGateway {
    fn runtime_diagnostics(&self) -> colui_app::DiagnosticsFuture<'_, RuntimeDiagnostics> {
        Box::pin(async {
            let snapshot = self.snapshot.lock().await;
            let client = snapshot.client.as_ref();
            let cli_fingerprint = match &snapshot.state {
                RuntimeSessionState::ContextMismatch(details) => {
                    Some(details.cli_fingerprint.clone())
                }
                RuntimeSessionState::Ready(context) => Some(context.daemon_fingerprint.clone()),
                _ => None,
            };
            Ok(RuntimeDiagnostics {
                state: snapshot.state.clone(),
                resolved_endpoint: client.map(|value| value.endpoint.clone()),
                api_fingerprint: client.map(|value| value.fingerprint.clone()),
                cli_fingerprint,
                session_id: client.map(|value| value.session_id),
                connected_at: client.map(|value| value.connected_at.clone()),
            })
        })
    }
}
impl DockerApi for RuntimeGateway {
    fn list_containers(&self) -> RuntimeFuture<'_, Vec<ContainerObservation>> {
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
