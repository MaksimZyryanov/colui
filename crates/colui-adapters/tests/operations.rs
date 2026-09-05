use colui_adapters::OperationLockManager as ConcreteOperationLockManager;
use colui_app::{
    DefinitionBusy, OperationKind, OperationLockManager as OperationLockManagerPort,
    OperationLockReader,
};
use colui_domain::{AppErrorCode, ProfileId};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;
use uuid::Uuid;

fn id(value: u128) -> ProfileId {
    ProfileId::new(Uuid::from_u128(value))
}

fn locks() -> ConcreteOperationLockManager {
    ConcreteOperationLockManager::new(
        std::env::temp_dir().join(format!("colui-{}.recovery.lock", Uuid::new_v4())),
    )
}

#[tokio::test]
async fn lifecycle_pending_blocks_definition_and_duplicate_lifecycle() {
    let locks = locks();
    let lifecycle = locks
        .acquire_lifecycle(id(1), OperationKind::Apply)
        .await
        .unwrap();

    assert!(matches!(
        locks.acquire_definition(id(1)),
        Err(DefinitionBusy::LifecyclePending)
    ));
    assert_eq!(
        locks
            .acquire_lifecycle(id(1), OperationKind::Stop)
            .await
            .unwrap_err()
            .code,
        AppErrorCode::OperationConflict
    );

    drop(lifecycle);
    assert!(!locks.is_busy(&id(1)));
}

