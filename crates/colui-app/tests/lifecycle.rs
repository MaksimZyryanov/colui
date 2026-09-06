use colui_app::{
    ApplyProject, InventoryFuture, InventoryReader, InventoryRefresher, LifecycleFuture,
    LifecycleOperation, LifecycleOperationGuard, LifecycleResult, LifecycleRuntime,
    OperationFuture, OperationKind, OperationLockManager, OperationLockReader, ProfileReader,
    RegistrySnapshot, StopProject, TearDownProject,
};
use colui_domain::{
    AppError, AppErrorCode, ComposeProjectName, DisplayName, ProfileDraft, ProfileId,
    ProjectProfile, RegistrationOrigin, RuntimeInventory,
};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use uuid::Uuid;

struct FakeReader {
    snapshot: RegistrySnapshot,
}

impl FakeReader {
    fn with_profile(profile: ProjectProfile) -> Self {
        Self {
            snapshot: RegistrySnapshot {
                registry_revision: 1,
                profiles: vec![profile],
            },
        }
    }
}

impl ProfileReader for FakeReader {
    fn load(&self) -> colui_app::StoreFuture<'_, RegistrySnapshot> {
        let snapshot = self.snapshot.clone();
        Box::pin(async move { Ok(snapshot) })
    }
}

struct FakeRuntime {
    operations: Arc<Mutex<Vec<LifecycleOperation>>>,
    error: Option<AppError>,
    events: Arc<Mutex<Vec<&'static str>>>,
}

impl FakeRuntime {
    fn ready() -> Self {
        Self {
            operations: Arc::new(Mutex::new(Vec::new())),
            error: None,
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn context_mismatch() -> Self {
        Self {
            operations: Arc::new(Mutex::new(Vec::new())),
            error: Some(AppError::new(
                AppErrorCode::RuntimeContextMismatch,
                "lifecycle",
                None,
                "runtime context is not verified",
            )),
            events: Arc::new(Mutex::new(Vec::new())),
        }
    }

    fn operations(&self) -> Vec<LifecycleOperation> {
        self.operations.lock().unwrap().clone()
    }

    fn invocations(&self) -> Vec<LifecycleInvocation> {
        self.operations()
            .into_iter()
            .map(|operation| LifecycleInvocation { operation })
            .collect()
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct LifecycleInvocation {
    operation: LifecycleOperation,
}

impl LifecycleRuntime for FakeRuntime {
    fn run_profile(
        &self,
        profile: ProjectProfile,
        operation: LifecycleOperation,
    ) -> LifecycleFuture<'_, LifecycleResult> {
        let operations = self.operations.clone();
        let error = self.error.clone();
        let events = self.events.clone();
        Box::pin(async move {
            if let Some(error) = error {
                return Err(error);
            }
            operations.lock().unwrap().push(operation);
            events.lock().unwrap().push("compose");
            Ok(LifecycleResult {
                profile_id: profile.id,
                success: true,
                inventory: RuntimeInventory::unavailable(),
            })
        })
    }
}

struct FakeLocks {
    busy: Arc<Mutex<bool>>,
    events: Arc<Mutex<Vec<&'static str>>>,
}

impl OperationLockReader for FakeLocks {
    fn is_busy(&self, _: &ProfileId) -> bool {
        *self.busy.lock().unwrap()
    }
}

impl OperationLockManager for FakeLocks {
    fn acquire_lifecycle(
        &self,
        _: ProfileId,
        _: OperationKind,
    ) -> OperationFuture<'_, LifecycleOperationGuard> {
        let busy = self.busy.clone();
        let events = self.events.clone();
        Box::pin(async move {
            if *busy.lock().unwrap() {
                return Err(AppError::new(
                    AppErrorCode::OperationConflict,
                    "acquire_lifecycle",
                    None,
                    "busy",
                ));
            }
            *busy.lock().unwrap() = true;
            events.lock().unwrap().push("lock");
            Ok(LifecycleOperationGuard::new(move || {
                *busy.lock().unwrap() = false;
                events.lock().unwrap().push("unlock");
            }))
        })
    }

    fn acquire_definition(
        &self,
        _: ProfileId,
    ) -> Result<colui_app::DefinitionLoadGuard, colui_app::DefinitionBusy> {
        Err(colui_app::DefinitionBusy::LifecyclePending)
    }
}

struct FakeInventory {
    current: RuntimeInventory,
    refresh_result: Result<RuntimeInventory, AppError>,
    calls: Arc<Mutex<u32>>,
    events: Arc<Mutex<Vec<&'static str>>>,
}

impl InventoryReader for FakeInventory {
    fn current_inventory(&self) -> InventoryFuture<'_, RuntimeInventory> {
        let current = self.current.clone();
        Box::pin(async move { Ok(current) })
    }
}

impl InventoryRefresher for FakeInventory {
    fn observation_marker(&self) -> colui_app::ObservationOrder {
        panic!("lifecycle uses ordinary refresh")
    }
    fn refresh_after(
        &self,
        _: colui_app::ObservationOrder,
    ) -> colui_app::InventoryFuture<'_, colui_domain::RuntimeInventory> {
        panic!("lifecycle uses ordinary refresh")
    }
    fn refresh(&self) -> InventoryFuture<'_, RuntimeInventory> {
        *self.calls.lock().unwrap() += 1;
        self.events.lock().unwrap().push("refresh");
        let result = self.refresh_result.clone();
        Box::pin(async move { result })
    }
}

