use colui_domain::{AppError, DefinitionRevision, ProfileId, ProjectDefinition};
use std::future::Future;
use std::pin::Pin;

pub type DefinitionFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, AppError>> + Send + 'a>>;

/// Read-only access to cached project definitions.
pub trait DefinitionReader: Send + Sync {
    /// Get the cached definition for a profile.
    /// Returns `NotLoaded` if no definition has been parsed yet.
    fn get_definition(&self, profile_id: ProfileId) -> DefinitionFuture<'_, ProjectDefinition>;

    /// Get the current definition revision number.
    fn get_revision(&self, profile_id: ProfileId) -> DefinitionFuture<'_, DefinitionRevision>;
}

/// Trigger definition parsing and caching operations.
pub trait DefinitionRefresher: Send + Sync {
    /// Parse and cache the definition for a specific profile.
    /// Returns the new definition revision.
    fn refresh_definition(
        &self,
        profile_id: ProfileId,
    ) -> DefinitionFuture<'_, ProjectDefinition>;
}
