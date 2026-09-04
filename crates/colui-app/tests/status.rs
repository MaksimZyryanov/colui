use colui_app::{
    project_status_from_inventory, GetProjectStatus, ProfileReader, ProjectStatus,
    ProjectStatusFuture, ProjectStatusReader, RegistrySnapshot, RuntimeProjection, StoreFuture,
};
use colui_domain::{
    AppError, ContainerId, ContainerInstance, ContainerState, DefinitionState, ProfileDraft,
    ProfileId, ProjectProfile, ProjectRuntimeSnapshot, RegistrationOrigin, RuntimeInventory,
    RuntimePresence,
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

struct StatusPort(Mutex<Option<ProfileId>>);

impl ProjectStatusReader for StatusPort {
    fn project_status(&self, profile: ProjectProfile) -> ProjectStatusFuture<'_> {
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
