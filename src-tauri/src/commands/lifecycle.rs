use crate::{dto::*, AppState};
use colui_app::{ApplyProject, RestartProject, StopProject, TearDownProject};
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
macro_rules! command {
    ($name:ident, $use_case:ident) => {
        #[tauri::command]
        pub async fn $name(
            request: ProfileIdRequestDto,
            state: State<'_, AppState>,
        ) -> Result<LifecycleResultDto, AppErrorDto> {
            let result = $use_case::new(state.profiles.as_ref(), state.runtime.as_ref())
                .execute(id(request.profile_id, stringify!($name))?)
                .await
                .map_err(AppErrorDto::from)?;
            Ok(result.into())
        }
    };
}
command!(apply_project, ApplyProject);
command!(stop_project, StopProject);
command!(tear_down_project, TearDownProject);
command!(restart_project, RestartProject);
