use colui_app::*;
use colui_domain::*;

#[test]
fn logs_ring_retains_newest_payload_bytes_and_decodes_only_at_end() {
    for size in [0_usize, 262_144, 262_145, 900_000] {
        let bytes: Vec<u8> = (0..size).map(|n| b'a' + (n % 26) as u8).collect();
        let mut ring = LogByteRing::default();
        for chunk in bytes.chunks(777) {
            ring.push(chunk);
        }
        let logs = ring.finish(ContainerId("abc".into()), Timestamp("now".into()));
        assert_eq!(logs.retained_bytes as usize, size.min(262_144));
        assert_eq!(logs.truncated, size > 262_144);
        assert_eq!(logs.text.as_bytes(), &bytes[size.saturating_sub(262_144)..]);
    }
    let mut ring = LogByteRing::default();
    ring.push(&[0xe2]);
    ring.push(&[0x82, 0xac]);
    ring.push(&vec![b'x'; 262_140]);
    ring.push(&[0xf0, 0x9f]);
    let logs = ring.finish(ContainerId("abc".into()), Timestamp("now".into()));
    assert_eq!(logs.retained_bytes, 262_144);
    assert!(logs.truncated);
    assert!(logs.text.starts_with("\u{fffd}\u{fffd}x"));
    assert!(logs.text.ends_with("x\u{fffd}"));
}

fn binding(host: Option<&str>, port: Option<u16>, service: u16, protocol: &str) -> PortBinding {
    PortBinding {
        host_ip: host.map(str::to_owned),
        host_port: port,
        container_port: service,
        protocol: protocol.into(),
    }
}

#[test]
fn ports_exact_display_copy_and_safe_url_table() {
    for (host, port, display, authority) in [
        (
            Some("0.0.0.0"),
            Some(8080),
            "0.0.0.0:8080",
            Some("127.0.0.1:8080"),
        ),
        (Some("::"), Some(8080), "[::]:8080", Some("[::1]:8080")),
        (
            Some("2001:db8::1"),
            Some(8080),
            "[2001:db8::1]:8080",
            Some("[2001:db8::1]:8080"),
        ),
        (
            Some("127.0.0.2"),
            Some(8080),
            "127.0.0.2:8080",
            Some("127.0.0.2:8080"),
        ),
        (Some("::"), None, "[::]:?", None),
        (None, Some(8080), "?:8080", None),
        (None, None, "<unpublished>", None),
    ] {
        let b = binding(host, port, 443, "TCP");
        assert_eq!(binding_display(&b), format!("{display} -> 443/tcp"));
        assert_eq!(binding_url(&b), authority.map(|a| format!("https://{a}")));
        let action = PortBindingAction::from(&b);
        assert_eq!(action.copy, binding_display(&b));
        assert_eq!(action.url, binding_url(&b));
    }
    for service in [80, 443, 3000, 5173, 8000, 8080, 8443] {
        for host_port in [80, 443, 12345] {
            let scheme = if [443, 8443].contains(&service) {
                "https"
            } else {
                "http"
            };
            assert_eq!(
                binding_url(&binding(Some("127.0.0.1"), Some(host_port), service, "tcp")),
                Some(format!("{scheme}://127.0.0.1:{host_port}"))
            );
        }
        for protocol in ["udp", "sctp", "", "tcp "] {
            assert!(
                binding_url(&binding(Some("127.0.0.1"), Some(service), 80, protocol)).is_none()
            );
        }
        let scheme = if [443, 8443].contains(&service) {
            "https"
        } else {
            "http"
        };
        assert_eq!(
            binding_url(&binding(Some("127.0.0.1"), Some(service), 9999, "tcp")),
            Some(format!("{scheme}://127.0.0.1:{service}"))
        );
    }
    for host in [
        "",
        "localhost",
        "https://evil",
        "127.0.0.1@evil",
        "127.0.0.1/path",
        "[::1]",
        "::1%eth0",
        "127.0.0.1\n",
    ] {
        assert!(
            binding_url(&binding(Some(host), Some(80), 80, "tcp")).is_none(),
            "{host}"
        );
    }
    assert!(binding_url(&binding(Some("127.0.0.1"), Some(9999), 9999, "tcp")).is_none());
    assert!(binding_url(&binding(Some("127.0.0.1"), Some(0), 80, "tcp")).is_none());
}

