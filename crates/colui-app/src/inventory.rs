use colui_domain::{AppError, RuntimeInventory};
use std::future::Future;
use std::pin::Pin;

pub type InventoryFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, AppError>> + Send + 'a>>;

/// Read-only access to cached runtime inventory.
pub trait InventoryReader: Send + Sync {
    fn current_inventory(&self) -> InventoryFuture<'_, RuntimeInventory>;
}

/// Trigger inventory refresh operations.
pub trait InventoryRefresher: InventoryReader {
    fn refresh(&self) -> InventoryFuture<'_, RuntimeInventory>;
}
