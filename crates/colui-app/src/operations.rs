use crate::lifecycle::LifecycleOperation;
use colui_domain::{AppError, ProfileId};
use std::future::Future;
use std::pin::Pin;

pub type OperationKind = LifecycleOperation;

pub type OperationFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, AppError>> + Send + 'a>>;

/// RAII guard for lifecycle operations.
/// Releases the operation lock exactly once when dropped.
pub trait LifecycleOperationGuard: Send + Sync {}

/// RAII guard for definition loading operations.
/// Releases the definition lock exactly once when dropped.
pub trait DefinitionLoadGuard: Send + Sync {}

/// Manages exclusive locks for profile lifecycle operations.
pub trait OperationLockManager: Send + Sync {
    /// Acquire an exclusive lifecycle lock for the given profile.
    /// Returns `OperationConflict` if another operation is already in progress.
    fn acquire_lifecycle(
        &self,
        profile_id: ProfileId,
        operation: OperationKind,
    ) -> OperationFuture<'_, Box<dyn LifecycleOperationGuard>>;

    /// Acquire an exclusive definition load lock for the given profile.
    /// Returns `OperationConflict` if a load is already in progress.
    fn acquire_definition_load(
        &self,
        profile_id: ProfileId,
    ) -> OperationFuture<'_, Box<dyn DefinitionLoadGuard>>;
}
