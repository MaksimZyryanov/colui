use crate::error::{AppError, AppErrorCode};
use crate::{ComposeProjectName, DisplayName, ProfileId, Revision};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RegistrationOrigin {
    Manual,
    Discovered,
    Migrated,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProfileDraft {
    pub display_name: DisplayName,
    pub compose_project_name: ComposeProjectName,
    pub working_directory: PathBuf,
    pub compose_files: Vec<PathBuf>,
    pub environment_files: Vec<PathBuf>,
    pub registration_origin: RegistrationOrigin,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProjectProfile {
    pub id: ProfileId,
    pub revision: Revision,
    pub display_name: DisplayName,
    pub compose_project_name: ComposeProjectName,
    pub working_directory: PathBuf,
    pub compose_files: Vec<PathBuf>,
    pub environment_files: Vec<PathBuf>,
    pub registration_origin: RegistrationOrigin,
}

impl ProjectProfile {
    pub fn id(&self) -> &ProfileId {
        &self.id
    }
    pub fn revision(&self) -> Revision {
        self.revision
    }
    pub fn display_name(&self) -> &DisplayName {
        &self.display_name
    }
    pub fn compose_project_name(&self) -> &ComposeProjectName {
        &self.compose_project_name
    }
    pub fn working_directory(&self) -> &PathBuf {
        &self.working_directory
    }
    pub fn compose_files(&self) -> &[PathBuf] {
        &self.compose_files
    }
    pub fn environment_files(&self) -> &[PathBuf] {
        &self.environment_files
    }
    pub fn registration_origin(&self) -> &RegistrationOrigin {
        &self.registration_origin
    }
    pub fn from_draft(id: ProfileId, draft: ProfileDraft) -> Result<Self, AppError> {
        validate_draft(&draft)?;
        Ok(Self {
            id,
            revision: Revision::initial(),
            display_name: draft.display_name,
            compose_project_name: draft.compose_project_name,
            working_directory: draft.working_directory,
            compose_files: draft.compose_files,
            environment_files: draft.environment_files,
            registration_origin: draft.registration_origin,
        })
    }

    pub fn with_display_name(&self, display_name: DisplayName) -> Result<Self, AppError> {
        let mut renamed = self.clone();
        renamed.display_name = display_name;
        renamed.revision = self.revision.next()?;
        Ok(renamed)
    }
}

pub fn validate_draft(draft: &ProfileDraft) -> Result<(), AppError> {
    if draft.compose_files.is_empty() {
        return Err(AppError::new(
            AppErrorCode::ProfileInvalid,
            "validate_profile",
            None,
            "profile requires at least one Compose file",
        ));
    }

    let mut paths = HashSet::new();
    if draft
        .compose_files
        .iter()
        .chain(draft.environment_files.iter())
        .any(|path| !paths.insert(path))
    {
        return Err(AppError::new(
            AppErrorCode::ProfileInvalid,
            "validate_profile",
            None,
            "profile paths must be unique",
        ));
    }

    Ok(())
}
