use colui_adapters::OperationLockManager as ConcreteOperationLockManager;
use colui_app::{
    DefinitionBusy, OperationKind, OperationLockManager as OperationLockManagerPort,
    OperationLockReader,
};
use colui_domain::{AppErrorCode, ProfileId};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;
use uuid::Uuid;

fn id(value: u128) -> ProfileId {
    ProfileId::new(Uuid::from_u128(value))
}

#[tokio::test]
async fn lifecycle_pending_blocks_definition_and_duplicate_lifecycle() {
    let locks = ConcreteOperationLockManager::new();
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
    let locks = ConcreteOperationLockManager::new();
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

#[tokio::test]
async fn pending_lifecycle_overtakes_definition_lease() {
    let locks = Arc::new(ConcreteOperationLockManager::new());
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
    let locks = Arc::new(ConcreteOperationLockManager::new());
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

#[tokio::test]
async fn profiles_hold_independent_operation_leases() {
    let locks = ConcreteOperationLockManager::new();
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
    let locks = ConcreteOperationLockManager::new();
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
