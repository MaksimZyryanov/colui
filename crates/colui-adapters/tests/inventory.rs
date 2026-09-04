use bollard::models::ContainerSummary;
use colui_adapters::runtime::{normalize_container_summary, DockerControl, RuntimeGateway};
use colui_adapters::InventoryCoordinator;
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

#[tokio::test]
async fn concurrent_refreshes_share_one_list_and_generation() {
    let gateway = gateway_with_summary(compose_summary("checkout", "web")).await;
    let coordinator = InventoryCoordinator::new(Arc::new(gateway.gateway));
    let (a, b) = tokio::join!(coordinator.refresh(), coordinator.refresh());

    let a = a.unwrap();
    let b = b.unwrap();
    assert_eq!(a.generation, 1);
    assert_eq!(a.generation, b.generation);
    assert_eq!(gateway.docker.list_calls.load(Ordering::Acquire), 1);
}

#[tokio::test]
async fn first_inventory_is_unavailable_and_success_groups_observations() {
    let gateway = gateway_with_summary(compose_summary("checkout", "web")).await;
    let coordinator = InventoryCoordinator::new(Arc::new(gateway.gateway));
    let before = coordinator.current_inventory().await.unwrap();
    assert!(!before.has_snapshot);
    assert_eq!(before.generation, 0);

    let snapshot = coordinator.refresh().await.unwrap();
    assert!(snapshot.has_snapshot);
    assert_eq!(
        snapshot.project_snapshots[0].compose_project_name,
        "checkout"
    );
    assert!(snapshot.standalone_containers.is_empty());
}

#[tokio::test]
async fn successful_refresh_notifies_subscriber_once() {
    let gateway = gateway_with_summary(compose_summary("checkout", "web")).await;
    let coordinator = InventoryCoordinator::new(Arc::new(gateway.gateway));
    let mut subscriber = coordinator.subscribe();

    coordinator.refresh().await.unwrap();
    assert_eq!(subscriber.recv().await.unwrap().generation, 1);
    assert!(subscriber.try_recv().is_err());
}

#[tokio::test]
async fn refresh_failure_retains_snapshot_and_automatic_backoff() {
    let source = Arc::new(QueuedSource::new(vec![
        Ok(vec![observation("first", Some("checkout"))]),
        Err(runtime_error("offline")),
        Ok(vec![observation("third", None)]),
    ]));
    let coordinator = InventoryCoordinator::new(source.clone());
    assert_eq!(coordinator.refresh().await.unwrap().generation, 1);

    let retained = coordinator.refresh().await.unwrap();
    assert_eq!(retained.generation, 1);
    assert_eq!(retained.freshness, colui_domain::InventoryFreshness::Stale);
    assert_eq!(retained.containers[0].name, "first");
    assert!(retained.error.is_some());

    let blocked = coordinator.refresh_automatic().await.unwrap();
    assert_eq!(blocked.generation, 1);
    assert_eq!(source.calls.load(Ordering::Acquire), 2);
    assert_eq!(coordinator.refresh().await.unwrap().generation, 2);
}

fn observation(name: &str, project: Option<&str>) -> ContainerObservation {
    ContainerObservation::new(
        colui_domain::ContainerInstance {
            id: ContainerId(name.to_owned()),
            name: name.to_owned(),
            image: "image".to_owned(),
            state: colui_domain::ContainerState::Running,
            status_text: "Up".to_owned(),
            service_name: None,
            published_ports: Vec::new(),
        },
        project.map(colui_domain::ComposeContainerMetadata::project),
    )
}

fn runtime_error(message: &str) -> AppError {
    AppError::new(AppErrorCode::RuntimeUnavailable, "list", None, message)
}

struct QueuedSource {
    values:
        std::sync::Mutex<std::collections::VecDeque<Result<Vec<ContainerObservation>, AppError>>>,
    calls: AtomicUsize,
    context: colui_domain::SessionContext,
}

impl QueuedSource {
    fn new(values: Vec<Result<Vec<ContainerObservation>, AppError>>) -> Self {
        Self {
            values: std::sync::Mutex::new(values.into()),
            calls: AtomicUsize::new(0),
            context: colui_domain::SessionContext {
                session_id: colui_domain::RuntimeSessionId::new(uuid::Uuid::from_u128(1)),
                endpoint: colui_domain::DockerEndpoint::try_from("unix:///tmp/docker.sock")
                    .unwrap(),
                daemon_fingerprint: DaemonFingerprint::new("same", "1", "linux", "x86_64"),
                connected_at: colui_domain::Timestamp("now".to_owned()),
            },
        }
    }
}

impl DockerApi for QueuedSource {
    fn list_containers(&self) -> RuntimeFuture<'_, Vec<ContainerObservation>> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        let value = self.values.lock().unwrap().pop_front().unwrap();
        Box::pin(async move { value })
    }

    fn inspect_container(&self, _: &ContainerId) -> RuntimeFuture<'_, ContainerDetails> {
        panic!("inventory refresh must not inspect")
    }
}

impl colui_app::RuntimeStateReader for QueuedSource {
    fn session_state(&self) -> RuntimeFuture<'_, colui_domain::RuntimeSessionState> {
        let context = self.context.clone();
        Box::pin(async move { Ok(colui_domain::RuntimeSessionState::Ready(context)) })
    }
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
            tokio::time::sleep(Duration::from_millis(10)).await;
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
