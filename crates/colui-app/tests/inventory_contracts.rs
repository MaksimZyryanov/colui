use colui_app::{LifecycleResult, OperationKind};
use colui_domain::{ProfileId, RuntimeInventory};
use std::sync::Arc;
use tokio::sync::Mutex;
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

    async fn acquire_lifecycle(
        &self,
        profile_id: ProfileId,
        operation: OperationKind,
    ) -> Result<LifecycleOperationGuard, colui_domain::AppError> {
        let mut locks = self.locks.lock().await;
        if locks.iter().any(|(id, _)| id == &profile_id) {
            return Err(colui_domain::AppError::new(
                colui_domain::AppErrorCode::OperationConflict,
                "acquire_lifecycle",
                Some(profile_id),
                "operation already in progress",
            ));
        }
        locks.push((profile_id.clone(), operation));
        Ok(LifecycleOperationGuard {
            profile_id,
            locks: self.locks.clone(),
        })
    }
}

#[derive(Debug)]
struct LifecycleOperationGuard {
    profile_id: ProfileId,
    locks: Arc<Mutex<Vec<(ProfileId, OperationKind)>>>,
}

impl Drop for LifecycleOperationGuard {
    fn drop(&mut self) {
        let profile_id = self.profile_id.clone();
        let locks = self.locks.clone();
        tokio::spawn(async move {
            let mut locks = locks.lock().await;
            locks.retain(|(id, _)| id != &profile_id);
        });
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
