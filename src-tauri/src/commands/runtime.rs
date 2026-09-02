use crate::{dto::*, AppState};
use colui_domain::{AppError, AppErrorCode};
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
    _: State<'_, AppState>,
) -> Result<ProjectStatusDto, AppErrorDto> {
    let _ = request;
    Err(AppError::new(
        AppErrorCode::ProtocolMismatch,
        "get_project_status",
        None,
        "status inspection unavailable",
    )
    .into())
}