fn fixture() -> (FakeReader, FakeRuntime, FakeLocks, FakeInventory) {
    let events = Arc::new(Mutex::new(Vec::new()));
    let calls = Arc::new(Mutex::new(0));
    (
        FakeReader::with_profile(profile("id-1")),
        FakeRuntime {
            operations: Arc::new(Mutex::new(Vec::new())),
            error: None,
            events: events.clone(),
        },
        FakeLocks {
            busy: Arc::new(Mutex::new(false)),
            events: events.clone(),
        },
        FakeInventory {
            current: RuntimeInventory::unavailable(),
            refresh_result: Ok(RuntimeInventory {
                generation: 1,
                has_snapshot: true,
                ..RuntimeInventory::unavailable()
            }),
            calls,
            events,
        },
    )
}

fn profile_id(value: &str) -> ProfileId {
    let uuid = match value {
        "id-1" => Uuid::from_u128(1),
        "missing" => Uuid::from_u128(2),
        _ => Uuid::from_u128(3),
    };
    ProfileId::new(uuid)
}

fn profile(value: &str) -> ProjectProfile {
    ProjectProfile::from_draft(
        profile_id(value),
        ProfileDraft {
            display_name: DisplayName::try_from("Demo").unwrap(),
            compose_project_name: ComposeProjectName::try_from("demo").unwrap(),
            working_directory: PathBuf::from("/tmp/demo"),
            compose_files: vec![PathBuf::from("compose.yml")],
            environment_files: vec![],
            registration_origin: RegistrationOrigin::Manual,
        },
    )
    .unwrap()
}

#[tokio::test]
async fn apply_looks_up_profile_and_uses_up_without_caller_paths() {
    let _reader = FakeReader::with_profile(profile("id-1"));
    let (reader, runtime, locks, inventory) = fixture();
    let result = ApplyProject::new_with_dependencies(&reader, &runtime, &locks, &inventory)
        .execute(profile_id("id-1"))
        .await
        .unwrap();
    assert_eq!(result.profile_id, profile_id("id-1"));
    assert_eq!(
        runtime.invocations()[0].operation,
        LifecycleOperation::Apply
    );
}

#[tokio::test]
async fn stop_and_tear_down_are_distinct() {
    let (reader, runtime, locks, inventory) = fixture();
    StopProject::new_with_dependencies(&reader, &runtime, &locks, &inventory)
        .execute(profile_id("id-1"))
        .await
        .unwrap();
    TearDownProject::new_with_dependencies(&reader, &runtime, &locks, &inventory)
        .execute(profile_id("id-1"))
        .await
        .unwrap();
    assert_eq!(
        runtime.operations(),
        vec![LifecycleOperation::Stop, LifecycleOperation::TearDown]
    );
}

