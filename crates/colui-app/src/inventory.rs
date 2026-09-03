use colui_domain::{AppError, ProfileId, ProjectRuntimeSnapshot, RuntimeInventory};
use std::future::Future;
use std::pin::Pin;

pub type InventoryFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, AppError>> + Send + 'a>>;

/// Read-only access to cached runtime inventory.
pub trait InventoryReader: Send + Sync {
    /// Get the current inventory snapshot for a profile.
    /// Returns `Unavailable` generation if no inventory has been collected.
    fn get_inventory(&self, profile_id: ProfileId) -> InventoryFuture<'_, RuntimeInventory>;

    /// Get the full runtime snapshot (containers + metadata) for a profile.
    fn get_snapshot(&self, profile_id: ProfileId) -> InventoryFuture<'_, ProjectRuntimeSnapshot>;
}

/// Trigger inventory refresh operations.
pub trait InventoryRefresher: Send + Sync {
    /// Refresh inventory for a specific profile.
    /// Returns the new inventory generation.
    fn refresh_profile(&self, profile_id: ProfileId) -> InventoryFuture<'_, RuntimeInventory>;

    /// Refresh inventory for all profiles.
    /// Returns the count of profiles refreshed.
    fn refresh_all(&self) -> InventoryFuture<'_, usize>;
}
