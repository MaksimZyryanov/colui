use crate::{dto::*, AppState};
use colui_app::GetProjectStatus;
use colui_domain::{AppError, AppErrorCode, ProfileId, RuntimeSessionState};
use tauri::State;

pub(crate) fn map_runtime_state(
    state: RuntimeSessionState,
    operation: &str,
) -> Result<RuntimeStateDto, AppErrorDto> {
    match state {
        RuntimeSessionState::ContextMismatch(details) => Err(AppErrorDto::from(
            AppError::new(
                AppErrorCode::RuntimeContextMismatch,
                operation,
                None,
                "Runtime context mismatch",
            )
            .with_details(format!("Runtime endpoint: {}", details.endpoint.as_str())),
        )),
        RuntimeSessionState::Failed(error) => Err(error.into()),
        state => Ok(state.into()),
    }
}

#[tauri::command]
pub async fn get_runtime_state(state: State<'_, AppState>) -> Result<RuntimeStateDto, AppErrorDto> {
    map_runtime_state(
        state
            .runtime
            .session_state()
            .await
            .map_err(AppErrorDto::from)?,
        "get_runtime_state",
    )
}
#[tauri::command]
pub async fn connect_runtime(state: State<'_, AppState>) -> Result<RuntimeStateDto, AppErrorDto> {
    map_runtime_state(
        state
            .runtime
            .connect_runtime(None)
            .await
            .map_err(AppErrorDto::from)?,
        "connect_runtime",
    )
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
        let result = map_runtime_state(
            RuntimeSessionState::Failed(AppError::new(
                AppErrorCode::RuntimeUnavailable,
                "connect_runtime",
                None,
                "runtime unavailable",
            )),
            "connect_runtime",
        );
        assert_eq!(
            result.unwrap_err().code,
            super::AppErrorCodeDto::RuntimeUnavailable
        );
    }

    #[test]
    fn maps_context_mismatch_to_typed_error() {
        let mismatch = colui_domain::MismatchDetails::new(
            "mock://runtime".try_into().unwrap(),
            colui_domain::DaemonFingerprint::new("api", "1", "test", "test"),
            colui_domain::DaemonFingerprint::new("cli", "2", "test", "test"),
        );
        let result = map_runtime_state(
            RuntimeSessionState::ContextMismatch(mismatch),
            "get_runtime_state",
        );
        let error = result.unwrap_err();
        assert_eq!(error.code, super::AppErrorCodeDto::RuntimeContextMismatch);
        assert_eq!(error.operation, "get_runtime_state");
    }
}
