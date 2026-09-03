use bollard::models::ContainerSummary;
use colui_adapters::runtime::{normalize_container_summary, DockerControl, RuntimeGateway};
use colui_app::{
    ComposeInvocation, ComposeProcessResult, ComposeRunner, DockerApi, RuntimeConnector,
    RuntimeFuture,
};
use colui_domain::{
    AppError, AppErrorCode, ContainerDetails, ContainerId, ContainerObservation, DaemonFingerprint,
};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};
use std::time::Duration;

#[tokio::test]
async fn list_returns_compose_metadata_without_inspect() {
    let gateway = gateway_with_summary(compose_summary("checkout", "web")).await;
    let values = gateway.list_containers().await.unwrap();
    assert_eq!(values[0].compose.as_ref().unwrap().project, "checkout");
    assert_eq!(gateway.inspect_call_count(), 0);
}

#[tokio::test]
async fn list_maps_all_four_compose_labels_from_one_docker_call() {
    let gateway = gateway_with_summary(compose_summary("checkout", "web")).await;
    let values = gateway.list_containers().await.unwrap();

    let compose = values[0].compose.as_ref().unwrap();
    assert_eq!(compose.service.as_deref(), Some("web"));
    assert_eq!(compose.working_directory.as_deref(), Some("/workspace"));
    assert_eq!(
        compose.config_files,
        vec![
            "/workspace/compose.yml".to_owned(),
            "/workspace/compose.override.yml".to_owned()
        ]
    );
    assert_eq!(gateway.list_call_count(), 1);
    assert_eq!(gateway.inspect_call_count(), 0);
}

#[test]
fn missing_project_label_is_standalone_observation() {
    let value = normalize_summary(standalone_summary());
    assert!(value.compose.is_none());
}

#[test]
fn standalone_observation_still_maps_core_instance_fields() {
    let value = normalize_summary(standalone_summary());
    assert_eq!(value.instance.id.0, "xyz789");
    assert_eq!(value.instance.name, "standalone");
    assert_eq!(value.instance.image, "alpine:latest");
    assert!(value.instance.service_name.is_none());
}

fn normalize_summary(summary: ContainerSummary) -> ContainerObservation {
    normalize_container_summary(summary)
}

fn compose_summary(project: &str, service: &str) -> ContainerSummary {
    serde_json::from_str(&format!(
        r#"{{
            "Id": "abc123",
            "Names": ["/{project}-{service}-1"],
            "Image": "{project}:latest",
            "State": "running",
            "Status": "Up 2 hours",
            "Labels": {{
                "com.docker.compose.project": "{project}",
                "com.docker.compose.service": "{service}",
                "com.docker.compose.working_dir": "/workspace",
                "com.docker.compose.config-files": "/workspace/compose.yml,/workspace/compose.override.yml",
                "com.docker.compose.container-number": "1"
            }},
            "Ports": []
        }}"#
    ))
    .unwrap()
}

fn standalone_summary() -> ContainerSummary {
    serde_json::from_str(
        r#"{
            "Id": "xyz789",
            "Names": ["/standalone"],
            "Image": "alpine:latest",
            "State": "running",
            "Status": "Up 1 hour",
            "Labels": {"maintainer": "team"},
            "Ports": []
        }"#,
    )
    .unwrap()
}

async fn gateway_with_summary(summary: ContainerSummary) -> ObservingGateway {
    let docker = Arc::new(CountingDocker::new(vec![summary]));
    let gateway = RuntimeGateway::new_for_tests(
        Box::new(CountingDocker {
            summaries: docker.summaries.clone(),
            list_calls: docker.list_calls.clone(),
            inspect_calls: docker.inspect_calls.clone(),
        }),
        Box::new(FingerprintRunner),
    );
    gateway.connect_runtime(None).await.unwrap();
    ObservingGateway { gateway, docker }
}

struct ObservingGateway {
    gateway: RuntimeGateway,
    docker: Arc<CountingDocker>,
}

impl ObservingGateway {
    async fn list_containers(&self) -> Result<Vec<ContainerObservation>, AppError> {
        DockerApi::list_containers(&self.gateway).await
    }
    fn inspect_call_count(&self) -> usize {
        self.docker.inspect_calls.load(Ordering::Acquire)
    }
    fn list_call_count(&self) -> usize {
        self.docker.list_calls.load(Ordering::Acquire)
    }
}

struct CountingDocker {
    summaries: Arc<Vec<ContainerSummary>>,
    list_calls: Arc<AtomicUsize>,
    inspect_calls: Arc<AtomicUsize>,
}

impl CountingDocker {
    fn new(summaries: Vec<ContainerSummary>) -> Self {
        Self {
            summaries: Arc::new(summaries),
            list_calls: Arc::new(AtomicUsize::new(0)),
            inspect_calls: Arc::new(AtomicUsize::new(0)),
        }
    }
}

impl DockerControl for CountingDocker {
    fn info(&self) -> RuntimeFuture<'_, DaemonFingerprint> {
        Box::pin(async { Ok(DaemonFingerprint::new("same", "1", "linux", "x86_64")) })
    }
    fn list(&self) -> RuntimeFuture<'_, Vec<ContainerObservation>> {
        self.list_calls.fetch_add(1, Ordering::AcqRel);
        let summaries = self.summaries.clone();
        Box::pin(async move {
            Ok(summaries
                .iter()
                .cloned()
                .map(normalize_container_summary)
                .collect())
        })
    }
    fn inspect(&self, _: &ContainerId) -> RuntimeFuture<'_, ContainerDetails> {
        self.inspect_calls.fetch_add(1, Ordering::AcqRel);
        Box::pin(async {
            Err(AppError::new(
                AppErrorCode::RuntimeUnavailable,
                "inspect",
                None,
                "fast refresh must not inspect",
            ))
        })
    }
}

struct FingerprintRunner;

impl ComposeRunner for FingerprintRunner {
    fn invoke(&self, _: ComposeInvocation) -> RuntimeFuture<'_, ComposeProcessResult> {
        Box::pin(async {
            Ok(ComposeProcessResult::completed(
                0,
                "ID: same\nServer Version: 1\nOSType: linux\nArchitecture: x86_64\n",
                "",
                Duration::ZERO,
            ))
        })
    }
}