#[tokio::test]
async fn mismatch_blocks_lifecycle_before_runner_call() {
    let reader = FakeReader::with_profile(profile("id-1"));
    let runtime = FakeRuntime::context_mismatch();
    let locks = FakeLocks {
        busy: Arc::new(Mutex::new(false)),
        events: runtime.events.clone(),
    };
    let inventory = FakeInventory {
        current: RuntimeInventory::unavailable(),
        refresh_result: Ok(RuntimeInventory::unavailable()),
        calls: Arc::new(Mutex::new(0)),
        events: runtime.events.clone(),
    };
    let error = ApplyProject::new_with_dependencies(&reader, &runtime, &locks, &inventory)
        .execute(profile_id("id-1"))
        .await
        .unwrap_err();
    assert_eq!(error.code, AppErrorCode::RuntimeContextMismatch);
}

#[tokio::test]
async fn missing_profile_returns_not_found_before_runtime_call() {
    let reader = FakeReader::with_profile(profile("id-1"));
    let runtime = FakeRuntime::ready();
    let locks = FakeLocks {
        busy: Arc::new(Mutex::new(false)),
        events: runtime.events.clone(),
    };
    let inventory = FakeInventory {
        current: RuntimeInventory::unavailable(),
        refresh_result: Ok(RuntimeInventory::unavailable()),
        calls: Arc::new(Mutex::new(0)),
        events: runtime.events.clone(),
    };
    let error = ApplyProject::new_with_dependencies(&reader, &runtime, &locks, &inventory)
        .execute(profile_id("missing"))
        .await
        .unwrap_err();
    assert_eq!(error.code, AppErrorCode::ProfileNotFound);
    assert!(runtime.operations().is_empty());
}

#[tokio::test]
async fn successful_lifecycle_refreshes_once_before_unlock() {
    let (reader, runtime, locks, inventory) = fixture();
    let calls = inventory.calls.clone();
    let result = ApplyProject::new_with_dependencies(&reader, &runtime, &locks, &inventory)
        .execute(profile_id("id-1"))
        .await
        .unwrap();
    assert!(result.success);
    assert_eq!(result.inventory.generation, 1);
    assert_eq!(*calls.lock().unwrap(), 1);
    assert_eq!(
        *runtime.events.lock().unwrap(),
        vec!["lock", "compose", "refresh", "unlock"]
    );
}

#[tokio::test]
async fn observation_failure_keeps_compose_success_and_retained_inventory() {
    let (reader, runtime, locks, mut inventory) = fixture();
    inventory.current = RuntimeInventory {
        generation: 4,
        has_snapshot: true,
        ..RuntimeInventory::unavailable()
    };
    inventory.refresh_result = Err(AppError::new(
        AppErrorCode::RuntimeUnavailable,
        "inventory",
        None,
        "unavailable",
    ));
    let result = ApplyProject::new_with_dependencies(&reader, &runtime, &locks, &inventory)
        .execute(profile_id("id-1"))
        .await
        .unwrap();
    assert!(result.success);
    assert_eq!(result.inventory.generation, 4);
    assert_eq!(
        result.inventory.freshness,
        colui_domain::InventoryFreshness::Stale
    );
    assert_eq!(
        result.inventory.error.unwrap().code,
        AppErrorCode::RuntimeUnavailable
    );
    assert!(!locks.is_busy(&profile_id("id-1")));
}

#[tokio::test]
async fn duplicate_lifecycle_conflicts_without_compose_or_refresh() {
    let (reader, runtime, locks, inventory) = fixture();
    let guard = locks
        .acquire_lifecycle(profile_id("id-1"), OperationKind::Apply)
        .await
        .unwrap();
    let error = ApplyProject::new_with_dependencies(&reader, &runtime, &locks, &inventory)
        .execute(profile_id("id-1"))
        .await
        .unwrap_err();
    assert_eq!(error.code, AppErrorCode::OperationConflict);
    assert!(runtime.operations().is_empty());
    assert_eq!(*inventory.calls.lock().unwrap(), 0);
    drop(guard);
}