struct ReadFixture {
    inner: Fixture,
    opened: Mutex<Vec<String>>,
    reads: AtomicUsize,
}
impl ReadFixture {
    fn new() -> Self {
        let inner = Fixture::new();
        inner.inventory.lock().unwrap().standalone_containers[0].published_ports =
            vec![binding(Some("::"), Some(18080), 80, "tcp")];
        Self {
            inner,
            opened: Mutex::new(vec![]),
            reads: AtomicUsize::new(0),
        }
    }
}
impl RuntimeStateReader for ReadFixture {
    fn session_state(&self) -> RuntimeFuture<'_, RuntimeSessionState> {
        self.inner.session_state()
    }
}
impl InventoryReader for ReadFixture {
    fn current_inventory(&self) -> InventoryFuture<'_, RuntimeInventory> {
        Box::pin(async {
            self.reads.fetch_add(1, Ordering::SeqCst);
            if self.inner.change_on_read.load(Ordering::SeqCst) {
                *self.inner.state.lock().unwrap() = session(2);
            }
            Ok(self.inner.inventory.lock().unwrap().clone())
        })
    }
}
impl BrowserOpener for ReadFixture {
    fn open(&self, url: &str) -> Result<(), AppError> {
        self.opened.lock().unwrap().push(url.into());
        Ok(())
    }
}
impl ContainerLogsRuntime for ReadFixture {
    fn read_logs(&self, request: ContainerLogsRequest) -> RuntimeFuture<'_, ContainerLogs> {
        Box::pin(async move {
            self.inner.actions.fetch_add(1, Ordering::SeqCst);
            if self.inner.change_on_action.load(Ordering::SeqCst) {
                *self.inner.state.lock().unwrap() = session(2);
            }
            if self.inner.fail_action.load(Ordering::SeqCst) {
                return Err(container_operation_error(&request.container_id, "SECRET"));
            }
            let mut ring = LogByteRing::default();
            ring.push(b"SECRET");
            Ok(ring.finish(request.container_id, Timestamp("now".into())))
        })
    }
}
#[tokio::test]
async fn ports_browser_rereads_inventory_and_rejects_untrusted_inputs() {
    let f = ReadFixture::new();
    let open = OpenContainerPort::new(&f, &f, &f);
    open.execute(ContainerId("abc".into()), session(1), 0)
        .await
        .unwrap();
    assert_eq!(*f.opened.lock().unwrap(), vec!["http://[::1]:18080"]);
    f.inner.inventory.lock().unwrap().standalone_containers[0].published_ports[0].host_port =
        Some(8081);
    open.execute(ContainerId("abc".into()), session(1), 0)
        .await
        .unwrap();
    assert_eq!(f.opened.lock().unwrap()[1], "http://[::1]:8081");
    for (id, s, index) in [
        ("https://evil", session(1), 0),
        ("abc", session(2), 0),
        ("abc", session(1), usize::MAX),
    ] {
        let error = open
            .execute(ContainerId(id.into()), s, index)
            .await
            .unwrap_err();
        assert_eq!(error.code, AppErrorCode::ContainerOperationFailed);
        assert!(!error.retryable);
    }
    f.inner.inventory.lock().unwrap().standalone_containers[0].published_ports[0].protocol =
        "udp".into();
    assert!(open
        .execute(ContainerId("abc".into()), session(1), 0)
        .await
        .is_err());
    assert_eq!(f.opened.lock().unwrap().len(), 2);
}
#[tokio::test]
async fn logs_and_ports_reject_stale_missing_or_changed_inventory_without_side_effects() {
    for case in [
        "old_session",
        "missing",
        "stale",
        "no_snapshot",
        "changed_on_read",
    ] {
        let f = ReadFixture::new();
        {
            let mut i = f.inner.inventory.lock().unwrap();
            match case {
                "old_session" => i.runtime_session_id = Some(session(2)),
                "missing" => i.standalone_containers.clear(),
                "stale" => i.freshness = InventoryFreshness::Stale,
                "no_snapshot" => i.has_snapshot = false,
                _ => f.inner.change_on_read.store(true, Ordering::SeqCst),
            }
        }
        assert!(
            ReadContainerLogs::new(&f, &f)
                .execute(ContainerLogsRequest {
                    container_id: ContainerId("abc".into()),
                    runtime_session_id: session(1)
                })
                .await
                .is_err(),
            "{case}"
        );
        assert!(
            OpenContainerPort::new(&f, &f, &f)
                .execute(ContainerId("abc".into()), session(1), 0)
                .await
                .is_err(),
            "{case}"
        );
        assert_eq!(f.inner.actions.load(Ordering::SeqCst), 0);
        assert!(f.opened.lock().unwrap().is_empty());
    }
}
#[tokio::test]
async fn logs_session_guards_and_failures_never_return_or_retain_text() {
    let f = ReadFixture::new();
    let before = f.inner.inventory.lock().unwrap().clone();
    let request = ContainerLogsRequest {
        container_id: ContainerId("abc".into()),
        runtime_session_id: session(1),
    };
    let read = ReadContainerLogs::new(&f, &f);
    let logs = read.execute(request.clone()).await.unwrap();
    assert_eq!(logs.text, "SECRET");
    assert!(!format!("{logs:?}").contains("SECRET"));
    drop(logs);
    f.inner.fail_action.store(true, Ordering::SeqCst);
    let error = read.execute(request.clone()).await.unwrap_err();
    assert!(!format!("{error:?}").contains("SECRET"));
    f.inner.fail_action.store(false, Ordering::SeqCst);
    f.inner.change_on_action.store(true, Ordering::SeqCst);
    let error = read.execute(request.clone()).await.unwrap_err();
    assert!(!error.retryable);
    assert!(!format!("{error:?}").contains("SECRET"));
    let calls = f.inner.actions.load(Ordering::SeqCst);
    assert!(read.execute(request).await.is_err());
    assert_eq!(f.inner.actions.load(Ordering::SeqCst), calls);
    assert_eq!(*f.inner.inventory.lock().unwrap(), before);
    assert_eq!(f.inner.refreshes.load(Ordering::SeqCst), 0);
    assert_eq!(f.inner.acquisitions.load(Ordering::SeqCst), 0);
}
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};

