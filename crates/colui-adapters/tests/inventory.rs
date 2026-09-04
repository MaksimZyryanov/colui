use bollard::models::ContainerSummary;
use colui_adapters::runtime::{normalize_container_summary, DockerControl, RuntimeGateway};
use colui_adapters::InventoryCoordinator;
use colui_app::{
    Clock, ComposeInvocation, ComposeProcessResult, ComposeRunner, DockerApi, RuntimeConnector,
    RuntimeFuture,
};
use colui_domain::{
    AppError, AppErrorCode, ContainerDetails, ContainerId, ContainerObservation, DaemonFingerprint,
};
use std::sync::{
    atomic::{AtomicU64, AtomicUsize, Ordering},
    Arc, Mutex,
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
    let source = Arc::new(BlockingSource::new(vec![observation(
        "checkout-web",
        Some("checkout"),
    )]));
    let coordinator = Arc::new(InventoryCoordinator::new(source.clone(), test_clock()));
    let mut joined = coordinator.subscribe_refresh_joins();
    let first = tokio::spawn({
        let coordinator = coordinator.clone();
        async move { coordinator.refresh().await }
    });
    source.started.notified().await;
    let second = tokio::spawn({
        let coordinator = coordinator.clone();
        async move { coordinator.refresh().await }
    });
    joined.recv().await.unwrap();
    assert_eq!(source.calls.load(Ordering::Acquire), 1);
    source.release.add_permits(2);

    let (a, b) = tokio::join!(first, second);
    let a = a.unwrap().unwrap();
    let b = b.unwrap().unwrap();
    assert_eq!(a.generation, 1);
    assert_eq!(a.generation, b.generation);
    assert_eq!(source.calls.load(Ordering::Acquire), 1);
}

#[tokio::test]
async fn lifecycle_refresh_joins_background_refresh() {
    let source = Arc::new(BlockingSource::new(vec![observation(
        "checkout-web",
        Some("checkout"),
    )]));
    let coordinator = Arc::new(InventoryCoordinator::new(source.clone(), test_clock()));
    let background = tokio::spawn({
        let coordinator = coordinator.clone();
        async move { coordinator.refresh().await }
    });
    source.started.notified().await;
    let mut joined = coordinator.subscribe_refresh_joins();
    let lifecycle = tokio::spawn({
        let coordinator = coordinator.clone();
        async move { colui_app::InventoryRefresher::refresh(coordinator.as_ref()).await }
    });
    tokio::task::yield_now().await;
    source.release.add_permits(2);

    let background = background.await.unwrap().unwrap();
    let lifecycle = lifecycle.await.unwrap().unwrap();
    assert_eq!(background.generation, lifecycle.generation);
    assert_eq!(source.calls.load(Ordering::Acquire), 1);
    assert!(joined.try_recv().is_ok());
}

#[tokio::test]
async fn session_change_during_list_rejects_obsolete_response_and_retains_snapshot() {
    let source = Arc::new(MutableSessionSource::new(vec![observation("first", None)]));
    let coordinator = Arc::new(InventoryCoordinator::new(source.clone(), test_clock()));
    source.release.add_permits(1);
    assert_eq!(coordinator.refresh().await.unwrap().generation, 1);
    source.started.notified().await;

    source.set_observations(vec![observation("obsolete", None)]);
    let refresh = tokio::spawn({
        let coordinator = coordinator.clone();
        async move { coordinator.refresh().await }
    });
    source.started.notified().await;
    source.set_session(2);
    source.release.add_permits(1);

    assert_eq!(
        refresh.await.unwrap().unwrap_err().code,
        AppErrorCode::RuntimeUnavailable
    );
    let retained = coordinator.current_inventory().await.unwrap();
    assert_eq!(retained.generation, 1);
    assert_eq!(retained.containers[0].name, "first");
    assert_eq!(retained.freshness, colui_domain::InventoryFreshness::Stale);
}

