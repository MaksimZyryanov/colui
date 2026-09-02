pub mod registry;
pub mod runtime;

pub use registry::{JsonProfileRegistry, RegistryConfig};

pub struct UuidGenerator;

impl colui_app::IdGenerator for UuidGenerator {
    fn generate(&self) -> colui_domain::ProfileId {
        colui_domain::ProfileId::new(uuid::Uuid::new_v4())
    }
}
