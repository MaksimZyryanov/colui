use colui_app::ProfilePatch;
use colui_domain::{
    AppError, AppErrorCode, ComposeProjectName, DisplayName, ProfileDraft, ProjectProfile,
    RegistrationOrigin,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::{convert::TryFrom, path::PathBuf};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub enum RegistrationOriginDto {
    Manual,
    Discovered,
    Migrated,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSummaryDto {
    #[schemars(schema_with = "super::uuid_schema")]
    pub id: String,
    pub revision: u64,
    pub display_name: String,
    pub compose_project_name: String,
    pub working_directory: String,
    pub registration_origin: RegistrationOriginDto,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProfileDetailsDto {
    pub profile: ProfileSummaryDto,
    pub compose_files: Vec<String>,
    pub environment_files: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProfileDraftDto {
    pub display_name: String,
    pub compose_project_name: String,
    pub working_directory: String,
    pub compose_files: Vec<String>,
    pub environment_files: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProfilePatchDto {
    pub display_name: String,
    pub compose_project_name: String,
    pub working_directory: String,
    pub compose_files: Vec<String>,
    pub environment_files: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct IssueDto {
    pub message: String,
}

impl ProfileDraftDto {
    pub fn into_domain(self) -> Result<ProfileDraft, AppError> {
        Ok(ProfileDraft {
            display_name: DisplayName::try_from(self.display_name).map_err(invalid)?,
            compose_project_name: ComposeProjectName::try_from(self.compose_project_name)
                .map_err(invalid)?,
            working_directory: self.working_directory.into(),
            compose_files: self.compose_files.into_iter().map(PathBuf::from).collect(),
            environment_files: self
                .environment_files
                .into_iter()
                .map(PathBuf::from)
                .collect(),
            registration_origin: RegistrationOrigin::Manual,
        })
    }
}

impl ProfilePatchDto {
    pub fn into_domain(self) -> Result<ProfilePatch, AppError> {
        Ok(ProfilePatch {
            display_name: Some(DisplayName::try_from(self.display_name).map_err(invalid)?),
            compose_project_name: Some(
                ComposeProjectName::try_from(self.compose_project_name).map_err(invalid)?,
            ),
            working_directory: Some(self.working_directory.into()),
            compose_files: Some(self.compose_files.into_iter().map(PathBuf::from).collect()),
            environment_files: Some(
                self.environment_files
                    .into_iter()
                    .map(PathBuf::from)
                    .collect(),
            ),
        })
    }
}

impl From<RegistrationOrigin> for RegistrationOriginDto {
    fn from(v: RegistrationOrigin) -> Self {
        match v {
            RegistrationOrigin::Manual => Self::Manual,
            RegistrationOrigin::Discovered => Self::Discovered,
            RegistrationOrigin::Migrated => Self::Migrated,
        }
    }
}
impl From<ProjectProfile> for ProfileSummaryDto {
    fn from(p: ProjectProfile) -> Self {
        Self {
            id: p.id().to_string(),
            revision: p.revision().value(),
            display_name: p.display_name().as_ref().to_owned(),
            compose_project_name: p.compose_project_name().as_ref().to_owned(),
            working_directory: p.working_directory().to_string_lossy().into_owned(),
            registration_origin: p.registration_origin().clone().into(),
        }
    }
}
impl From<ProjectProfile> for ProfileDetailsDto {
    fn from(p: ProjectProfile) -> Self {
        Self {
            profile: p.clone().into(),
            compose_files: p
                .compose_files()
                .iter()
                .map(|v| v.to_string_lossy().into_owned())
                .collect(),
            environment_files: p
                .environment_files()
                .iter()
                .map(|v| v.to_string_lossy().into_owned())
                .collect(),
        }
    }
}
fn invalid(message: impl Into<String>) -> AppError {
    AppError::new(
        AppErrorCode::ProfileInvalid,
        "decode_profile",
        None,
        message,
    )
}
