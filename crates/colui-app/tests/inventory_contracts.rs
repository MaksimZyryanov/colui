use colui_app::{
    LifecycleOperationGuard, LifecycleResult, OperationFuture, OperationKind, OperationLockManager,
    OperationLockReader,
};
use colui_domain::{AppError, AppErrorCode, ProfileId, RuntimeInventory};
use std::sync::{Arc, Mutex};
use uuid::Uuid;

fn profile_id() -> ProfileId {
    ProfileId::new(Uuid::from_u128(1))
}

struct FakeOperationLocks {
    locks: Arc<Mutex<Vec<(ProfileId, OperationKind)>>>,
}

impl FakeOperationLocks {
    fn new() -> Self {
        Self {
            locks: Arc::new(Mutex::new(Vec::new())),
        }
    }
}

impl OperationLockReader for FakeOperationLocks {
    fn is_busy(&self, profile_id: &ProfileId) -> bool {
        self.locks
            .lock()
            .unwrap()
            .iter()
            .any(|(id, _)| id == profile_id)
    }
}

impl OperationLockManager for FakeOperationLocks {
    fn acquire_lifecycle(
        &self,
        profile_id: ProfileId,
        operation: OperationKind,
    ) -> OperationFuture<'_, LifecycleOperationGuard> {
        let lock_store = self.locks.clone();
        Box::pin(async move {
            let mut locks = lock_store.lock().unwrap();
            if locks.iter().any(|(id, _)| id == &profile_id) {
                return Err(AppError::new(
                    AppErrorCode::OperationConflict,
                    "acquire_lifecycle",
                    Some(profile_id),
                    "operation already in progress",
                ));
            }
            locks.push((profile_id.clone(), operation));
            let release_locks = lock_store.clone();
            Ok(LifecycleOperationGuard::new(move || {
                let mut locks = release_locks.lock().unwrap();
                locks.retain(|(id, _)| id != &profile_id);
            }))
        })
    }

    fn acquire_definition(
        &self,
        profile_id: ProfileId,
    ) -> Result<colui_app::DefinitionLoadGuard, colui_app::DefinitionBusy> {
        let mut locks = self.locks.lock().unwrap();
        if locks.iter().any(|(id, _)| id == &profile_id) {
            return Err(colui_app::DefinitionBusy::LifecyclePending);
        }
        locks.push((profile_id.clone(), OperationKind::Apply));
        drop(locks);
        let release_locks = self.locks.clone();
        Ok(colui_app::DefinitionLoadGuard::new(move || {
            let mut locks = release_locks.lock().unwrap();
            locks.retain(|(id, _)| id != &profile_id);
        }))
    }
}

#[tokio::test]
async fn concurrent_lifecycle_acquisition_conflicts_per_profile() {
    let locks = FakeOperationLocks::new();
    let first = locks
        .acquire_lifecycle(profile_id(), OperationKind::Apply)
        .await
        .unwrap();
    let error = locks
        .acquire_lifecycle(profile_id(), OperationKind::Stop)
        .await
        .unwrap_err();
    assert_eq!(error.code, colui_domain::AppErrorCode::OperationConflict);
    drop(first);
}

#[test]
fn lifecycle_result_exposes_inventory_generation() {
    let result = LifecycleResult {
        profile_id: profile_id(),
        success: true,
        inventory: RuntimeInventory::unavailable(),
    };
    assert_eq!(result.inventory.generation, 0);
}
