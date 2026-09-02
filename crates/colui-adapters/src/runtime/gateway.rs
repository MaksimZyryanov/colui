use super::{build_cli_environment, resolve_endpoint, DockerControl};
use colui_app::{
    ComposeInvocation, ComposeProcessResult, ComposeRunner, DockerApi, RuntimeConnector,
    RuntimeFuture, RuntimeStateReader,
};
use colui_domain::{
    AppError, AppErrorCode, ContainerDetails, ContainerId, ContainerInstance, DockerEndpoint,
    MismatchDetails, RuntimeSessionId, RuntimeSessionState, SessionContext, Timestamp,
};
use std::collections::BTreeMap;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::{Duration, Instant};
use tokio::sync::Semaphore;

pub struct RuntimeGateway {
    docker: Arc<dyn DockerControl>,
    runner: Mutex<Arc<dyn ComposeRunner>>,
    state: Mutex<RuntimeSessionState>,
    client: Mutex<Option<(Arc<dyn DockerControl>, RuntimeSessionId)>>,
    gate: Arc<Semaphore>,
    created: AtomicUsize,
}
impl RuntimeGateway {
    pub fn new_for_tests(docker: Box<dyn DockerControl>, runner: Box<dyn ComposeRunner>) -> Self {
        Self {
            docker: docker.into(),
            runner: Mutex::new(runner.into()),
            state: Mutex::new(RuntimeSessionState::Disconnected),
            client: Mutex::new(None),
            gate: Arc::new(Semaphore::new(1)),
            created: AtomicUsize::new(0),
        }
    }
    pub fn created_client_count(&self) -> usize {
        self.created.load(Ordering::Relaxed)
    }
    pub fn replace_runner_for_tests(&self, runner: Box<dyn ComposeRunner>) {
        *self.runner.lock().unwrap() = runner.into();
    }
    pub fn new(runner: Box<dyn ComposeRunner>) -> Result<Self, AppError> {
        let endpoint = resolve_endpoint(None, None).map_err(|error| {
            Self::error(
                AppErrorCode::RuntimeConnectionFailed,
                "connect_runtime",
                error,
            )
        })?;
        let docker = bollard::Docker::connect_with_unix(
            endpoint.as_str().trim_start_matches("unix://"),
            120,
            bollard::API_DEFAULT_VERSION,
        )
        .map_err(|error| {
            Self::error(
                AppErrorCode::RuntimeConnectionFailed,
                "connect_runtime",
                &error.to_string(),
            )
        })?;
        Ok(Self::new_for_tests(
            Box::new(super::DockerApiAdapter::new(docker)),
            runner,
        ))
    }
    fn error(code: AppErrorCode, operation: &str, message: &str) -> AppError {
        AppError::new(code, operation, None, message)
    }
    fn current_client(&self) -> Result<Arc<dyn DockerControl>, AppError> {
        self.client
            .lock()
            .unwrap()
            .as_ref()
            .map(|(c, _)| c.clone())
            .ok_or_else(|| {
                Self::error(
                    AppErrorCode::RuntimeUnavailable,
                    "runtime_read",
                    "runtime unavailable",
                )
            })
    }
    fn compose_error(&self) -> Option<AppError> {
        match &*self.state.lock().unwrap() {
            RuntimeSessionState::Disconnected => Some(Self::error(
                AppErrorCode::RuntimeUnavailable,
                "compose",
                "runtime unavailable",
            )),
            RuntimeSessionState::ContextMismatch(_) => Some(Self::error(
                AppErrorCode::RuntimeContextMismatch,
                "compose",
                "runtime context is not verified",
            )),
            RuntimeSessionState::Failed(error) => Some(error.clone()),
            _ => None,
        }
    }
}
impl RuntimeConnector for RuntimeGateway {
    fn connect_runtime(
        &self,
        preference: Option<DockerEndpoint>,
    ) -> RuntimeFuture<'_, RuntimeSessionState> {
        Box::pin(async move {
            let endpoint = resolve_endpoint(
                preference.as_ref().map(|v| v.as_str()),
                std::env::var("DOCKER_HOST").ok().as_deref(),
            )
            .map_err(|e| {
                Self::error(AppErrorCode::RuntimeConnectionFailed, "connect_runtime", e)
            })?;
            let api = match self.docker.info().await {
                Ok(value) => value,
                Err(error) => {
                    *self.client.lock().unwrap() = None;
                    let state = RuntimeSessionState::Failed(error);
                    *self.state.lock().unwrap() = state.clone();
                    return Ok(state);
                }
            };
            let runner = self.runner.lock().unwrap().clone();
            let cli = match runner
                .invoke(ComposeInvocation {
                    executable: "docker".into(),
                    args: vec!["info".into()],
                    working_directory: ".".into(),
                    environment: build_cli_environment(endpoint.as_str(), BTreeMap::new()),
                    deadline: Instant::now() + Duration::from_secs(30),
                })
                .await
            {
                Ok(value) => value,
                Err(error) => {
                    *self.client.lock().unwrap() = None;
                    let state = RuntimeSessionState::Failed(error);
                    *self.state.lock().unwrap() = state.clone();
                    return Ok(state);
                }
            };
            let cli_fp = match super::parse_cli_fingerprint(cli.stdout()) {
                Ok(value) => value,
                Err(error) => {
                    let error = Self::error(
                        AppErrorCode::RuntimeConnectionFailed,
                        "connect_runtime",
                        error,
                    );
                    *self.client.lock().unwrap() = None;
                    let state = RuntimeSessionState::Failed(error);
                    *self.state.lock().unwrap() = state.clone();
                    return Ok(state);
                }
            };
            let session_id = RuntimeSessionId::new(uuid::Uuid::new_v4());
            let state = if api == cli_fp {
                RuntimeSessionState::Ready(SessionContext {
                    session_id,
                    endpoint,
                    daemon_fingerprint: api,
                    connected_at: Timestamp("now".into()),
                })
            } else {
                RuntimeSessionState::ContextMismatch(MismatchDetails::new(endpoint, api, cli_fp))
            };
            *self.client.lock().unwrap() = Some((self.docker.clone(), session_id));
            self.created.fetch_add(1, Ordering::Relaxed);
            *self.state.lock().unwrap() = state.clone();
            Ok(state)
        })
    }
    fn disconnect_runtime(&self) -> RuntimeFuture<'_, ()> {
        Box::pin(async {
            *self.client.lock().unwrap() = None;
            *self.state.lock().unwrap() = RuntimeSessionState::Disconnected;
            Ok(())
        })
    }
}
impl RuntimeStateReader for RuntimeGateway {
    fn session_state(&self) -> RuntimeFuture<'_, RuntimeSessionState> {
        Box::pin(async { Ok(self.state.lock().unwrap().clone()) })
    }
}
impl DockerApi for RuntimeGateway {
    fn list_containers(&self) -> RuntimeFuture<'_, Vec<ContainerInstance>> {
        Box::pin(async { self.current_client()?.list().await })
    }
    fn inspect_container(&self, id: &ContainerId) -> RuntimeFuture<'_, ContainerDetails> {
        let id = id.clone();
        Box::pin(async move { self.current_client()?.inspect(&id).await })
    }
}
impl ComposeRunner for RuntimeGateway {
    fn invoke(&self, invocation: ComposeInvocation) -> RuntimeFuture<'_, ComposeProcessResult> {
        Box::pin(async move {
            if let Some(error) = self.compose_error() {
                return Err(error);
            }
            let _permit = self.gate.acquire().await.map_err(|_| {
                Self::error(
                    AppErrorCode::ComposeFailed,
                    "compose",
                    "compose gate closed",
                )
            })?;
            let runner = self.runner.lock().unwrap().clone();
            runner.invoke(invocation).await
        })
    }
}