#[tokio::test]
async fn lifecycle_guard_release_allows_next_lifecycle() {
    let locks = locks();
    let first = locks
        .acquire_lifecycle(id(1), OperationKind::Apply)
        .await
        .unwrap();
    assert!(locks.is_busy(&id(1)));

    drop(first);

    let second = locks
        .acquire_lifecycle(id(1), OperationKind::Stop)
        .await
        .unwrap();
    drop(second);
    assert!(!locks.is_busy(&id(1)));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_lifecycle_acquisition_has_one_winner() {
    let locks = Arc::new(locks());
    let barrier = Arc::new(tokio::sync::Barrier::new(2));
    let first = acquire_lifecycle_at_barrier(Arc::clone(&locks), Arc::clone(&barrier));
    let second = acquire_lifecycle_at_barrier(Arc::clone(&locks), barrier);

    let (first, second) = tokio::join!(first, second);
    assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
    drop(first);
    drop(second);
}

async fn acquire_lifecycle_at_barrier(
    locks: Arc<ConcreteOperationLockManager>,
    barrier: Arc<tokio::sync::Barrier>,
) -> Result<colui_app::LifecycleOperationGuard, colui_domain::AppError> {
    barrier.wait().await;
    locks.acquire_lifecycle(id(1), OperationKind::Apply).await
}

#[tokio::test]
async fn pending_lifecycle_overtakes_definition_lease() {
    let locks = Arc::new(locks());
    let definition = locks.acquire_definition(id(1)).unwrap();
    let pending = {
        let locks = Arc::clone(&locks);
        tokio::spawn(async move { locks.acquire_lifecycle(id(1), OperationKind::Restart).await })
    };

    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if matches!(
                locks.acquire_definition(id(1)),
                Err(DefinitionBusy::LifecyclePending)
            ) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();
    assert_eq!(
        locks
            .acquire_lifecycle(id(1), OperationKind::Stop)
            .await
            .unwrap_err()
            .code,
        AppErrorCode::OperationConflict
    );

    drop(definition);
    let lifecycle = pending.await.unwrap().unwrap();
    drop(lifecycle);
    assert!(!locks.is_busy(&id(1)));
}

#[tokio::test]
async fn cancelled_pending_lifecycle_releases_reservation() {
    let locks = Arc::new(locks());
    let definition = locks.acquire_definition(id(1)).unwrap();
    let pending = {
        let locks = Arc::clone(&locks);
        tokio::spawn(async move { locks.acquire_lifecycle(id(1), OperationKind::Apply).await })
    };

    tokio::time::timeout(Duration::from_secs(1), async {
        loop {
            if matches!(
                locks.acquire_definition(id(1)),
                Err(DefinitionBusy::LifecyclePending)
            ) {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await
    .unwrap();

    pending.abort();
    assert!(pending.await.unwrap_err().is_cancelled());
    drop(definition);

    assert!(!locks.is_busy(&id(1)));
    let definition = locks.acquire_definition(id(1)).unwrap();
    drop(definition);
}

#[test]
fn concurrent_definition_acquisition_has_one_winner() {
    let locks = Arc::new(locks());
    let barrier = Arc::new(Barrier::new(2));
    let first = acquire_definition_at_barrier(Arc::clone(&locks), Arc::clone(&barrier));
    let second = acquire_definition_at_barrier(Arc::clone(&locks), barrier);
    let first = first.join().unwrap();
    let second = second.join().unwrap();

    assert_eq!(usize::from(first.is_ok()) + usize::from(second.is_ok()), 1);
    drop(first);
    drop(second);
}

fn acquire_definition_at_barrier(
    locks: Arc<ConcreteOperationLockManager>,
    barrier: Arc<Barrier>,
) -> thread::JoinHandle<Result<colui_app::DefinitionLoadGuard, DefinitionBusy>> {
    thread::spawn(move || {
        barrier.wait();
        locks.acquire_definition(id(1))
    })
}

#[tokio::test]
async fn cancelled_active_lifecycle_releases_guard() {
    let locks = Arc::new(locks());
    let acquired = Arc::new(tokio::sync::Notify::new());
    let held = Arc::clone(&locks);
    let acquired_task = Arc::clone(&acquired);
    let task = tokio::spawn(async move {
        let _guard = held
            .acquire_lifecycle(id(1), OperationKind::Apply)
            .await
            .unwrap();
        acquired_task.notify_one();
        std::future::pending::<()>().await;
    });

    tokio::time::timeout(Duration::from_secs(1), acquired.notified())
        .await
        .unwrap();
    assert!(locks.is_busy(&id(1)));
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());

    let guard = locks
        .acquire_lifecycle(id(1), OperationKind::Stop)
        .await
        .unwrap();
    drop(guard);
}

#[tokio::test]
async fn profiles_hold_independent_operation_leases() {
    let locks = locks();
    let lifecycle = locks
        .acquire_lifecycle(id(1), OperationKind::Apply)
        .await
        .unwrap();
    let definition = locks.acquire_definition(id(2)).unwrap();

    assert!(locks.is_busy(&id(1)));
    assert!(locks.is_busy(&id(2)));

    drop(lifecycle);
    drop(definition);
    assert!(!locks.is_busy(&id(1)));
    assert!(!locks.is_busy(&id(2)));
}

#[test]
fn definition_lease_releases_after_guard_drop() {
    let locks = locks();
    let definition = locks.acquire_definition(id(1)).unwrap();
    assert!(matches!(
        locks.acquire_definition(id(1)),
        Err(DefinitionBusy::DefinitionActive)
    ));

    drop(definition);
    assert!(locks.acquire_definition(id(1)).is_ok());
}

#[test]
fn guards_are_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<colui_app::LifecycleOperationGuard>();
    assert_send_sync::<colui_app::DefinitionLoadGuard>();
}

#[test]
fn guard_release_callback_runs_once() {
    let releases = Arc::new(AtomicUsize::new(0));
    let guard = {
        let releases = Arc::clone(&releases);
        colui_app::LifecycleOperationGuard::new(move || {
            releases.fetch_add(1, Ordering::AcqRel);
        })
    };
    drop(guard);
    assert_eq!(releases.load(Ordering::Acquire), 1);
}

fn recovery_locks() -> (TempDir, ConcreteOperationLockManager) {
    let directory = tempfile::tempdir().unwrap();
    let locks = ConcreteOperationLockManager::new(directory.path().join("registry.recovery.lock"));
    (directory, locks)
}

#[test]
fn container_actions_exclude_same_container_but_not_other_containers() {
    let (_directory, locks) = recovery_locks();
    let first = locks.acquire_container("container-a").unwrap();
    assert_eq!(
        locks.acquire_container("container-a").unwrap_err().code,
        AppErrorCode::OperationConflict
    );
    let second = locks.acquire_container("container-b").unwrap();
    drop((first, second));
    assert!(locks.acquire_container("container-a").is_ok());
}

#[test]
fn recovery_fails_fast_while_shared_operation_is_active() {
    let (_directory, locks) = recovery_locks();
    let mutation = locks.acquire_mutation().unwrap();
    assert_eq!(
        locks.acquire_recovery().unwrap_err().code,
        AppErrorCode::OperationConflict
    );
    drop(mutation);
    let recovery = locks.acquire_recovery().unwrap();
    assert_eq!(
        locks.acquire_mutation().unwrap_err().code,
        AppErrorCode::OperationConflict
    );
    drop(recovery);
    assert!(locks.acquire_mutation().is_ok());
}

#[test]
fn recovery_file_lease_excludes_another_manager() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("registry.recovery.lock");
    let first = ConcreteOperationLockManager::new(path.clone());
    let second = ConcreteOperationLockManager::new(path);
    let operation = first.acquire_mutation().unwrap();
    assert_eq!(
        second.acquire_recovery().unwrap_err().code,
        AppErrorCode::RecoveryConflict
    );
    drop(operation);
    assert!(second.acquire_recovery().is_ok());
}

#[test]
fn recovery_lock_child_process() {
    let Ok(lock_path) = std::env::var("COLUI_RECOVERY_CHILD_LOCK") else {
        return;
    };
    let ready = std::env::var("COLUI_RECOVERY_CHILD_READY").unwrap();
    let release = std::env::var("COLUI_RECOVERY_CHILD_RELEASE").unwrap();
    let locks = ConcreteOperationLockManager::new(lock_path.into());
    let _lease = locks.acquire_mutation().unwrap();
    std::fs::write(&ready, b"ready").unwrap();
    while !std::path::Path::new(&release).exists() {
        std::thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn recovery_file_lease_excludes_another_process() {
    let directory = tempfile::tempdir().unwrap();
    let lock_path = directory.path().join("registry.recovery.lock");
    let ready = directory.path().join("ready");
    let release = directory.path().join("release");
    let mut child = std::process::Command::new(std::env::current_exe().unwrap())
        .args(["--exact", "recovery_lock_child_process", "--nocapture"])
        .env("COLUI_RECOVERY_CHILD_LOCK", &lock_path)
        .env("COLUI_RECOVERY_CHILD_READY", &ready)
        .env("COLUI_RECOVERY_CHILD_RELEASE", &release)
        .spawn()
        .unwrap();
    let deadline = std::time::Instant::now() + Duration::from_secs(5);
    while !ready.exists() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(ready.exists());
    let locks = ConcreteOperationLockManager::new(lock_path);
    assert_eq!(
        locks.acquire_recovery().unwrap_err().code,
        AppErrorCode::RecoveryConflict
    );
    std::fs::write(release, b"release").unwrap();
    assert!(child.wait().unwrap().success());
}
