use crate::ProfileReader;
use colui_domain::{
    AppError, AppErrorCode, DefinitionState, Issue, IssueCode, ProfileId, ProjectProfile,
    RuntimeActivity, RuntimePresence, Timestamp,
};
use std::collections::{BTreeMap, HashMap};
use std::path::{Component, Path, PathBuf};

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
    let observed_at = (presence == RuntimePresence::Present)
        .then_some(inventory.observed_at)
        .flatten();
    ProjectStatus {
        profile_id: profile.id.clone(),
        runtime: RuntimeProjection {
            presence,
            activity,
            container_count,
            running_container_count,
            observed_at,
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

pub fn project_status_from_inventory_and_definition_projection(
    profile: &ProjectProfile,
    inventory: colui_domain::RuntimeInventory,
    definition: &crate::DefinitionProjection,
) -> ProjectStatus {
    project_status_from_inventory_and_definition(
        profile,
        inventory,
        Some(definition.definition.clone()),
    )
}

pub fn project_status_from_registry_inventory_and_definition(
    profile: &ProjectProfile,
    registry: &crate::RegistrySnapshot,
    inventory: colui_domain::RuntimeInventory,
    definition: Option<colui_domain::ProjectDefinition>,
) -> ProjectStatus {
    let associations = RuntimeAssociationContext::new(registry, &inventory);
    let mut status = project_status_from_associations(profile, &inventory, &associations);
    if let Some(definition) = definition {
        status.definition_state = definition.state;
        status.issues = definition.issues;
    }
    if associations.is_ambiguous(profile) {
        status.issues.push(Issue {
            field: Some(IssueCode::AmbiguousRuntimeAssociation.as_str().to_owned()),
            message: "runtime association is ambiguous".to_owned(),
        });
    }
    status
}

type AssociationTuple = (String, String, Vec<String>);

struct RuntimeAssociationContext {
    profile_counts: BTreeMap<AssociationTuple, usize>,
    profile_tuples: HashMap<ProfileId, Option<AssociationTuple>>,
    observation_counts: BTreeMap<AssociationTuple, usize>,
}

impl RuntimeAssociationContext {
    fn new(registry: &crate::RegistrySnapshot, inventory: &colui_domain::RuntimeInventory) -> Self {
        let mut profile_counts = BTreeMap::new();
        let mut profile_tuples = HashMap::new();
        for profile in &registry.profiles {
            let tuple = profile_tuple(profile);
            if let Some(tuple) = &tuple {
                *profile_counts.entry(tuple.clone()).or_insert(0) += 1;
            }
            profile_tuples.insert(profile.id.clone(), tuple);
        }
        let mut observation_counts = BTreeMap::new();
        for group in &inventory.compose_observation_groups {
            if let Some(working_directory) = &group.working_directory {
                *observation_counts
                    .entry((
                        group.compose_project_name.clone(),
                        working_directory.clone(),
                        group.config_files.clone(),
                    ))
                    .or_insert(0) += 1;
            }
        }
        Self {
            profile_counts,
            profile_tuples,
            observation_counts,
        }
    }

    fn profile_tuple(&self, profile: &ProjectProfile) -> Option<&AssociationTuple> {
        self.profile_tuples
            .get(&profile.id)
            .and_then(Option::as_ref)
    }

    fn is_ambiguous(&self, profile: &ProjectProfile) -> bool {
        let Some(tuple) = self.profile_tuple(profile) else {
            return true;
        };
        self.profile_counts.get(tuple).copied().unwrap_or(0) != 1
            || self.observation_counts.get(tuple).copied().unwrap_or(0) != 1
    }
}

fn project_status_from_associations(
    profile: &ProjectProfile,
    inventory: &colui_domain::RuntimeInventory,
    associations: &RuntimeAssociationContext,
) -> ProjectStatus {
    let containers = associations
        .profile_tuple(profile)
        .filter(|tuple| {
            associations.profile_counts.get(*tuple) == Some(&1)
                && associations.observation_counts.get(*tuple) == Some(&1)
        })
        .and_then(|tuple| {
            inventory.compose_observation_groups.iter().find(|group| {
                group.compose_project_name == tuple.0
                    && group.working_directory.as_deref() == Some(tuple.1.as_str())
                    && group.config_files == tuple.2
            })
        })
        .map(|group| {
            inventory
                .containers
                .iter()
                .filter(|container| group.container_ids.contains(&container.id))
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();
    project_status(profile, inventory, &containers)
}

fn project_status(
    profile: &ProjectProfile,
    inventory: &colui_domain::RuntimeInventory,
    containers: &[&colui_domain::ContainerInstance],
) -> ProjectStatus {
    let container_count = containers.len() as u32;
    let running_container_count = containers
        .iter()
        .filter(|container| container.state == colui_domain::ContainerState::Running)
        .count() as u32;
    let (presence, activity) = runtime_state(
        inventory.has_snapshot,
        container_count,
        running_container_count,
    );
    ProjectStatus {
        profile_id: profile.id.clone(),
        runtime: RuntimeProjection {
            observed_at: (presence == RuntimePresence::Present)
                .then_some(inventory.observed_at.clone())
                .flatten(),
            presence,
            activity,
            container_count,
            running_container_count,
        },
        definition_state: DefinitionState::Unchecked,
        issues: Vec::new(),
    }
}

fn runtime_state(
    has_snapshot: bool,
    container_count: u32,
    running_container_count: u32,
) -> (RuntimePresence, Option<RuntimeActivity>) {
    if !has_snapshot {
        (RuntimePresence::Unavailable, None)
    } else if container_count == 0 {
        (RuntimePresence::Absent, None)
    } else if running_container_count == container_count {
        (RuntimePresence::Present, Some(RuntimeActivity::AllRunning))
    } else if running_container_count == 0 {
        (RuntimePresence::Present, Some(RuntimeActivity::NoneRunning))
    } else {
        (RuntimePresence::Present, Some(RuntimeActivity::Mixed))
    }
}

fn profile_tuple(profile: &ProjectProfile) -> Option<AssociationTuple> {
    let working_directory = normalize_absolute_path(&profile.working_directory)?;
    let mut config_files = Vec::new();
    for compose_file in &profile.compose_files {
        let path = if compose_file.is_absolute() {
            compose_file.clone()
        } else {
            PathBuf::from(&working_directory).join(compose_file)
        };
        let normalized = normalize_absolute_path(&path)?;
        if !config_files.contains(&normalized) {
            config_files.push(normalized);
        }
    }
    Some((
        profile.compose_project_name.as_ref().to_owned(),
        working_directory,
        config_files,
    ))
}

fn normalize_absolute_path(path: &Path) -> Option<String> {
    if !path.is_absolute() {
        return None;
    }
    let mut normalized = PathBuf::new();
    for component in path.components() {
        match component {
            Component::RootDir | Component::Prefix(_) => normalized.push(component.as_os_str()),
            Component::CurDir => {}
            Component::ParentDir => {
                normalized.pop();
            }
            Component::Normal(value) => normalized.push(value),
        }
    }
    normalized.to_str().map(str::to_owned)
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
    fn project_status(
        &self,
        profile: ProjectProfile,
        registry: crate::RegistrySnapshot,
    ) -> ProjectStatusFuture<'_>;
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
        let registry = self.profiles.load().await?;
        let profile = registry
            .profiles
            .iter()
            .find(|profile| profile.id == id)
            .cloned()
            .ok_or_else(|| {
                AppError::new(
                    AppErrorCode::ProfileNotFound,
                    "get_project_status",
                    Some(id),
                    "profile not found",
                )
            })?;
        self.status.project_status(profile, registry).await
    }
}
