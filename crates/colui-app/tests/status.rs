use colui_app::{
    project_status_from_inventory, project_status_from_registry_inventory_and_definition,
    GetProjectStatus, ProfileReader, ProjectStatus, ProjectStatusFuture, ProjectStatusReader,
    RegistrySnapshot, RuntimeProjection, StoreFuture,
};
use colui_domain::{
    AppError, ComposeObservationGroup, ContainerId, ContainerInstance, ContainerState,
    DefinitionRevision, DefinitionState, Issue, IssueCode, ProfileDraft, ProfileId,
    ProjectDefinition, ProjectProfile, ProjectRuntimeSnapshot, RegistrationOrigin,
    RuntimeInventory, RuntimePresence, Timestamp,
};
use std::sync::Mutex;

struct Profiles(ProjectProfile);

impl ProfileReader for Profiles {
    fn load(&self) -> StoreFuture<'_, RegistrySnapshot> {
        let profile = self.0.clone();
        Box::pin(async move {
            Ok(RegistrySnapshot {
                registry_revision: 1,
                profiles: vec![profile],
            })
        })
    }
}

#[test]
fn successful_inventory_without_matching_project_has_no_observation_timestamp() {
    let inventory = RuntimeInventory {
        generation: 1,
        has_snapshot: true,
        observed_at: Some(Timestamp("2026-09-03T00:00:00Z".into())),
        ..RuntimeInventory::unavailable()
    };

    let status = project_status_from_inventory(&profile(), inventory);

    assert_eq!(status.runtime.presence, RuntimePresence::Absent);
    assert_eq!(status.runtime.activity, None);
    assert_eq!(status.runtime.observed_at, None);
}

struct StatusPort(Mutex<Option<ProfileId>>);

impl ProjectStatusReader for StatusPort {
    fn project_status(
        &self,
        profile: ProjectProfile,
        _: RegistrySnapshot,
    ) -> ProjectStatusFuture<'_> {
        *self.0.lock().unwrap() = Some(profile.id.clone());
        Box::pin(async move {
            Ok(ProjectStatus {
                profile_id: profile.id,
                runtime: RuntimeProjection {
                    presence: RuntimePresence::Absent,
                    activity: None,
                    container_count: 0,
                    running_container_count: 0,
                    observed_at: None,
                },
                definition_state: DefinitionState::Unchecked,
                issues: vec![],
            })
        })
    }
}

fn profile() -> ProjectProfile {
    ProjectProfile::from_draft(
        ProfileId::parse("00000000-0000-0000-0000-000000000001").unwrap(),
        ProfileDraft {
            display_name: "Demo".try_into().unwrap(),
            compose_project_name: "demo".try_into().unwrap(),
            working_directory: "/tmp/demo".into(),
            compose_files: vec!["compose.yml".into()],
            environment_files: vec![],
            registration_origin: RegistrationOrigin::Manual,
        },
    )
    .unwrap()
}

#[tokio::test]
async fn get_project_status_resolves_profile_before_runtime_read() -> Result<(), AppError> {
    let profile = profile();
    let expected_id = profile.id.clone();
    let port = StatusPort(Mutex::new(None));

    let result = GetProjectStatus::new(&Profiles(profile), &port)
        .execute(expected_id.clone())
        .await?;

    assert_eq!(result.profile_id, expected_id);
    assert_eq!(*port.0.lock().unwrap(), Some(expected_id));
    Ok(())
}

#[test]
fn pre_observation_status_is_unavailable_without_runtime_reads() {
    let status = project_status_from_inventory(&profile(), RuntimeInventory::unavailable());
    assert_eq!(status.runtime.presence, RuntimePresence::Unavailable);
    assert_eq!(status.runtime.activity, None);
    assert_eq!(status.runtime.container_count, 0);
    assert_eq!(status.runtime.running_container_count, 0);
    assert_eq!(status.runtime.observed_at, None);
    assert_eq!(status.definition_state, DefinitionState::Unchecked);
    assert!(status.issues.is_empty());
}

#[test]
fn status_projection_uses_shared_project_snapshot() {
    let inventory = RuntimeInventory {
        generation: 1,
        has_snapshot: true,
        project_snapshots: vec![ProjectRuntimeSnapshot {
            compose_project_name: "demo".into(),
            working_directory: None,
            config_files: vec![],
            containers: vec![ContainerInstance {
                id: ContainerId("container".into()),
                name: "web".into(),
                image: "web".into(),
                state: ContainerState::Running,
                status_text: "Up".into(),
                service_name: None,
                published_ports: vec![],
            }],
        }],
        ..RuntimeInventory::unavailable()
    };
    let status = project_status_from_inventory(&profile(), inventory);
    assert_eq!(status.runtime.presence, RuntimePresence::Present);
    assert_eq!(status.runtime.container_count, 1);
    assert_eq!(status.runtime.running_container_count, 1);
}

