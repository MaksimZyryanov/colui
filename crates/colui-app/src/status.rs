use crate::ProfileReader;
use colui_domain::{
    AppError, AppErrorCode, DefinitionState, Issue, ProfileId, ProjectProfile, RuntimeActivity,
    RuntimePresence, Timestamp,
};
use std::future::Future;
use std::pin::Pin;

pub type ProjectStatusFuture<'a> =
    Pin<Box<dyn Future<Output = Result<ProjectStatus, AppError>> + Send + 'a>>;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct RuntimeProjection {
    pub presence: RuntimePresence,
    pub activity: Option<RuntimeActivity>,
    pub container_count: u32,
    pub running_container_count: u32,
    pub observed_at: Option<Timestamp>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProjectStatus {
    pub profile_id: ProfileId,
    pub runtime: RuntimeProjection,
    pub definition_state: DefinitionState,
    pub issues: Vec<Issue>,
}

pub trait ProjectStatusReader: Send + Sync {
    fn project_status(&self, profile: ProjectProfile) -> ProjectStatusFuture<'_>;
}

pub struct GetProjectStatus<'a, R: ?Sized, S: ?Sized> {
    profiles: &'a R,
    status: &'a S,
}

impl<'a, R: ProfileReader + ?Sized, S: ProjectStatusReader + ?Sized> GetProjectStatus<'a, R, S> {
    pub fn new(profiles: &'a R, status: &'a S) -> Self {
        Self { profiles, status }
    }

    pub async fn execute(&self, id: ProfileId) -> Result<ProjectStatus, AppError> {
        let profile = self
            .profiles
            .load()
            .await?
            .profiles
            .into_iter()
            .find(|profile| profile.id == id)
            .ok_or_else(|| {
                AppError::new(
                    AppErrorCode::ProfileNotFound,
                    "get_project_status",
                    Some(id),
                    "profile not found",
                )
            })?;
        self.status.project_status(profile).await
    }
}