#[tokio::test]
async fn disconnect_during_list_rejects_obsolete_response_and_retains_snapshot() {
    let source = Arc::new(MutableSessionSource::new(vec![observation("first", None)]));
    let coordinator = Arc::new(InventoryCoordinator::new(source.clone(), test_clock()));
    source.release.add_permits(1);
    assert_eq!(coordinator.refresh().await.unwrap().generation, 1);
    source.started.notified().await;

    source.set_observations(vec![observation("obsolete", None)]);
    let refresh = tokio::spawn({
        let coordinator = coordinator.clone();
        async move { coordinator.refresh().await }
    });
    source.started.notified().await;
    source.disconnect();
    source.release.add_permits(1);

    assert_eq!(
        refresh.await.unwrap().unwrap_err().code,
        AppErrorCode::RuntimeUnavailable
    );
    let retained = coordinator.current_inventory().await.unwrap();
    assert_eq!(retained.generation, 1);
    assert_eq!(retained.containers[0].name, "first");
    assert_eq!(retained.freshness, colui_domain::InventoryFreshness::Stale);
}

#[tokio::test]
async fn cancelled_creator_does_not_leak_in_flight_refresh() {
    let source = Arc::new(BlockingSource::new(vec![]));
    let coordinator = Arc::new(InventoryCoordinator::new(source.clone(), test_clock()));
    let first = tokio::spawn({
        let coordinator = coordinator.clone();
        async move { coordinator.refresh().await }
    });
    source.started.notified().await;
    first.abort();
    source.release.add_permits(2);

    let completed = coordinator.refresh().await.unwrap();
    assert_eq!(completed.generation, 1);
    assert_eq!(source.calls.load(Ordering::Acquire), 1);
    assert_eq!(coordinator.refresh().await.unwrap().generation, 2);
    assert_eq!(source.calls.load(Ordering::Acquire), 2);
}

#[tokio::test]
async fn first_inventory_is_unavailable_and_success_groups_observations() {
    let gateway = gateway_with_summary(compose_summary("checkout", "web")).await;
    let coordinator = InventoryCoordinator::new(Arc::new(gateway.gateway), test_clock());
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
    let coordinator = InventoryCoordinator::new(Arc::new(gateway.gateway), test_clock());
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
    let clock = test_clock();
    let coordinator = InventoryCoordinator::new(source.clone(), clock.clone());
    assert_eq!(coordinator.refresh().await.unwrap().generation, 1);

    assert_eq!(
        coordinator.refresh().await.unwrap_err().code,
        AppErrorCode::RuntimeUnavailable
    );
    let retained = coordinator.current_inventory().await.unwrap();
    assert_eq!(retained.generation, 1);
    assert_eq!(retained.freshness, colui_domain::InventoryFreshness::Stale);
    assert_eq!(retained.containers[0].name, "first");
    assert!(retained.error.is_some());

    let blocked = coordinator.refresh_automatic().await.unwrap();
    assert_eq!(blocked.generation, 1);
    assert_eq!(source.calls.load(Ordering::Acquire), 2);
    assert_eq!(coordinator.refresh().await.unwrap().generation, 2);
}

#[tokio::test]
async fn automatic_backoff_uses_injected_clock_and_caps_then_resets() {
    let source = Arc::new(QueuedSource::new(vec![
        Err(runtime_error("1")),
        Err(runtime_error("2")),
        Err(runtime_error("3")),
        Err(runtime_error("4")),
        Err(runtime_error("5")),
        Ok(vec![]),
        Err(runtime_error("reset")),
        Ok(vec![]),
    ]));
    let clock = test_clock();
    let coordinator = InventoryCoordinator::new(source.clone(), clock.clone());

    for (attempt, delay) in [1, 2, 4, 8, 10].into_iter().enumerate() {
        assert!(coordinator.refresh_automatic().await.is_err());
        let calls = attempt + 1;
        assert_eq!(source.calls.load(Ordering::Acquire), calls);
        assert!(coordinator.refresh_automatic().await.is_ok());
        assert_eq!(source.calls.load(Ordering::Acquire), calls);
        clock.advance(delay);
    }
    assert_eq!(coordinator.refresh_automatic().await.unwrap().generation, 1);
    assert!(coordinator.refresh_automatic().await.is_err());
    assert_eq!(source.calls.load(Ordering::Acquire), 7);
    assert!(coordinator.refresh_automatic().await.is_ok());
    assert_eq!(source.calls.load(Ordering::Acquire), 7);
    clock.advance(1);
    assert_eq!(coordinator.refresh_automatic().await.unwrap().generation, 2);
}

#[derive(Default)]
struct TestClock {
    seconds: AtomicU64,
}

impl TestClock {
    fn advance(&self, seconds: u64) {
        self.seconds.fetch_add(seconds, Ordering::AcqRel);
    }
}

impl Clock for TestClock {
    fn now(&self) -> colui_domain::Timestamp {
        colui_domain::Timestamp(format!(
            "2026-09-04T00:00:{:02}Z",
            self.seconds.load(Ordering::Acquire)
        ))
    }

