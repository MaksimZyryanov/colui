use crate::{dto::*, AppState};
use colui_app::GetProjectStatus;
use colui_domain::{AppError, AppErrorCode, ProfileId, RuntimeSessionState};
use tauri::State;

fn map_runtime_state(state: RuntimeSessionState) -> Result<RuntimeStateDto, AppError> {
    match state {
        RuntimeSessionState::Failed(error) => Err(error),
        state => Ok(state.into()),
    }
}

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
    use colui_app::JournalEventKind as K;
    use ApplicationStateScopeDto as S;
    state
        .journaled(
            [K::ConnectStarted, K::ConnectSucceeded, K::ConnectFailed],
            None,
            &[S::Runtime, S::Discovery, S::Diagnostics],
            async {
                let runtime = state.runtime.connect_runtime(None).await?;
                map_runtime_state(runtime)
            },
        )
        .await
}

#[tauri::command]
pub async fn disconnect_runtime(
    state: State<'_, AppState>,
) -> Result<RuntimeStateDto, AppErrorDto> {
    use colui_app::JournalEventKind as K;
    use ApplicationStateScopeDto as S;
    state
        .journaled(
            [
                K::DisconnectStarted,
                K::DisconnectSucceeded,
                K::DisconnectFailed,
            ],
            None,
            &[S::Runtime, S::Discovery, S::Diagnostics],
            async {
                state.runtime.disconnect_runtime().await?;
                Ok(RuntimeStateDto::Disconnected)
            },
        )
        .await
}

#[tauri::command]
pub async fn reconnect_runtime(
    state: State<'_, AppState>,
) -> Result<ReconnectResultDto, AppErrorDto> {
    use colui_app::JournalEventKind as K;
    use ApplicationStateScopeDto as S;
    state
        .journaled(
            [
                K::ReconnectStarted,
                K::ReconnectSucceeded,
                K::ReconnectFailed,
            ],
            None,
            &[S::Runtime, S::Discovery, S::Diagnostics],
            async {
                colui_app::ReconnectRuntime::new(state.runtime.as_ref(), state.inventory.as_ref())
                    .execute(None)
                    .await
                    .map(Into::into)
            },
        )
        .await
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

#[cfg(test)]
mod tests {
    use super::map_runtime_state;
    use colui_domain::{AppError, AppErrorCode, RuntimeSessionState};

    #[test]
    fn maps_failed_runtime_state_to_typed_error() {
        let result = map_runtime_state(RuntimeSessionState::Failed(AppError::new(
            AppErrorCode::RuntimeUnavailable,
            "connect_runtime",
            None,
            "runtime unavailable",
        )));
        assert_eq!(result.unwrap_err().code, AppErrorCode::RuntimeUnavailable);
    }

    #[test]
    fn maps_context_mismatch_to_structured_state() {
        let mismatch = colui_domain::MismatchDetails::new(
            "mock://runtime".try_into().unwrap(),
            colui_domain::DaemonFingerprint::new("api", "1", "test", "test"),
            colui_domain::DaemonFingerprint::new("cli", "2", "test", "test"),
        );
        let result = map_runtime_state(RuntimeSessionState::ContextMismatch(mismatch));
        let value = serde_json::to_value(result.unwrap()).unwrap();
        assert_eq!(value["state"], "contextMismatch");
        assert_eq!(value["details"]["apiFingerprint"]["daemonId"], "api");
        assert_eq!(value["details"]["cliFingerprint"]["daemonId"], "cli");
    }
}
