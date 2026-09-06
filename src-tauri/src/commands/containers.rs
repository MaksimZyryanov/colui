use crate::{dto::*, AppState};
use colui_app::{JournalEventKind as K, OpenContainerPort, ReadContainerLogs, RunContainerAction};
use colui_domain::{AppError, AppErrorCode, AppErrorSubject, AppErrorSubjectKind, ContainerId};
use tauri::State;

#[tauri::command]
pub async fn run_container_action(
    request: ContainerActionRequestDto,
    state: State<'_, AppState>,
) -> Result<ContainerActionResultDto, AppErrorDto> {
    let session = decode_session_id(request.runtime_session_id).map_err(AppErrorDto::from)?;
    let subject = AppErrorSubject {
        kind: AppErrorSubjectKind::Container,
        id: request.container_id.clone(),
    };
    state
        .journaled(
            [
                K::ContainerActionStarted,
                K::ContainerActionSucceeded,
                K::ContainerActionFailed,
            ],
            Some(subject),
            &[
                ApplicationStateScopeDto::Discovery,
                ApplicationStateScopeDto::Diagnostics,
            ],
            async {
                RunContainerAction::new(
                    state.gateway.as_ref(),
                    state.inventory.as_ref(),
                    state.locks.as_ref(),
                )
                .execute(
                    ContainerId(request.container_id),
                    request.action.into(),
                    session,
                )
                .await
                .map(Into::into)
            },
        )
        .await
}

#[tauri::command]
pub async fn get_container_logs(
    request: ContainerLogsRequestDto,
    state: State<'_, AppState>,
) -> Result<ContainerLogsDto, AppErrorDto> {
    ReadContainerLogs::new(state.gateway.as_ref(), state.inventory.as_ref())
        .execute(colui_app::ContainerLogsRequest {
            container_id: ContainerId(request.container_id),
            runtime_session_id: decode_session_id(request.runtime_session_id)
                .map_err(AppErrorDto::from)?,
        })
        .await
        .map(Into::into)
        .map_err(Into::into)
}

struct TauriBrowserOpener;
impl colui_app::BrowserOpener for TauriBrowserOpener {
    fn open(&self, url: &str) -> Result<(), AppError> {
        tauri_plugin_opener::open_url(url, None::<&str>).map_err(|_| {
            AppError::new(
                AppErrorCode::ContainerOperationFailed,
                "open_container_port",
                None,
                "Browser open failed",
            )
        })
    }
}

#[tauri::command]
pub async fn open_container_port(
    request: OpenContainerPortRequestDto,
    state: State<'_, AppState>,
) -> Result<(), AppErrorDto> {
    OpenContainerPort::new(
        state.gateway.as_ref(),
        state.inventory.as_ref(),
        &TauriBrowserOpener,
    )
    .execute(
        ContainerId(request.container_id),
        decode_session_id(request.runtime_session_id).map_err(AppErrorDto::from)?,
        request.binding_index as usize,
    )
    .await
    .map_err(Into::into)
}
