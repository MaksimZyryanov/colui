use colui_domain::{AppError, ProfileId, ProjectDefinition, ProjectProfile};
use std::future::Future;
use std::pin::Pin;

pub type DefinitionFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, AppError>> + Send + 'a>>;

/// Read-only access to cached project definitions.
pub trait DefinitionReader: Send + Sync {
    fn definition(&self, profile: ProjectProfile) -> DefinitionFuture<'_, ProjectDefinition>;
}

/// Trigger definition parsing and caching operations.
pub trait DefinitionRefresher: DefinitionReader {
    fn refresh_definition(
        &self,
        profile: ProjectProfile,
    ) -> DefinitionFuture<'_, ProjectDefinition>;
    fn invalidate(&self, profile_id: ProfileId);
}
