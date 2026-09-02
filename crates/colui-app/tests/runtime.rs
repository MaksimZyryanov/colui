use colui_app::{
    ComposeInvocation, ComposeProcessResult, ComposeRunner, DockerApi, RuntimeConnector,
    RuntimeStateReader,
};
use colui_domain::{
    AppError, ContainerDetails, ContainerId, ContainerInstance, ContainerState, DockerEndpoint,
    RuntimeSessionState,
};
use std::collections::BTreeMap;
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::time::{Duration, Instant};

struct FakeRuntime;

impl FakeRuntime {
    fn ready() -> Self {
        Self
    }
}

impl DockerApi for FakeRuntime {
    fn list_containers(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<ContainerInstance>, AppError>> + Send + '_>> {
        Box::pin(async {
            Ok(vec![ContainerInstance {
                id: ContainerId("container-1".into()),
                name: "demo".into(),
                image: "demo:latest".into(),
                state: ContainerState::Running,
                status_text: "Up".into(),
                service_name: Some("web".into()),
                published_ports: vec![],
            }])
        })
    }

    fn inspect_container(
        &self,
        _container_id: &ContainerId,
    ) -> Pin<Box<dyn Future<Output = Result<ContainerDetails, AppError>> + Send + '_>> {
        Box::pin(async {
            Err(AppError::new(
                colui_domain::AppErrorCode::RuntimeUnavailable,
                "test",
                None,
                "not implemented",
            ))
        })
    }
}

impl ComposeRunner for FakeRuntime {
    fn invoke(
        &self,
        _invocation: ComposeInvocation,
    ) -> Pin<Box<dyn Future<Output = Result<ComposeProcessResult, AppError>> + Send + '_>> {
        Box::pin(async { Ok(ComposeProcessResult::completed(0, "", "", Duration::ZERO)) })
    }
}

impl RuntimeConnector for FakeRuntime {
    fn connect_runtime(
        &self,
        _preference: Option<DockerEndpoint>,
    ) -> Pin<Box<dyn Future<Output = Result<RuntimeSessionState, AppError>> + Send + '_>> {
        Box::pin(async { Ok(RuntimeSessionState::Disconnected) })
    }

    fn disconnect_runtime(
        &self,
    ) -> Pin<Box<dyn Future<Output = Result<(), AppError>> + Send + '_>> {
        Box::pin(async { Ok(()) })
    }
}

impl RuntimeStateReader for FakeRuntime {
    fn session_state(&self) -> RuntimeSessionState {
        RuntimeSessionState::Disconnected
    }
}

fn fake_invocation() -> ComposeInvocation {
    ComposeInvocation {
        executable: PathBuf::from("docker"),
        args: vec!["compose".into(), "up".into()],
        working_directory: PathBuf::from("/tmp/demo"),
        environment: BTreeMap::new(),
        deadline: Instant::now() + Duration::from_secs(5),
    }
}

#[tokio::test]
async fn ports_use_shared_borrow_and_do_not_construct_runtime_clients() {
    let fake = FakeRuntime::ready();
    let containers = DockerApi::list_containers(&fake).await.unwrap();
    assert_eq!(containers.len(), 1);
}

#[tokio::test]
async fn compose_runner_accepts_structured_invocation() {
    let fake = FakeRuntime::ready();
    let result = ComposeRunner::invoke(&fake, fake_invocation())
        .await
        .unwrap();
    assert_eq!(result.exit_code(), Some(0));
}

#[test]
fn process_result_exposes_independent_truncation_and_timeout() {
    let result =
        ComposeProcessResult::from_timeout("tail", "error", true, false, Duration::from_secs(1));
    assert!(result.timed_out());
    assert!(result.stdout_truncated());
    assert!(!result.stderr_truncated());
}
