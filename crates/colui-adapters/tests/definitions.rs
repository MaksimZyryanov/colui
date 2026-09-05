use colui_adapters::definitions::DefinitionCache;
use colui_adapters::runtime::ComposeExecutionGate;
use colui_adapters::OperationLockManager;
use colui_app::{
    project_status_from_inventory_and_definition_projection, Clock, ComposeInvocation,
    ComposeProcessResult, ComposeRunner, DefinitionBusy, DefinitionLoadGuard, DefinitionReader,
    DefinitionRefresher, LifecycleOperationGuard, OperationFuture, OperationKind,
    OperationLockManager as OperationLockManagerPort, RuntimeFuture, RuntimeStateReader,
};
use colui_domain::{
    AppErrorCode, DaemonFingerprint, DefinitionState, DisplayName, DockerEndpoint, ProfileDraft,
    ProfileId, ProjectProfile, RegistrationOrigin, RuntimeSessionId, RuntimeSessionState,
    Timestamp,
};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, Condvar, Mutex,
};
use std::time::Duration;
use tokio::sync::Notify;
use uuid::Uuid;

#[derive(Clone)]
struct TestClock {
    mono: Arc<std::sync::atomic::AtomicU64>,
}
impl Clock for TestClock {
    fn now(&self) -> Timestamp {
        Timestamp("t".into())
    }
    fn monotonic(&self) -> Duration {
        Duration::from_secs(self.mono.load(Ordering::SeqCst))
    }
}

struct Runner {
    output: String,
    calls: AtomicUsize,
}

struct SequenceRunner {
    outputs: Mutex<Vec<ComposeProcessResult>>,
    calls: AtomicUsize,
}
impl SequenceRunner {
    fn new(outputs: Vec<ComposeProcessResult>) -> Arc<Self> {
        Arc::new(Self {
            outputs: Mutex::new(outputs.into_iter().rev().collect()),
            calls: AtomicUsize::new(0),
        })
    }
}
impl ComposeRunner for SequenceRunner {
    fn invoke(&self, _: ComposeInvocation) -> RuntimeFuture<'_, ComposeProcessResult> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let output = self.outputs.lock().unwrap().pop().unwrap();
        Box::pin(async move { Ok(output) })
    }
}

fn success(output: &str) -> ComposeProcessResult {
    ComposeProcessResult::completed(0, output, "", Duration::ZERO)
}

fn failure() -> ComposeProcessResult {
    ComposeProcessResult::completed(
        1,
        "",
        "secret=/Users/max/private.env project=top-secret",
        Duration::ZERO,
    )
}
impl Runner {
    fn new(output: &str) -> Arc<Self> {
        Arc::new(Self {
            output: output.into(),
            calls: AtomicUsize::new(0),
        })
    }
}

impl ComposeRunner for Runner {
    fn invoke(&self, _: ComposeInvocation) -> RuntimeFuture<'_, ComposeProcessResult> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        let output = self.output.clone();
        Box::pin(async move {
            Ok(ComposeProcessResult::completed(
                0,
                output,
                "",
                Duration::ZERO,
            ))
        })
    }
}

