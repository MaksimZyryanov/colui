pub mod definitions;
mod inventory;
pub mod operations;
pub mod registry;
pub mod runtime;

pub use definitions::DefinitionCache;
pub use inventory::InventoryCoordinator;
pub use operations::OperationLockManager;
pub use registry::{JsonProfileRegistry, RegistryConfig, RegistryRecoveryIo};

#[derive(Clone, Copy)]
pub struct UuidGenerator;

impl colui_app::IdGenerator for UuidGenerator {
    fn generate(&self) -> colui_domain::ProfileId {
        colui_domain::ProfileId::new(uuid::Uuid::new_v4())
    }
}
