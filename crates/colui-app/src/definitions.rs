use colui_domain::{AppError, ProfileId, ProjectDefinition, ProjectProfile};
use std::future::Future;
use std::pin::Pin;

pub type DefinitionFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, AppError>> + Send + 'a>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DefinitionProjection {
    pub definition: ProjectDefinition,
    pub error: Option<AppError>,
}

/// Read-only access to cached project definitions.
pub trait DefinitionReader: Send + Sync {
    fn definition(&self, profile: ProjectProfile) -> DefinitionFuture<'_, DefinitionProjection>;
}

/// Trigger definition parsing and caching operations.
pub trait DefinitionRefresher: DefinitionReader {
    fn refresh_definition(
        &self,
        profile: ProjectProfile,
    ) -> DefinitionFuture<'_, DefinitionProjection>;
    fn invalidate(&self, profile_id: ProfileId);
}
