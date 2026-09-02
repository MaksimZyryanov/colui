use crate::{dto::*, AppState};
use colui_app::GetProjectStatus;
use colui_domain::{AppError, AppErrorCode, ProfileId};
use tauri::State;

#[tauri::command]
pub async fn get_runtime_state(state: State<'_, AppState>) -> Result<RuntimeStateDto, AppErrorDto> {
    Ok(state
        .runtime
        .session_state()
        .await
        .map_err(AppErrorDto::from)?
        .into())
}
#[tauri::command]
pub async fn connect_runtime(state: State<'_, AppState>) -> Result<RuntimeStateDto, AppErrorDto> {
    Ok(state
        .runtime
        .connect_runtime(None)
        .await
        .map_err(AppErrorDto::from)?
        .into())
}
#[tauri::command]
pub async fn get_project_status(
    request: ProfileIdRequestDto,
    state: State<'_, AppState>,
) -> Result<ProjectStatusDto, AppErrorDto> {
    let id = ProfileId::parse(&request.profile_id).map_err(|_| {
        AppErrorDto::from(AppError::new(
            AppErrorCode::ProfileInvalid,
            "get_project_status",
            None,
            "profileId must be a UUID",
        ))
    })?;
    Ok(
        GetProjectStatus::new(state.profiles.as_ref(), state.runtime.as_ref())
            .execute(id)
            .await
            .map_err(AppErrorDto::from)?
            .into(),
    )
}