#[test]
fn ambiguous_runtime_full_tuple_match_projects_only_matching_containers() {
    let profile = profile();
    let registry = RegistrySnapshot {
        registry_revision: 1,
        profiles: vec![profile.clone()],
    };
    let inventory = grouped_inventory(vec![
        group("other", "/tmp/other", &["compose.yml"], &["other"]),
        group("demo", "/tmp/demo", &["compose.yml"], &["matching"]),
    ]);

    let status =
        project_status_from_registry_inventory_and_definition(&profile, &registry, inventory, None);

    assert_eq!(status.runtime.presence, RuntimePresence::Present);
    assert_eq!(status.runtime.container_count, 1);
    assert!(status.issues.is_empty());
}

#[test]
fn ambiguous_runtime_same_name_distinct_tuple_projects_unique_full_tuple_match() {
    let profile = profile();
    let registry = RegistrySnapshot {
        registry_revision: 1,
        profiles: vec![profile.clone()],
    };
    let inventory = grouped_inventory(vec![
        group("demo", "/tmp/other", &["compose.yml"], &["other"]),
        group("demo", "/tmp/demo", &["compose.yml"], &["matching"]),
    ]);

    let status =
        project_status_from_registry_inventory_and_definition(&profile, &registry, inventory, None);

    assert_eq!(status.runtime.presence, RuntimePresence::Present);
    assert_eq!(status.runtime.container_count, 1);
    assert!(status.issues.is_empty());
}

#[test]
fn ambiguous_runtime_zero_matching_full_tuple_projects_no_containers_and_reports_issue() {
    let profile = profile();
    let registry = RegistrySnapshot {
        registry_revision: 1,
        profiles: vec![profile.clone()],
    };
    let inventory = grouped_inventory(vec![group(
        "demo",
        "/tmp/other",
        &["compose.yml"],
        &["other"],
    )]);

    let status =
        project_status_from_registry_inventory_and_definition(&profile, &registry, inventory, None);

    assert_eq!(status.runtime.presence, RuntimePresence::Absent);
    assert_eq!(status.runtime.container_count, 0);
    assert_eq!(
        status.issues[0].field.as_deref(),
        Some(IssueCode::AmbiguousRuntimeAssociation.as_str())
    );
}

#[test]
fn ambiguous_runtime_duplicate_matching_profiles_project_no_containers_and_merge_issues() {
    let first = profile();
    let mut second = profile();
    second.id = ProfileId::parse("00000000-0000-0000-0000-000000000002").unwrap();
    let registry = RegistrySnapshot {
        registry_revision: 1,
        profiles: vec![first.clone(), second],
    };
    let inventory = grouped_inventory(vec![group(
        "demo",
        "/tmp/demo",
        &["compose.yml"],
        &["matching"],
    )]);
    let definition_issue = Issue {
        field: Some("services.web".into()),
        message: "definition issue".into(),
    };
    let definition = ProjectDefinition {
        profile_id: first.id.clone(),
        definition_revision: DefinitionRevision("revision".into()),
        loaded_at: Timestamp("2026-09-05T00:00:00Z".into()),
        state: DefinitionState::Invalid,
        services: vec![],
        issues: vec![definition_issue.clone()],
    };

    let status = project_status_from_registry_inventory_and_definition(
        &first,
        &registry,
        inventory,
        Some(definition),
    );

    assert_eq!(status.runtime.presence, RuntimePresence::Absent);
    assert_eq!(status.runtime.container_count, 0);
    assert_eq!(status.issues[0], definition_issue);
    assert_eq!(
        status.issues[1].field.as_deref(),
        Some(IssueCode::AmbiguousRuntimeAssociation.as_str())
    );
}

fn grouped_inventory(groups: Vec<ComposeObservationGroup>) -> RuntimeInventory {
    RuntimeInventory {
        generation: 1,
        has_snapshot: true,
        observed_at: Some(Timestamp("2026-09-05T00:00:00Z".into())),
        compose_observation_groups: groups,
        containers: vec![container("other"), container("matching")],
        ..RuntimeInventory::unavailable()
    }
}

fn group(
    project: &str,
    working_directory: &str,
    config_files: &[&str],
    container_ids: &[&str],
) -> ComposeObservationGroup {
    ComposeObservationGroup {
        compose_project_name: project.into(),
        working_directory: Some(working_directory.into()),
        config_files: config_files
            .iter()
            .map(|path| {
                if path.starts_with('/') {
                    (*path).into()
                } else {
                    format!("{working_directory}/{path}")
                }
            })
            .collect(),
        container_ids: container_ids
            .iter()
            .map(|id| ContainerId((*id).into()))
            .collect(),
    }
}

fn container(id: &str) -> ContainerInstance {
    ContainerInstance {
        id: ContainerId(id.into()),
        name: id.into(),
        image: "image".into(),
        state: ContainerState::Running,
        status_text: "Up".into(),
        service_name: None,
        published_ports: vec![],
    }
}
