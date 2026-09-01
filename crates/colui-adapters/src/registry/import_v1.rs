use colui_app::IdGenerator;
use colui_domain::{
    AppError, AppErrorCode, ComposeProjectName, DisplayName, ProfileDraft, ProjectProfile,
    RegistrationOrigin,
};
use serde::Deserialize;
use std::path::PathBuf;

#[derive(Deserialize)]
struct LegacyProject {
    name: String,
    working_dir: PathBuf,
    config_files: Vec<PathBuf>,
    #[serde(default)]
    env_files: Vec<PathBuf>,
}

pub fn import_v1(source: &[u8], ids: &impl IdGenerator) -> Result<Vec<ProjectProfile>, AppError> {
    let projects: Vec<LegacyProject> = serde_json::from_slice(source).map_err(|error| {
        AppError::new(
            AppErrorCode::RegistryCorrupt,
            "import_v1",
            None,
            "legacy registry is malformed",
        )
        .with_details(error.to_string())
    })?;

    projects
        .into_iter()
        .map(|project| {
            let id = ids.generate();
            let compose_project_name = normalized_name(&project.name, &id)?;
            ProjectProfile::from_draft(
                id,
                ProfileDraft {
                    display_name: DisplayName::try_from(project.name).map_err(|error| {
                        AppError::new(AppErrorCode::ProfileInvalid, "import_v1", None, error)
                    })?,
                    compose_project_name,
                    working_directory: project.working_dir,
                    compose_files: project.config_files,
                    environment_files: project.env_files,
                    registration_origin: RegistrationOrigin::Migrated,
                },
            )
        })
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ImportDiagnostics {
    pub imported_profiles: usize,
    pub error: Option<AppError>,
}

pub async fn import_v1_if_needed<G: IdGenerator + Clone + Send + Sync + 'static>(
    registry: &crate::registry::format::JsonProfileRegistry,
    legacy_path: &std::path::Path,
    backup_path: &std::path::Path,
    ids: &G,
) -> Result<ImportDiagnostics, AppError> {
    let config = registry.config().clone();
    let legacy_path = legacy_path.to_owned();
    let backup_path = backup_path.to_owned();
    let ids = ids.clone();
    let result = tokio::task::spawn_blocking(move || {
        let registry = crate::registry::format::JsonProfileRegistry::new(config)?;
        registry.run_import(&legacy_path, &backup_path, |source| import_v1(source, &ids))
    })
    .await
    .map_err(|error| {
        AppError::new(
            AppErrorCode::RegistryWriteFailed,
            "import_v1",
            None,
            error.to_string(),
        )
    })??;
    match result {
        crate::registry::format::ImportResult::Skipped => Ok(ImportDiagnostics {
            imported_profiles: 0,
            error: None,
        }),
        crate::registry::format::ImportResult::Imported(count) => Ok(ImportDiagnostics {
            imported_profiles: count,
            error: None,
        }),
        crate::registry::format::ImportResult::Malformed(error) => Ok(ImportDiagnostics {
            imported_profiles: 0,
            error: Some(error),
        }),
    }
}

fn normalized_name(
    name: &str,
    id: &colui_domain::ProfileId,
) -> Result<ComposeProjectName, AppError> {
    let mut normalized = String::new();
    let mut separator = false;
    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric() || byte == b'_' {
            if separator && !normalized.is_empty() {
                normalized.push('-');
            }
            normalized.push(byte.to_ascii_lowercase() as char);
            separator = false;
        } else {
            separator = true;
        }
    }
    let value = if normalized.is_empty() {
        format!("imported-{}", &id.to_string()[..8])
    } else {
        normalized
    };
    ComposeProjectName::try_from(value).map_err(|error| {
        AppError::new(
            AppErrorCode::ProfileInvalid,
            "import_v1",
            Some(id.clone()),
            error,
        )
    })
}