#[tokio::test]
async fn changed_profile_revision_does_not_return_old_services_when_busy() {
    let runner = Runner::new(r#"{"services":{"old":{"image":"old"}}}"#);
    let locks = Arc::new(OperationLockManager::new(
        std::env::temp_dir().join(format!("colui-{}.lock", uuid::Uuid::new_v4())),
    ));
    let clock = Arc::new(TestClock {
        mono: Arc::new(0.into()),
    });
    let cache = DefinitionCache::new(
        runner.clone(),
        Arc::new(Runtime),
        clock,
        locks.clone(),
        Arc::new(ComposeExecutionGate::new()),
    );
    let old = profile(1);
    cache.refresh_definition(old.clone()).await.unwrap();
    let guard = locks
        .acquire_lifecycle(old.id.clone(), OperationKind::Apply)
        .await
        .unwrap();
    let newer = profile(2);
    let definition = cache.definition(newer).await.unwrap();
    assert_eq!(definition.definition.state, DefinitionState::Unchecked);
    assert!(definition.definition.services.is_empty());
    assert_eq!(
        definition.error.unwrap().code,
        AppErrorCode::OperationConflict
    );
    drop(guard);
}

#[tokio::test]
async fn first_load_failure_returns_unchecked_definition_and_typed_error() {
    let projection = cache(
        SequenceRunner::new(vec![failure()]),
        Arc::new(TestClock {
            mono: Arc::new(0.into()),
        }),
    )
    .refresh_definition(profile(1))
    .await
    .unwrap();

    assert_eq!(projection.definition.state, DefinitionState::Unchecked);
    assert!(projection.definition.services.is_empty());
    assert_eq!(
        projection.error.unwrap().code,
        AppErrorCode::DefinitionFailed
    );
}

#[tokio::test]
async fn failed_first_definition_load_is_reused_for_project_details_projection() {
    let runner = SequenceRunner::new(vec![failure()]);
    let cache = cache(
        runner.clone(),
        Arc::new(TestClock {
            mono: Arc::new(0.into()),
        }),
    );
    let profile = profile(1);

    let definition = cache.definition(profile.clone()).await.unwrap();
    let status = project_status_from_inventory_and_definition_projection(
        &profile,
        colui_domain::RuntimeInventory::unavailable(),
        &definition,
    );

    assert_eq!(runner.calls.load(Ordering::SeqCst), 1);
    assert_eq!(definition.definition.state, DefinitionState::Unchecked);
    assert_eq!(
        definition.error.as_ref().unwrap().code,
        AppErrorCode::DefinitionFailed
    );
    assert_eq!(status.definition_state, definition.definition.state);
    assert_eq!(status.issues, definition.definition.issues);
}

#[tokio::test]
async fn failed_refresh_retains_services_as_stale_and_success_clears_error() {
    let runner = SequenceRunner::new(vec![
        success(r#"{"services":{"web":{"image":"nginx"}}}"#),
        failure(),
        success(r#"{"services":{"api":{"image":"api"}}}"#),
    ]);
    let cache = cache(
        runner,
        Arc::new(TestClock {
            mono: Arc::new(0.into()),
        }),
    );
    cache.refresh_definition(profile(1)).await.unwrap();

    let failed = cache.refresh_definition(profile(1)).await.unwrap();
    assert_eq!(failed.definition.state, DefinitionState::Stale);
    assert_eq!(failed.definition.services[0].name, "web");
    assert_eq!(failed.error.unwrap().code, AppErrorCode::DefinitionFailed);

    let locks = Arc::new(OperationLockManager::new(
        std::env::temp_dir().join(format!("colui-{}.lock", uuid::Uuid::new_v4())),
    ));
    let busy_runner = SequenceRunner::new(vec![
        success(r#"{"services":{"web":{"image":"nginx"}}}"#),
        failure(),
    ]);
    let busy_cache = DefinitionCache::new(
        busy_runner.clone(),
        Arc::new(Runtime),
        Arc::new(TestClock {
            mono: Arc::new(0.into()),
        }),
        locks.clone(),
        Arc::new(ComposeExecutionGate::new()),
    );
    busy_cache.refresh_definition(profile(1)).await.unwrap();
    busy_cache.refresh_definition(profile(1)).await.unwrap();
    let guard = locks
        .acquire_lifecycle(profile(1).id, OperationKind::Apply)
        .await
        .unwrap();
    let busy = busy_cache.refresh_definition(profile(1)).await.unwrap();
    assert_eq!(busy.definition.state, DefinitionState::Stale);
    assert_eq!(busy.error.unwrap().code, AppErrorCode::OperationConflict);
    assert_eq!(busy_runner.calls.load(Ordering::SeqCst), 2);
    drop(guard);

    let recovered = cache.refresh_definition(profile(1)).await.unwrap();
    assert_eq!(recovered.definition.state, DefinitionState::Valid);
    assert_eq!(recovered.definition.services[0].name, "api");
    assert!(recovered.error.is_none());
}

struct Runtime;
impl RuntimeStateReader for Runtime {
    fn session_state(&self) -> RuntimeFuture<'_, RuntimeSessionState> {
        Box::pin(async {
            Ok(RuntimeSessionState::Ready(colui_domain::SessionContext {
                session_id: RuntimeSessionId::new(Uuid::nil()),
                endpoint: DockerEndpoint::try_from("unix:///var/run/docker.sock").unwrap(),
                daemon_fingerprint: DaemonFingerprint::new("d", "v", "darwin", "arm64"),
                connected_at: Timestamp("now".into()),
            }))
        })
    }
}

fn profile(revision: u64) -> ProjectProfile {
    let draft = ProfileDraft {
        display_name: DisplayName::try_from("Web").unwrap(),
        compose_project_name: "web".try_into().unwrap(),
        working_directory: PathBuf::from("/tmp"),
        compose_files: vec![PathBuf::from("compose.yml")],
        environment_files: vec![],
        registration_origin: RegistrationOrigin::Manual,
    };
    let mut profile = ProjectProfile::from_draft(ProfileId::new(Uuid::nil()), draft).unwrap();
    while profile.revision.value() < revision {
        profile = profile
            .with_display_name(DisplayName::try_from("Web").unwrap())
            .unwrap();
    }
    profile
}

fn cache(runner: Arc<dyn ComposeRunner>, clock: Arc<TestClock>) -> DefinitionCache {
    DefinitionCache::new(
        runner,
        Arc::new(Runtime),
        clock,
        Arc::new(OperationLockManager::new(
            std::env::temp_dir().join(format!("colui-{}.lock", uuid::Uuid::new_v4())),
        )),
        Arc::new(ComposeExecutionGate::new()),
    )
}

#[tokio::test]
async fn matching_revision_within_ttl_avoids_second_config_call() {
    let runner = Runner::new(r#"{"services":{"web":{"image":"nginx"}}}"#);
    let cache = cache(
        runner.clone(),
        Arc::new(TestClock {
            mono: Arc::new(1.into()),
        }),
    );
    let p = profile(1);
    cache.refresh_definition(p.clone()).await.unwrap();
    cache.definition(p).await.unwrap();
    assert_eq!(runner.calls.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn canonical_json_key_order_has_same_revision() {
    let a = Runner::new(r#"{"services":{"web":{"image":"nginx","build":"."}}}"#);
    let b = Runner::new(r#"{"services":{"web":{"build":".","image":"nginx"}}}"#);
    let clock = Arc::new(TestClock {
        mono: Arc::new(0.into()),
    });
    let c1 = cache(a, clock.clone());
    let c2 = cache(b, clock);
    let p = profile(1);
    assert_eq!(
        c1.refresh_definition(p.clone())
            .await
            .unwrap()
            .definition
            .definition_revision,
        c2.refresh_definition(p)
            .await
            .unwrap()
            .definition
            .definition_revision
    );
}

#[tokio::test]
async fn expired_entry_refreshes_and_invalid_definition_is_retained() {
    let runner = Runner::new("not json");
    let clock = Arc::new(TestClock {
        mono: Arc::new(0.into()),
    });
    let cache = cache(runner.clone(), clock.clone());
    let definition = cache.definition(profile(1)).await.unwrap();
    assert_eq!(definition.definition.state, DefinitionState::Invalid);
    clock.mono.store(61, Ordering::SeqCst);
    cache.definition(profile(1)).await.unwrap();
    assert_eq!(runner.calls.load(Ordering::SeqCst), 2);
}

struct BlockingRunner {
    started: Notify,
    release: Notify,
    calls: AtomicUsize,
}
impl BlockingRunner {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            started: Notify::new(),
            release: Notify::new(),
            calls: AtomicUsize::new(0),
        })
    }
}
impl ComposeRunner for BlockingRunner {
    fn invoke(&self, _: ComposeInvocation) -> RuntimeFuture<'_, ComposeProcessResult> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.started.notify_one();
        Box::pin(async move {
            self.release.notified().await;
            Ok(ComposeProcessResult::completed(
                0,
                r#"{"services":{}}"#,
                "",
                Duration::ZERO,
            ))
        })
    }
}

struct RevisionRaceRunner {
    old_started: Notify,
    release_old: Notify,
    calls: AtomicUsize,
}

struct RevisionRaceClock {
    calls: AtomicUsize,
    old_completed: Notify,
    release: (Mutex<bool>, Condvar),
}
impl RevisionRaceClock {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            calls: AtomicUsize::new(0),
            old_completed: Notify::new(),
            release: (Mutex::new(false), Condvar::new()),
        })
    }

    fn release_old(&self) {
        *self.release.0.lock().unwrap() = true;
        self.release.1.notify_one();
    }
}
impl Clock for RevisionRaceClock {
    fn now(&self) -> Timestamp {
        Timestamp("t".into())
    }

    fn monotonic(&self) -> Duration {
        if self.calls.fetch_add(1, Ordering::SeqCst) == 1 {
            self.old_completed.notify_one();
            let released = self.release.0.lock().unwrap();
            drop(
                self.release
                    .1
                    .wait_while(released, |value| !*value)
                    .unwrap(),
            );
        }
        Duration::ZERO
    }
}
impl RevisionRaceRunner {
    fn new() -> Arc<Self> {
        Arc::new(Self {
            old_started: Notify::new(),
            release_old: Notify::new(),
            calls: AtomicUsize::new(0),
        })
    }
}
impl ComposeRunner for RevisionRaceRunner {
    fn invoke(&self, _: ComposeInvocation) -> RuntimeFuture<'_, ComposeProcessResult> {
        let call = self.calls.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            if call == 0 {
                self.old_started.notify_one();
                self.release_old.notified().await;
                return Ok(success(r#"{"services":{"old":{"image":"old"}}}"#));
            }
            Ok(success(r#"{"services":{"new":{"image":"new"}}}"#))
        })
    }
}

struct PermissiveLocks;
impl colui_app::OperationLockReader for PermissiveLocks {
    fn is_busy(&self, _: &ProfileId) -> bool {
        false
    }
}
impl OperationLockManagerPort for PermissiveLocks {
    fn acquire_lifecycle(
        &self,
        _: ProfileId,
        _: OperationKind,
    ) -> OperationFuture<'_, LifecycleOperationGuard> {
        Box::pin(async { Ok(LifecycleOperationGuard::new(|| {})) })
    }

    fn acquire_definition(&self, _: ProfileId) -> Result<DefinitionLoadGuard, DefinitionBusy> {
        Ok(DefinitionLoadGuard::new(|| {}))
    }
}

#[tokio::test]
async fn invalidation_discards_late_load_and_next_access_reloads() {
    let runner = BlockingRunner::new();
    let cache = Arc::new(cache(
        runner.clone(),
        Arc::new(TestClock {
            mono: Arc::new(0.into()),
        }),
    ));
    let task = tokio::spawn({
        let cache = cache.clone();
        async move { cache.refresh_definition(profile(1)).await.unwrap() }
    });
    runner.started.notified().await;
    cache.invalidate(profile(1).id.clone());
    runner.release.notify_one();
    task.await.unwrap();
    let reload = tokio::spawn({
        let cache = cache.clone();
        async move { cache.definition(profile(1)).await.unwrap() }
    });
    runner.started.notified().await;
    runner.release.notify_one();
    reload.await.unwrap();
    assert_eq!(runner.calls.load(Ordering::SeqCst), 2);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn profile_revision_change_during_load_discards_old_result() {
    let runner = RevisionRaceRunner::new();
    let clock = RevisionRaceClock::new();
    let cache = Arc::new(DefinitionCache::new(
        runner.clone(),
        Arc::new(Runtime),
        clock.clone(),
        Arc::new(PermissiveLocks),
        Arc::new(ComposeExecutionGate::new()),
    ));
    let old_load = tokio::spawn({
        let cache = cache.clone();
        async move { cache.refresh_definition(profile(1)).await.unwrap() }
    });
    runner.old_started.notified().await;

    runner.release_old.notify_one();
    clock.old_completed.notified().await;

    let current = cache.definition(profile(2)).await.unwrap();
    assert_eq!(current.definition.services[0].name, "new");
    assert!(current.error.is_none());

    clock.release_old();
    let late = old_load.await.unwrap();
    assert_eq!(late, current);
    assert_eq!(runner.calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn compose_config_invocation_records_exact_boundary_inputs() {
    let invocations = Arc::new(Mutex::new(Vec::new()));
    let runner = Arc::new(RecordingRunner(invocations.clone()));
    cache(
        runner,
        Arc::new(TestClock {
            mono: Arc::new(0.into()),
        }),
    )
    .refresh_definition(profile(1))
    .await
    .unwrap();

    let invocation = &invocations.lock().unwrap()[0];
    assert_eq!(invocation.executable, PathBuf::from("docker"));
    assert_eq!(invocation.working_directory, PathBuf::from("/tmp"));
    assert!(invocation
        .args
        .windows(3)
        .any(|args| args == ["config", "--format", "json"]));
    assert!(invocation.environment.contains_key("DOCKER_HOST"));
}

struct RecordingRunner(Arc<Mutex<Vec<ComposeInvocation>>>);

impl ComposeRunner for RecordingRunner {
    fn invoke(&self, invocation: ComposeInvocation) -> RuntimeFuture<'_, ComposeProcessResult> {
        self.0.lock().unwrap().push(invocation);
        Box::pin(async {
            Ok(ComposeProcessResult::completed(
                0,
                r#"{"services":{}}"#,
                "",
                Duration::ZERO,
            ))
        })
    }
}

#[tokio::test]
async fn lifecycle_busy_returns_unchecked_without_running_compose() {
    let runner = Runner::new(r#"{"services":{}}"#);
    let locks = Arc::new(OperationLockManager::new(
        std::env::temp_dir().join(format!("colui-{}.lock", uuid::Uuid::new_v4())),
    ));
    let guard = locks
        .acquire_lifecycle(profile(1).id.clone(), OperationKind::Apply)
        .await
        .unwrap();
    let cache = DefinitionCache::new(
        runner.clone(),
        Arc::new(Runtime),
        Arc::new(TestClock {
            mono: Arc::new(0.into()),
        }),
        locks,
        Arc::new(ComposeExecutionGate::new()),
    );
    let definition = cache.definition(profile(1)).await.unwrap();
    assert_eq!(definition.definition.state, DefinitionState::Unchecked);
    assert_eq!(
        definition.error.unwrap().code,
        AppErrorCode::OperationConflict
    );
    assert_eq!(runner.calls.load(Ordering::SeqCst), 0);
    drop(guard);
}
