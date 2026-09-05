use crate::{RegistrySnapshot, StoreFuture};
use colui_domain::AppError;

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
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RegistryHealth {
    pub state: RegistryHealthState,
    pub identity: Option<RegistrySnapshotIdentity>,
    pub error: Option<AppError>,
}

pub trait RegistryRecovery: Send + Sync {
    fn registry_health(&self) -> StoreFuture<'_, RegistryHealth>;
    fn create_registry_backup(&self) -> StoreFuture<'_, RegistrySnapshotIdentity>;
    fn restore_registry_backup(&self) -> StoreFuture<'_, RegistrySnapshot>;
}
