use colui_app::*;
use colui_domain::*;
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
        inventory.standalone_containers.push(ContainerInstance {
            id: ContainerId("abc".into()),
            name: "standalone".into(),
            image: "alpine".into(),
            state: ContainerState::Stopped,
            status_text: "exited".into(),
            service_name: None,
            published_ports: vec![],
        });
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
async fn fresh_standalone_membership_and_matching_inventory_session_required_under_lease() {
    for case in [
        "missing",
        "compose",
        "stale",
        "unavailable",
        "old_session",
        "no_snapshot",
    ] {
        let f = Fixture::new();
        {
            let mut i = f.inventory.lock().unwrap();
            match case {
                "missing" => i.standalone_containers.clear(),
                "compose" => {
                    i.containers = i.standalone_containers.clone();
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
