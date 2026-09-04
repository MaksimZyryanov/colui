use crate::{dto::*, AppState};
use colui_app::{DefinitionReader, DefinitionRefresher, GetProfile, GetProjectStatus};
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
    let profile = GetProfile::new(state.profiles.as_ref())
        .execute(id(request.profile_id, "get_project_details")?)
        .await
        .map_err(AppErrorDto::from)?;
    let definition = state
        .definitions
        .definition(profile.clone())
        .await
        .map_err(AppErrorDto::from)?;
    let status = GetProjectStatus::new(state.profiles.as_ref(), state.runtime.as_ref())
        .execute(profile.id.clone())
        .await
        .map_err(AppErrorDto::from)?;
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
