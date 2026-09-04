use colui_adapters::definitions::DefinitionCache;
use colui_adapters::OperationLockManager;
use colui_app::{
    Clock, ComposeInvocation, ComposeProcessResult, ComposeRunner, DefinitionReader,
    DefinitionRefresher, OperationKind, OperationLockManager as OperationLockManagerPort,
    RuntimeFuture, RuntimeStateReader,
};
use colui_domain::{
    DaemonFingerprint, DefinitionState, DisplayName, DockerEndpoint, ProfileDraft, ProfileId,
    ProjectProfile, RegistrationOrigin, RuntimeSessionId, RuntimeSessionState, Timestamp,
};
use std::path::PathBuf;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
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
    let locks = Arc::new(OperationLockManager::new());
    let clock = Arc::new(TestClock {
        mono: Arc::new(0.into()),
    });
    let cache = DefinitionCache::new(runner.clone(), Arc::new(Runtime), clock, locks.clone());
    let old = profile(1);
    cache.refresh_definition(old.clone()).await.unwrap();
    let guard = locks
        .acquire_lifecycle(old.id.clone(), OperationKind::Apply)
        .await
        .unwrap();
    let newer = profile(2);
    let definition = cache.definition(newer).await.unwrap();
    assert_eq!(definition.state, DefinitionState::Unchecked);
    assert!(definition.services.is_empty());
    drop(guard);
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
        Arc::new(OperationLockManager::new()),
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
            .definition_revision,
        c2.refresh_definition(p).await.unwrap().definition_revision
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
    assert_eq!(definition.state, DefinitionState::Invalid);
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

#[tokio::test]
async fn lifecycle_busy_returns_unchecked_without_running_compose() {
    let runner = Runner::new(r#"{"services":{}}"#);
    let locks = Arc::new(OperationLockManager::new());
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
    );
    let definition = cache.definition(profile(1)).await.unwrap();
    assert_eq!(definition.state, DefinitionState::Unchecked);
    assert_eq!(runner.calls.load(Ordering::SeqCst), 0);
    drop(guard);
}
