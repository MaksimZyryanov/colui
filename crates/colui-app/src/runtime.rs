use colui_domain::{
    AppError, ContainerDetails, ContainerId, ContainerInstance, DockerEndpoint, RuntimeSessionState,
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
    fn list_containers(&self) -> RuntimeFuture<'_, Vec<ContainerInstance>>;
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
}

pub trait RuntimeStateReader: Send + Sync {
    fn session_state(&self) -> RuntimeSessionState;
}
