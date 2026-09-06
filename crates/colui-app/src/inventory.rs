use colui_domain::{AppError, RuntimeInventory};
use std::future::Future;
use std::pin::Pin;

pub type InventoryFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, AppError>> + Send + 'a>>;

/// A causal marker issued by the same coordinator that starts inventory lists.
#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ObservationOrder(pub u64);

/// Read-only access to cached runtime inventory.
pub trait InventoryReader: Send + Sync {
    fn current_inventory(&self) -> InventoryFuture<'_, RuntimeInventory>;
}

/// Trigger inventory refresh operations.
pub trait InventoryRefresher: InventoryReader {
    fn refresh(&self) -> InventoryFuture<'_, RuntimeInventory>;
    /// Advance the coordinator's list/action order immediately after daemon success.
    fn observation_marker(&self) -> ObservationOrder;
    /// Observe after this coordinator's marker, never joining an older list.
    /// On failure, retain the stale/unavailable result for `current_inventory`.
    fn refresh_after(&self, marker: ObservationOrder) -> InventoryFuture<'_, RuntimeInventory>;
}