#[tokio::test]
async fn lifecycle_runtime_failure_releases_guard() {
    let reader = FakeReader::with_profile(profile("id-1"));
    let runtime = FakeRuntime::context_mismatch();
    let locks = FakeLocks {
        busy: Arc::new(Mutex::new(false)),
        events: runtime.events.clone(),
    };
    let inventory = FakeInventory {
        current: RuntimeInventory::unavailable(),
        refresh_result: Ok(RuntimeInventory::unavailable()),
        calls: Arc::new(Mutex::new(0)),
        events: runtime.events.clone(),
    };
    assert!(
        ApplyProject::new_with_dependencies(&reader, &runtime, &locks, &inventory)
            .execute(profile_id("id-1"))
            .await
            .is_err()
    );
    assert!(!locks.is_busy(&profile_id("id-1")));
}

struct RestoredReader {
    profile: Arc<Mutex<ProjectProfile>>,
}

impl ProfileReader for RestoredReader {
    fn load(&self) -> colui_app::StoreFuture<'_, RegistrySnapshot> {
        let profile = self.profile.lock().unwrap().clone();
        Box::pin(async move {
            Ok(RegistrySnapshot {
                registry_revision: profile.revision.value(),
                profiles: vec![profile],
            })
        })
    }
}

struct RestoreOnLeaseLocks {
    restored: ProjectProfile,
    current: Arc<Mutex<ProjectProfile>>,
}

impl OperationLockReader for RestoreOnLeaseLocks {
    fn is_busy(&self, _: &ProfileId) -> bool {
        false
    }
}

impl OperationLockManager for RestoreOnLeaseLocks {
    fn acquire_lifecycle(
        &self,
        _: ProfileId,
        _: OperationKind,
    ) -> OperationFuture<'_, LifecycleOperationGuard> {
        *self.current.lock().unwrap() = self.restored.clone();
        Box::pin(async { Ok(LifecycleOperationGuard::new(|| {})) })
    }

    fn acquire_definition(
        &self,
        _: ProfileId,
    ) -> Result<colui_app::DefinitionLoadGuard, colui_app::DefinitionBusy> {
        unreachable!()
    }
}

struct ProfileRecordingRuntime(Arc<Mutex<Vec<PathBuf>>>);

impl LifecycleRuntime for ProfileRecordingRuntime {
    fn run_profile(
        &self,
        profile: ProjectProfile,
        _: LifecycleOperation,
    ) -> LifecycleFuture<'_, LifecycleResult> {
        self.0
            .lock()
            .unwrap()
            .push(profile.working_directory.clone());
        Box::pin(async move {
            Ok(LifecycleResult {
                profile_id: profile.id,
                success: false,
                inventory: RuntimeInventory::unavailable(),
            })
        })
    }
}

#[tokio::test]
async fn restore_between_request_and_lease_uses_profile_reread_under_lease() {
    let old = profile("id-1");
    let mut restored = old.clone();
    restored.working_directory = PathBuf::from("/tmp/restored");
    restored.revision = restored.revision.next().unwrap();
    let current = Arc::new(Mutex::new(old));
    let reader = RestoredReader {
        profile: current.clone(),
    };
    let locks = RestoreOnLeaseLocks { restored, current };
    let paths = Arc::new(Mutex::new(Vec::new()));
    let runtime = ProfileRecordingRuntime(paths.clone());
    let inventory = fixture().3;

    ApplyProject::new_with_dependencies(&reader, &runtime, &locks, &inventory)
        .execute(profile_id("id-1"))
        .await
        .unwrap();

    assert_eq!(*paths.lock().unwrap(), vec![PathBuf::from("/tmp/restored")]);
}
