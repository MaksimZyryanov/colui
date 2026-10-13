use crate::{dto::*, AppState};
use colui_app::{
    profile_association_is_unique, project_status_from_registry_inventory_and_definition,
    DefinitionReader, DefinitionRefresher, GetProfile, InventoryReader, ProfileReader,
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
    let profile_id = id(request.profile_id, "get_project_details")?;
    let registry = profiles.load().await.map_err(AppErrorDto::from)?;
    let profile = registry
        .profiles
        .iter()
        .find(|profile| profile.id == profile_id)
        .cloned()
        .ok_or_else(|| {
            AppErrorDto::from(AppError::new(
                AppErrorCode::ProfileNotFound,
                "get_project_details",
                Some(profile_id),
                "profile not found",
            ))
        })?;
    let definition = definitions
        .definition(profile.clone())
        .await
        .map_err(AppErrorDto::from)?;
    let inventory = inventory
        .current_inventory()
        .await
        .map_err(AppErrorDto::from)?;
    let status =
        project_status_from_registry_inventory_and_definition(&profile, &registry, inventory, None);
    let status = ProjectStatusDto::from(status);
    let profile_association_unique = profile_association_is_unique(&profile, &registry);
    Ok(ProjectDetailsResponseDto {
        profile: profile.into(),
        definition: definition.into(),
        runtime: status.runtime,
        runtime_issues: status.issues,
        profile_association_unique,
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
        ComposeObservationGroup, ContainerId, ContainerInstance, ContainerState,
        DefinitionRevision, DefinitionState, DisplayName, InventoryFreshness, Issue, ProfileDraft,
        ProjectDefinition, ProjectProfile, RegistrationOrigin, RuntimeInventory, Timestamp,
    };
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct Profiles(Vec<ProjectProfile>);

    impl ProfileReader for Profiles {
        fn load(&self) -> StoreFuture<'_, RegistrySnapshot> {
            let profiles = self.0.clone();
            Box::pin(async move {
                Ok(RegistrySnapshot {
                    registry_revision: 1,
                    profiles,
                })
            })
        }
    }

    struct FailedDefinitions {
        calls: AtomicUsize,
        issues: Vec<Issue>,
    }

    impl DefinitionReader for FailedDefinitions {
        fn definition(
            &self,
            profile: ProjectProfile,
        ) -> DefinitionFuture<'_, DefinitionProjection> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let issues = self.issues.clone();
            Box::pin(async move {
                Ok(DefinitionProjection {
                    definition: ProjectDefinition {
                        profile_id: profile.id.clone(),
                        definition_revision: DefinitionRevision(String::new()),
                        loaded_at: Timestamp(String::new()),
                        state: DefinitionState::Unchecked,
                        services: vec![],
                        issues,
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
                issues: vec![],
            };

            let details = get_project_details_inner(
                ProfileIdRequestDto {
                    profile_id: profile.id.to_string(),
                },
                &Profiles(vec![profile]),
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

    struct ObservedInventory(RuntimeInventory);

    impl InventoryReader for ObservedInventory {
        fn current_inventory(&self) -> InventoryFuture<'_, RuntimeInventory> {
            let inventory = self.0.clone();
            Box::pin(async move { Ok(inventory) })
        }
    }

    #[test]
    fn project_details_reports_ambiguous_runtime_without_duplicating_definition_issues() {
        tauri::async_runtime::block_on(async {
            let first = profile();
            let mut duplicate = first.clone();
            duplicate.id = ProfileId::parse("00000000-0000-0000-0000-000000000002").unwrap();
            let definitions = FailedDefinitions {
                calls: AtomicUsize::new(0),
                issues: vec![Issue {
                    field: Some("services.web".into()),
                    message: "Definition issue".into(),
                }],
            };
            let inventory = ObservedInventory(RuntimeInventory {
                generation: 1,
                has_snapshot: true,
                observed_at: Some(Timestamp("2026-10-13T00:00:00Z".into())),
                freshness: InventoryFreshness::Fresh,
                containers: vec![ContainerInstance {
                    id: ContainerId("web".into()),
                    name: "demo-web-1".into(),
                    image: "alpine:3.20".into(),
                    state: ContainerState::Running,
                    status_text: "Up".into(),
                    service_name: Some("web".into()),
                    published_ports: vec![],
                }],
                compose_observation_groups: vec![ComposeObservationGroup {
                    compose_project_name: "demo".into(),
                    working_directory: Some("/tmp/demo".into()),
                    config_files: vec!["/tmp/demo/compose.yml".into()],
                    container_ids: vec![ContainerId("web".into())],
                }],
                ..RuntimeInventory::unavailable()
            });
            let details = get_project_details_inner(
                ProfileIdRequestDto {
                    profile_id: first.id.to_string(),
                },
                &Profiles(vec![first, duplicate]),
                &definitions,
                &inventory,
            )
            .await
            .unwrap();

            assert_eq!(details.runtime.container_count, 0);
            let value = serde_json::to_value(details).unwrap();
            assert_eq!(value["profileAssociationUnique"], false);
            assert_eq!(
                value["runtimeIssues"],
                serde_json::json!([{
                    "field": "ambiguous_runtime_association",
                    "message": "runtime association is ambiguous",
                }])
            );
            assert_eq!(
                value["definition"]["issues"],
                serde_json::json!([{
                    "field": "services.web", "message": "Definition issue",
                }])
            );
        });
    }

    #[test]
    fn project_details_registry_uniqueness_is_independent_of_runtime_groups() {
        tauri::async_runtime::block_on(async {
            let first = profile();
            let mut duplicate = first.clone();
            duplicate.id = ProfileId::parse("00000000-0000-0000-0000-000000000002").unwrap();
            duplicate.working_directory = "/tmp/other/../demo".into();
            duplicate.compose_files = vec!["/tmp/demo/./compose.yml".into()];
            let definitions = FailedDefinitions {
                calls: AtomicUsize::new(0),
                issues: vec![],
            };
            let inventory = ObservedInventory(RuntimeInventory {
                generation: 1,
                has_snapshot: true,
                compose_observation_groups: ["/tmp/demo", "/tmp/other"]
                    .into_iter()
                    .map(|directory| ComposeObservationGroup {
                        compose_project_name: "demo".into(),
                        working_directory: Some(directory.into()),
                        config_files: vec![format!("{directory}/compose.yml")],
                        container_ids: vec![],
                    })
                    .collect(),
                ..RuntimeInventory::unavailable()
            });
            for (profiles, unique) in [
                (vec![first.clone()], true),
                (vec![first.clone(), duplicate], false),
            ] {
                let details = get_project_details_inner(
                    ProfileIdRequestDto {
                        profile_id: first.id.to_string(),
                    },
                    &Profiles(profiles),
                    &definitions,
                    &inventory,
                )
                .await
                .unwrap();
                let value = serde_json::to_value(details).unwrap();
                assert_eq!(value["profileAssociationUnique"], unique);
                assert_eq!(
                    value["runtimeIssues"][0]["field"],
                    "ambiguous_runtime_association"
                );
            }
        });
    }
}