    fn monotonic(&self) -> Duration {
        Duration::from_secs(self.seconds.load(Ordering::Acquire))
    }
}

fn test_clock() -> Arc<TestClock> {
    Arc::new(TestClock::default())
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

struct BlockingSource {
    observations: Vec<ContainerObservation>,
    calls: AtomicUsize,
    started: tokio::sync::Notify,
    release: tokio::sync::Semaphore,
    context: colui_domain::SessionContext,
}

struct MutableSessionSource {
    observations: Mutex<Vec<ContainerObservation>>,
    state: Mutex<colui_domain::RuntimeSessionState>,
    started: tokio::sync::Notify,
    release: tokio::sync::Semaphore,
}

impl MutableSessionSource {
    fn new(observations: Vec<ContainerObservation>) -> Self {
        Self {
            observations: Mutex::new(observations),
            state: Mutex::new(colui_domain::RuntimeSessionState::Ready(session_context())),
            started: tokio::sync::Notify::new(),
            release: tokio::sync::Semaphore::new(0),
        }
    }

    fn set_observations(&self, observations: Vec<ContainerObservation>) {
        *self.observations.lock().unwrap() = observations;
    }

    fn set_session(&self, value: u128) {
        let mut state = self.state.lock().unwrap();
        let colui_domain::RuntimeSessionState::Ready(context) = &mut *state else {
            panic!("test session must be ready");
        };
        context.session_id = colui_domain::RuntimeSessionId::new(uuid::Uuid::from_u128(value));
    }

    fn disconnect(&self) {
        *self.state.lock().unwrap() = colui_domain::RuntimeSessionState::Disconnected;
    }
}

impl DockerApi for MutableSessionSource {
    fn list_containers(&self) -> RuntimeFuture<'_, Vec<ContainerObservation>> {
        Box::pin(async move {
            self.started.notify_one();
            self.release.acquire().await.unwrap().forget();
            Ok(self.observations.lock().unwrap().clone())
        })
    }

    fn inspect_container(&self, _: &ContainerId) -> RuntimeFuture<'_, ContainerDetails> {
        panic!("inventory refresh must not inspect")
    }
}

impl colui_app::RuntimeStateReader for MutableSessionSource {
    fn session_state(&self) -> RuntimeFuture<'_, colui_domain::RuntimeSessionState> {
        let state = self.state.lock().unwrap().clone();
        Box::pin(async move { Ok(state) })
    }
}

impl BlockingSource {
    fn new(observations: Vec<ContainerObservation>) -> Self {
        Self {
            observations,
            calls: AtomicUsize::new(0),
            started: tokio::sync::Notify::new(),
            release: tokio::sync::Semaphore::new(0),
            context: session_context(),
        }
    }
}

impl DockerApi for BlockingSource {
    fn list_containers(&self) -> RuntimeFuture<'_, Vec<ContainerObservation>> {
        self.calls.fetch_add(1, Ordering::AcqRel);
        Box::pin(async move {
            self.started.notify_one();
            self.release.acquire().await.unwrap().forget();
            Ok(self.observations.clone())
        })
    }

    fn inspect_container(&self, _: &ContainerId) -> RuntimeFuture<'_, ContainerDetails> {
        panic!("inventory refresh must not inspect")
    }
}

impl colui_app::RuntimeStateReader for BlockingSource {
    fn session_state(&self) -> RuntimeFuture<'_, colui_domain::RuntimeSessionState> {
        let context = self.context.clone();
        Box::pin(async move { Ok(colui_domain::RuntimeSessionState::Ready(context)) })
    }
}

impl QueuedSource {
    fn new(values: Vec<Result<Vec<ContainerObservation>, AppError>>) -> Self {
        Self {
            values: std::sync::Mutex::new(values.into()),
            calls: AtomicUsize::new(0),
            context: session_context(),
        }
    }
}

fn session_context() -> colui_domain::SessionContext {
    colui_domain::SessionContext {
        session_id: colui_domain::RuntimeSessionId::new(uuid::Uuid::from_u128(1)),
        endpoint: colui_domain::DockerEndpoint::try_from("unix:///tmp/docker.sock").unwrap(),
        daemon_fingerprint: DaemonFingerprint::new("same", "1", "linux", "x86_64"),
        connected_at: colui_domain::Timestamp("now".to_owned()),
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