fn session(n: u128) -> RuntimeSessionId {
    RuntimeSessionId::new(uuid::Uuid::from_u128(n))
}

struct Fixture {
    state: Mutex<RuntimeSessionId>,
    inventory: Mutex<RuntimeInventory>,
    held: Arc<AtomicBool>,
    acquisitions: AtomicUsize,
    actions: AtomicUsize,
    refreshes: AtomicUsize,
    markers: AtomicUsize,
    change_on_lease: AtomicBool,
    change_on_read: AtomicBool,
    change_on_action: AtomicBool,
    fail_action: AtomicBool,
    fail_refresh: AtomicBool,
}

impl Fixture {
    fn new() -> Self {
        let mut inventory = RuntimeInventory::unavailable();
        inventory.has_snapshot = true;
        inventory.generation = 7;
        inventory.freshness = InventoryFreshness::Fresh;
        inventory.runtime_session_id = Some(session(1));
        let container = ContainerInstance {
            id: ContainerId("abc".into()),
            name: "standalone".into(),
            image: "alpine".into(),
            state: ContainerState::Stopped,
            status_text: "exited".into(),
            service_name: None,
            published_ports: vec![],
        };
        inventory.containers.push(container.clone());
        inventory.standalone_containers.push(container);
        Self {
            state: Mutex::new(session(1)),
            inventory: Mutex::new(inventory),
            held: Arc::new(AtomicBool::new(false)),
            acquisitions: AtomicUsize::new(0),
            actions: AtomicUsize::new(0),
            refreshes: AtomicUsize::new(0),
            markers: AtomicUsize::new(0),
            change_on_lease: AtomicBool::new(false),
            change_on_read: AtomicBool::new(false),
            change_on_action: AtomicBool::new(false),
            fail_action: AtomicBool::new(false),
            fail_refresh: AtomicBool::new(false),
        }
    }
    async fn execute(&self, expected: RuntimeSessionId) -> Result<ContainerActionResult, AppError> {
        RunContainerAction::new(self, self, self)
            .execute(ContainerId("abc".into()), ContainerAction::Start, expected)
            .await
    }
}

