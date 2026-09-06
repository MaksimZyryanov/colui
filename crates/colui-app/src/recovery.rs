use crate::{
    DefinitionInvalidator, OperationLockManager, RegistryRecoveryGuard, RegistrySnapshot,
    StoreFuture,
};
use colui_domain::{AppError, Timestamp};

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistrySnapshotIdentity {
    pub registry_revision: u64,
    pub canonical_content_sha256: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RegistryHealthState {
    Healthy,
    Missing,
    Corrupt,
    Unreadable,
    Locked,
    WriteFailure,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryHealth {
    pub state: RegistryHealthState,
    pub identity: Option<RegistrySnapshotIdentity>,
    pub error: Option<AppError>,
    pub last_operation_at: Option<Timestamp>,
    pub last_failure_at: Option<Timestamp>,
}

pub trait RegistryRecovery: Send + Sync {
    fn registry_health(&self) -> StoreFuture<'_, RegistryHealth>;
    fn create_registry_backup(&self) -> StoreFuture<'_, RegistrySnapshotIdentity>;
    fn restore_registry_backup(
        &self,
        guard: &RegistryRecoveryGuard,
    ) -> StoreFuture<'_, RegistrySnapshot>;
}

pub struct RestoreRegistryBackup<'a, R: ?Sized, L: ?Sized, D: ?Sized> {
    registry: &'a R,
    locks: &'a L,
    definitions: &'a D,
}

impl<'a, R, L, D> RestoreRegistryBackup<'a, R, L, D>
where
    R: RegistryRecovery + ?Sized,
    L: OperationLockManager + ?Sized,
    D: DefinitionInvalidator + ?Sized,
{
    pub fn new(registry: &'a R, locks: &'a L, definitions: &'a D) -> Self {
        Self {
            registry,
            locks,
            definitions,
        }
    }

    pub async fn execute(&self) -> Result<RegistrySnapshot, AppError> {
        let guard = self.locks.acquire_recovery()?;
        let snapshot = self.registry.restore_registry_backup(&guard).await?;
        self.definitions.invalidate_all();
        Ok(snapshot)
    }
}
