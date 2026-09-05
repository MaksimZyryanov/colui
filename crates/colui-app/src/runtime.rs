use colui_domain::{
    AppError, ContainerDetails, ContainerId, ContainerObservation, DockerEndpoint,
    RuntimeSessionState, Timestamp,
};
use std::collections::BTreeMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::{Duration, Instant};

pub type RuntimeFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, AppError>> + Send + 'a>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComposeInvocation {
    pub executable: PathBuf,
    pub args: Vec<String>,
    pub working_directory: PathBuf,
    pub environment: BTreeMap<String, String>,
    pub deadline: Instant,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ComposeProcessResult {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub stdout_truncated: bool,
    pub stderr_truncated: bool,
    pub timed_out: bool,
    pub duration: Duration,
}

impl ComposeProcessResult {
    pub fn completed(
        exit_code: i32,
        stdout: impl Into<String>,
        stderr: impl Into<String>,
        duration: Duration,
    ) -> Self {
        Self {
            exit_code: Some(exit_code),
            stdout: stdout.into(),
            stderr: stderr.into(),
            stdout_truncated: false,
            stderr_truncated: false,
            timed_out: false,
            duration,
        }
    }

    pub fn from_timeout(
        stdout: impl Into<String>,
        stderr: impl Into<String>,
        stdout_truncated: bool,
        stderr_truncated: bool,
        duration: Duration,
    ) -> Self {
        Self {
            exit_code: None,
            stdout: stdout.into(),
            stderr: stderr.into(),
            stdout_truncated,
            stderr_truncated,
            timed_out: true,
            duration,
        }
    }

    pub fn exit_code(&self) -> Option<i32> {
        self.exit_code
    }

    pub fn stdout(&self) -> &str {
        &self.stdout
    }

    pub fn stderr(&self) -> &str {
        &self.stderr
    }

    pub fn stdout_truncated(&self) -> bool {
        self.stdout_truncated
    }

    pub fn stderr_truncated(&self) -> bool {
        self.stderr_truncated
    }

    pub fn timed_out(&self) -> bool {
        self.timed_out
    }

    pub fn duration(&self) -> Duration {
        self.duration
    }
}

pub trait DockerApi: Send + Sync {
    fn list_containers(&self) -> RuntimeFuture<'_, Vec<ContainerObservation>>;
    fn inspect_container(&self, container_id: &ContainerId) -> RuntimeFuture<'_, ContainerDetails>;
}

pub trait ComposeRunner: Send + Sync {
    fn invoke(&self, invocation: ComposeInvocation) -> RuntimeFuture<'_, ComposeProcessResult>;
}

pub trait RuntimeConnector: Send + Sync {
    fn connect_runtime(
        &self,
        preference: Option<DockerEndpoint>,
    ) -> RuntimeFuture<'_, RuntimeSessionState>;
    fn disconnect_runtime(&self) -> RuntimeFuture<'_, ()>;
    fn reconnect_runtime(
        &self,
        preference: Option<DockerEndpoint>,
    ) -> RuntimeFuture<'_, RuntimeSessionState>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ReconnectResult {
    pub runtime_state: RuntimeSessionState,
    pub inventory: Option<colui_domain::RuntimeInventory>,
}

pub struct ReconnectRuntime<'a, C: ?Sized, I: ?Sized> {
    connector: &'a C,
    inventory: &'a I,
}

impl<'a, C, I> ReconnectRuntime<'a, C, I>
where
    C: RuntimeConnector + ?Sized,
    I: crate::InventoryRefresher + ?Sized,
{
    pub fn new(connector: &'a C, inventory: &'a I) -> Self {
        Self {
            connector,
            inventory,
        }
    }

    pub async fn execute(
        &self,
        preference: Option<DockerEndpoint>,
    ) -> Result<ReconnectResult, AppError> {
        let runtime_state = self.connector.reconnect_runtime(preference).await?;
        match &runtime_state {
            RuntimeSessionState::Ready(_) | RuntimeSessionState::ContextMismatch(_) => {
                let inventory = match self.inventory.refresh().await {
                    Ok(inventory) => inventory,
                    Err(_) => self.inventory.current_inventory().await?,
                };
                Ok(ReconnectResult {
                    runtime_state,
                    inventory: Some(inventory),
                })
            }
            RuntimeSessionState::Failed(error) => Err(error.clone()),
            _ => Ok(ReconnectResult {
                runtime_state,
                inventory: None,
            }),
        }
    }
}

pub trait RuntimeStateReader: Send + Sync {
    fn session_state(&self) -> RuntimeFuture<'_, RuntimeSessionState>;
    fn api_read_context(&self) -> RuntimeFuture<'_, colui_domain::SessionContext> {
        Box::pin(async move {
            match self.session_state().await? {
                RuntimeSessionState::Ready(context) => Ok(context),
                _ => Err(AppError::new(
                    colui_domain::AppErrorCode::RuntimeUnavailable,
                    "runtime_read",
                    None,
                    "runtime unavailable",
                )),
            }
        })
    }
}

pub trait RuntimeInventorySource: DockerApi + RuntimeStateReader {}
impl<T: DockerApi + RuntimeStateReader> RuntimeInventorySource for T {}

pub trait Clock: Send + Sync {
    fn now(&self) -> Timestamp;
    fn monotonic(&self) -> Duration;
}
