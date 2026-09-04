use crate::{dto::*, AppState};
use colui_app::{
    project_status_from_inventory_and_definition_projection, DefinitionReader, DefinitionRefresher,
    GetProfile, InventoryReader, ProfileReader,
};
use colui_domain::{AppError, AppErrorCode, ProfileId};
use tauri::State;

fn id(value: String, operation: &str) -> Result<ProfileId, AppErrorDto> {
    ProfileId::parse(&value).map_err(|_| {
        AppError::new(
            AppErrorCode::ProfileInvalid,
            operation,
            None,
            "profileId must be a UUID",
        )
        .into()
    })
}

#[tauri::command]
pub async fn get_project_details(
    request: ProfileIdRequestDto,
    state: State<'_, AppState>,
) -> Result<ProjectDetailsResponseDto, AppErrorDto> {
    get_project_details_inner(
        request,
        state.profiles.as_ref(),
        state.definitions.as_ref(),
        state.inventory.as_ref(),
    )
    .await
}

async fn get_project_details_inner<P, D, I>(
    request: ProfileIdRequestDto,
    profiles: &P,
    definitions: &D,
    inventory: &I,
) -> Result<ProjectDetailsResponseDto, AppErrorDto>
where
    P: ProfileReader + ?Sized,
    D: DefinitionReader + ?Sized,
    I: InventoryReader + ?Sized,
{
    let profile = GetProfile::new(profiles)
        .execute(id(request.profile_id, "get_project_details")?)
        .await
        .map_err(AppErrorDto::from)?;
    let definition = definitions
        .definition(profile.clone())
        .await
        .map_err(AppErrorDto::from)?;
    let inventory = inventory
        .current_inventory()
        .await
        .map_err(AppErrorDto::from)?;
    let status =
        project_status_from_inventory_and_definition_projection(&profile, inventory, &definition);
    Ok(ProjectDetailsResponseDto {
        profile: profile.into(),
        definition: definition.into(),
        runtime: ProjectStatusDto::from(status).runtime,
    })
}

#[tauri::command]
pub async fn refresh_project_definition(
    request: ProfileIdRequestDto,
    state: State<'_, AppState>,
) -> Result<ProjectDefinitionDto, AppErrorDto> {
    let profile = GetProfile::new(state.profiles.as_ref())
        .execute(id(request.profile_id, "refresh_project_definition")?)
        .await
        .map_err(AppErrorDto::from)?;
    Ok(state
        .definitions
        .refresh_definition(profile)
        .await
        .map_err(AppErrorDto::from)?
        .into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use colui_app::{
        DefinitionFuture, DefinitionProjection, InventoryFuture, InventoryReader, ProfileReader,
        RegistrySnapshot, StoreFuture,
    };
    use colui_domain::{
        DefinitionRevision, DefinitionState, DisplayName, ProfileDraft, ProjectDefinition,
        ProjectProfile, RegistrationOrigin, RuntimeInventory, Timestamp,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

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

    struct FailedDefinitions {
        calls: AtomicUsize,
    }

    impl DefinitionReader for FailedDefinitions {
        fn definition(
            &self,
            profile: ProjectProfile,
        ) -> DefinitionFuture<'_, DefinitionProjection> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            Box::pin(async move {
                Ok(DefinitionProjection {
                    definition: ProjectDefinition {
                        profile_id: profile.id.clone(),
                        definition_revision: DefinitionRevision(String::new()),
                        loaded_at: Timestamp(String::new()),
                        state: DefinitionState::Unchecked,
                        services: vec![],
                        issues: vec![],
                    },
                    error: Some(AppError::new(
                        AppErrorCode::DefinitionFailed,
                        "definition",
                        Some(profile.id),
                        "Definition load failed",
                    )),
                })
            })
        }
    }

    struct Inventory;

    impl InventoryReader for Inventory {
        fn current_inventory(&self) -> InventoryFuture<'_, RuntimeInventory> {
            Box::pin(async { Ok(RuntimeInventory::unavailable()) })
        }
    }

    fn profile() -> ProjectProfile {
        ProjectProfile::from_draft(
            ProfileId::parse("00000000-0000-0000-0000-000000000001").unwrap(),
            ProfileDraft {
                display_name: DisplayName::try_from("Demo").unwrap(),
                compose_project_name: "demo".try_into().unwrap(),
                working_directory: "/tmp/demo".into(),
                compose_files: vec!["compose.yml".into()],
                environment_files: vec![],
                registration_origin: RegistrationOrigin::Manual,
            },
        )
        .unwrap()
    }

    #[test]
    fn project_details_reads_failed_definition_once() {
        tauri::async_runtime::block_on(async {
            let profile = profile();
            let definitions = FailedDefinitions {
                calls: AtomicUsize::new(0),
            };

            let details = get_project_details_inner(
                ProfileIdRequestDto {
                    profile_id: profile.id.to_string(),
                },
                &Profiles(profile),
                &definitions,
                &Inventory,
            )
            .await
            .unwrap();

            assert_eq!(definitions.calls.load(Ordering::SeqCst), 1);
            assert_eq!(details.definition.state, DefinitionStateDto::Unchecked);
            assert_eq!(
                details.definition.error.unwrap().code,
                AppErrorCodeDto::DefinitionFailed
            );
            assert_eq!(details.runtime.presence, RuntimePresenceDto::Unavailable);
            assert_eq!(details.runtime.container_count, 0);
        });
    }
}
