use crate::ProfileReader;
use colui_domain::{
    AppError, AppErrorCode, DefinitionState, Issue, ProfileId, ProjectProfile, RuntimeActivity,
    RuntimePresence, Timestamp,
};

pub fn project_status_from_inventory(
    profile: &ProjectProfile,
    inventory: colui_domain::RuntimeInventory,
) -> ProjectStatus {
    let containers = inventory
        .project_snapshots
        .iter()
        .find(|snapshot| snapshot.compose_project_name == profile.compose_project_name.as_ref())
        .map(|snapshot| snapshot.containers.as_slice())
        .unwrap_or(&[]);
    let container_count = containers.len() as u32;
    let running_container_count = containers
        .iter()
        .filter(|container| container.state == colui_domain::ContainerState::Running)
        .count() as u32;
    let (presence, activity) = if !inventory.has_snapshot {
        (RuntimePresence::Unavailable, None)
    } else if container_count == 0 {
        (RuntimePresence::Absent, None)
    } else if running_container_count == container_count {
        (RuntimePresence::Present, Some(RuntimeActivity::AllRunning))
    } else if running_container_count == 0 {
        (RuntimePresence::Present, Some(RuntimeActivity::NoneRunning))
    } else {
        (RuntimePresence::Present, Some(RuntimeActivity::Mixed))
    };
    ProjectStatus {
        profile_id: profile.id.clone(),
        runtime: RuntimeProjection {
            presence,
            activity,
            container_count,
            running_container_count,
            observed_at: inventory.observed_at,
        },
        definition_state: DefinitionState::Unchecked,
        issues: Vec::new(),
    }
}

pub fn project_status_from_inventory_and_definition(
    profile: &ProjectProfile,
    inventory: colui_domain::RuntimeInventory,
    definition: Option<colui_domain::ProjectDefinition>,
) -> ProjectStatus {
    let mut status = project_status_from_inventory(profile, inventory);
    if let Some(definition) = definition {
        status.definition_state = definition.state;
        status.issues = definition.issues;
    }
    status
}
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
