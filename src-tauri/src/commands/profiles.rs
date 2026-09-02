use crate::{dto::*, AppState};
use colui_app::{
    CreateProfile, GetProfile, InspectProfileDraft, ListProfiles, RemoveProfile, UpdateProfile,
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
pub async fn list_profiles(
    state: State<'_, AppState>,
) -> Result<Vec<ProfileSummaryDto>, AppErrorDto> {
    Ok(ListProfiles::new(state.profiles.as_ref())
        .execute()
        .await
        .map_err(AppErrorDto::from)?
        .into_iter()
        .map(Into::into)
        .collect())
}
#[tauri::command]
pub async fn get_profile(
    request: ProfileIdRequestDto,
    state: State<'_, AppState>,
) -> Result<ProfileDetailsDto, AppErrorDto> {
    Ok(GetProfile::new(state.profiles.as_ref())
        .execute(id(request.profile_id, "get_profile")?)
        .await
        .map_err(AppErrorDto::from)?
        .into())
}
#[tauri::command]
pub async fn inspect_profile_draft(
    draft: ProfileDraftDto,
    _: State<'_, AppState>,
) -> Result<ProfileValidationDto, AppErrorDto> {
    let draft = match draft.into_domain() {
        Ok(draft) => draft,
        Err(error) => {
            return Ok(ProfileValidationDto {
                valid: false,
                issues: vec![IssueDto {
                    message: error.message,
                }],
            })
        }
    };
    let validation = InspectProfileDraft::execute(draft);
    Ok(ProfileValidationDto {
        valid: validation.valid,
        issues: validation
            .issues
            .into_iter()
            .map(|issue| IssueDto {
                message: issue.message,
            })
            .collect(),
    })
}
#[tauri::command]
pub async fn create_profile(
    draft: ProfileDraftDto,
    state: State<'_, AppState>,
) -> Result<ProfileSummaryDto, AppErrorDto> {
    Ok(
        CreateProfile::new(state.profiles.as_ref(), state.ids.as_ref())
            .execute(draft.into_domain().map_err(AppErrorDto::from)?)
            .await
            .map_err(AppErrorDto::from)?
            .into(),
    )
}
#[tauri::command]
pub async fn update_profile(
    request: UpdateProfileRequestDto,
    state: State<'_, AppState>,
) -> Result<ProfileSummaryDto, AppErrorDto> {
    Ok(UpdateProfile::new(state.profiles.as_ref())
        .execute(
            id(request.profile_id, "update_profile")?,
            request.expected_revision,
            request.patch.into_domain().map_err(AppErrorDto::from)?,
        )
        .await
        .map_err(AppErrorDto::from)?
        .into())
}
#[tauri::command]
pub async fn remove_profile(
    request: RemoveProfileRequestDto,
    state: State<'_, AppState>,
) -> Result<(), AppErrorDto> {
    RemoveProfile::new(state.profiles.as_ref())
        .execute(
            id(request.profile_id, "remove_profile")?,
            request.expected_revision,
        )
        .await
        .map_err(AppErrorDto::from)
}