impl RuntimeStateReader for Fixture {
    fn session_state(&self) -> RuntimeFuture<'_, RuntimeSessionState> {
        Box::pin(async {
            Ok(RuntimeSessionState::Ready(SessionContext {
                session_id: *self.state.lock().unwrap(),
                endpoint: DockerEndpoint::try_from("unix:///tmp/docker.sock").unwrap(),
                daemon_fingerprint: DaemonFingerprint::new("same", "1", "linux", "x86_64"),
                connected_at: Timestamp("now".into()),
            }))
        })
    }
}
impl ContainerRuntime for Fixture {
    fn run_container(
        &self,
        id: ContainerId,
        action: ContainerAction,
        expected: RuntimeSessionId,
    ) -> RuntimeFuture<'_, ()> {
        Box::pin(async move {
            assert!(self.held.load(Ordering::SeqCst));
            assert_eq!(id.0, "abc");
            assert_eq!(action, ContainerAction::Start);
            assert_eq!(expected, *self.state.lock().unwrap());
            self.actions.fetch_add(1, Ordering::SeqCst);
            if self.fail_action.load(Ordering::SeqCst) {
                return Err(AppError::new(
                    AppErrorCode::RuntimeUnavailable,
                    "daemon",
                    None,
                    "SECRET",
                ));
            }
            if self.change_on_action.load(Ordering::SeqCst) {
                *self.state.lock().unwrap() = session(2);
            }
            Ok(())
        })
    }
}
impl InventoryReader for Fixture {
    fn current_inventory(&self) -> InventoryFuture<'_, RuntimeInventory> {
        Box::pin(async {
            assert!(self.held.load(Ordering::SeqCst));
            if self.change_on_read.load(Ordering::SeqCst) {
                *self.state.lock().unwrap() = session(2);
            }
            Ok(self.inventory.lock().unwrap().clone())
        })
    }
}
impl InventoryRefresher for Fixture {
    fn refresh(&self) -> InventoryFuture<'_, RuntimeInventory> {
        panic!("must use causal refresh")
    }
    fn observation_marker(&self) -> ObservationOrder {
        assert!(self.held.load(Ordering::SeqCst));
        assert_eq!(self.actions.load(Ordering::SeqCst), 1);
        assert_eq!(self.markers.fetch_add(1, Ordering::SeqCst), 0);
        ObservationOrder(42)
    }
    fn refresh_after(&self, marker: ObservationOrder) -> InventoryFuture<'_, RuntimeInventory> {
        Box::pin(async move {
            assert!(self.held.load(Ordering::SeqCst));
            assert_eq!(marker, ObservationOrder(42));
            self.refreshes.fetch_add(1, Ordering::SeqCst);
            let mut inventory = self.inventory.lock().unwrap();
            if self.fail_refresh.load(Ordering::SeqCst) {
                let error = AppError::new(
                    AppErrorCode::RuntimeUnavailable,
                    "inventory",
                    None,
                    "offline",
                );
                inventory.freshness = InventoryFreshness::Stale;
                inventory.error = Some(error.clone());
                Err(error)
            } else {
                inventory.generation = 8;
                Ok(inventory.clone())
            }
        })
    }
}
impl OperationLockReader for Fixture {
    fn is_busy(&self, _: &ProfileId) -> bool {
        false
    }
}
impl OperationLockManager for Fixture {
    fn acquire_lifecycle(
        &self,
        _: ProfileId,
        _: OperationKind,
    ) -> OperationFuture<'_, LifecycleOperationGuard> {
        panic!("not profile action")
    }
    fn acquire_definition(&self, _: ProfileId) -> Result<DefinitionLoadGuard, DefinitionBusy> {
        panic!("not definition")
    }
    fn acquire_container(&self, id: &str) -> Result<ContainerOperationGuard, AppError> {
        assert_eq!(id, "abc");
        self.acquisitions.fetch_add(1, Ordering::SeqCst);
        assert!(!self.held.swap(true, Ordering::SeqCst));
        if self.change_on_lease.load(Ordering::SeqCst) {
            *self.state.lock().unwrap() = session(2);
        }
        let held = self.held.clone();
        Ok(ContainerOperationGuard::new(move || {
            held.store(false, Ordering::SeqCst);
        }))
    }
}

