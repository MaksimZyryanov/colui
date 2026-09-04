use crate::{dto::*, AppState};
use tauri::State;

#[tauri::command]
pub async fn get_inventory(state: State<'_, AppState>) -> Result<RuntimeInventoryDto, AppErrorDto> {
    Ok(state
        .inventory
        .refresh_automatic()
        .await
        .map_err(AppErrorDto::from)?
        .into())
}

#[tauri::command]
pub async fn refresh_inventory(
    state: State<'_, AppState>,
) -> Result<RuntimeInventoryDto, AppErrorDto> {
    Ok(state
        .inventory
        .refresh()
        .await
        .map_err(AppErrorDto::from)?
        .into())
}
