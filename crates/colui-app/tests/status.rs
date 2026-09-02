use colui_app::{
    GetProjectStatus, ProfileReader, ProjectStatus, ProjectStatusFuture, ProjectStatusReader,
    RegistrySnapshot, RuntimeProjection, StoreFuture,
};
use colui_domain::{
    AppError, DefinitionState, ProfileDraft, ProfileId, ProjectProfile, RegistrationOrigin,
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