fn assert_rejected(error: AppError, fixture: &Fixture) {
    assert_eq!(error.code, AppErrorCode::ContainerOperationFailed);
    assert_eq!(
        error.subject.as_ref().unwrap().kind,
        AppErrorSubjectKind::Container
    );
    assert_eq!(error.subject.as_ref().unwrap().id, "abc");
    assert!(!error.retryable);
    assert!(!format!("{error:?}").contains("SECRET"));
    assert_eq!(fixture.refreshes.load(Ordering::SeqCst), 0);
    assert_eq!(fixture.markers.load(Ordering::SeqCst), 0);
    assert!(!fixture.held.load(Ordering::SeqCst));
}

#[tokio::test]
async fn expected_session_mismatch_rejects_before_lease() {
    let f = Fixture::new();
    assert_rejected(f.execute(session(2)).await.unwrap_err(), &f);
    assert_eq!(f.acquisitions.load(Ordering::SeqCst), 0);
    assert_eq!(f.actions.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn fresh_container_membership_and_matching_inventory_session_required_under_lease() {
    for case in [
        "missing",
        "stale",
        "unavailable",
        "old_session",
        "no_snapshot",
    ] {
        let f = Fixture::new();
        {
            let mut i = f.inventory.lock().unwrap();
            match case {
                "missing" => {
                    i.containers.clear();
                    i.standalone_containers.clear();
                }
                "stale" => i.freshness = InventoryFreshness::Stale,
                "unavailable" => i.freshness = InventoryFreshness::Unavailable,
                "old_session" => i.runtime_session_id = Some(session(2)),
                "no_snapshot" => i.has_snapshot = false,
                _ => unreachable!(),
            }
        }
        assert_rejected(f.execute(session(1)).await.unwrap_err(), &f);
        assert_eq!(f.acquisitions.load(Ordering::SeqCst), 1, "{case}");
        assert_eq!(f.actions.load(Ordering::SeqCst), 0, "{case}");
    }
}

#[tokio::test]
async fn fresh_compose_container_can_be_controlled() {
    let f = Fixture::new();
    f.inventory.lock().unwrap().standalone_containers.clear();

    let result = f.execute(session(1)).await.unwrap();

    assert_eq!(result.container_id.0, "abc");
    assert_eq!(f.actions.load(Ordering::SeqCst), 1);
    assert_eq!(f.refreshes.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn session_rechecked_after_lease_and_inventory_read() {
    for during_read in [false, true] {
        let f = Fixture::new();
        if during_read {
            f.change_on_read.store(true, Ordering::SeqCst);
        } else {
            f.change_on_lease.store(true, Ordering::SeqCst);
        }
        assert_rejected(f.execute(session(1)).await.unwrap_err(), &f);
        assert_eq!(f.actions.load(Ordering::SeqCst), 0);
    }
}

#[tokio::test]
async fn failed_action_is_sanitized_and_never_refreshes() {
    let f = Fixture::new();
    f.fail_action.store(true, Ordering::SeqCst);
    assert_rejected(f.execute(session(1)).await.unwrap_err(), &f);
    assert_eq!(f.actions.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn success_refreshes_exactly_once_and_retains_failure_snapshot_even_after_session_change() {
    for changed in [false, true] {
        for failed_refresh in [false, true] {
            let f = Fixture::new();
            f.change_on_action.store(changed, Ordering::SeqCst);
            f.fail_refresh.store(failed_refresh, Ordering::SeqCst);
            let result = f.execute(session(1)).await.unwrap();
            assert_eq!(result.container_id.0, "abc");
            assert_eq!(result.action, ContainerAction::Start);
            assert_eq!(
                result.observation,
                if changed {
                    ContainerActionObservation::IndeterminateAfterSessionChange
                } else {
                    ContainerActionObservation::ConfirmedInSession
                }
            );
            assert_eq!(
                result.inventory.generation,
                if failed_refresh { 7 } else { 8 }
            );
            assert_eq!(result.inventory.error.is_some(), failed_refresh);
            assert_eq!(
                result.inventory.freshness,
                if failed_refresh {
                    InventoryFreshness::Stale
                } else {
                    InventoryFreshness::Fresh
                }
            );
            assert_eq!(f.refreshes.load(Ordering::SeqCst), 1);
            assert_eq!(f.markers.load(Ordering::SeqCst), 1);
            assert!(!f.held.load(Ordering::SeqCst));
        }
    }
}
