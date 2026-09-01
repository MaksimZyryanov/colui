use colui_app::{IdGenerator, ProfileStore};
use colui_domain::{
    AppError, AppErrorCode, ComposeProjectName, DisplayName, ProfileDraft, ProjectProfile,
    RegistrationOrigin,
};
use serde::Deserialize;
use std::fs;
use std::io;
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

pub async fn import_v1_if_needed<G: IdGenerator>(
    registry: &crate::registry::format::JsonProfileRegistry,
    legacy_path: &std::path::Path,
    backup_path: &std::path::Path,
    ids: &G,
) -> Result<ImportDiagnostics, AppError> {
    if registry.config().canonical_path.exists() {
        return Ok(ImportDiagnostics {
            imported_profiles: 0,
            error: None,
        });
    }
    let source = match fs::read(legacy_path) {
        Ok(source) => source,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(ImportDiagnostics {
                imported_profiles: 0,
                error: None,
            })
        }
        Err(error) => return Err(io_error("read_legacy_registry", error)),
    };
    fs::write(backup_path, &source).map_err(|error| io_error("backup_legacy_registry", error))?;
    match import_v1(&source, ids) {
        Ok(profiles) => {
            let count = profiles.len();
            registry
                .mutate(Box::new(move |mut snapshot| {
                    snapshot.profiles = profiles;
                    Ok(snapshot)
                }))
                .await?;
            Ok(ImportDiagnostics {
                imported_profiles: count,
                error: None,
            })
        }
        Err(error) => {
            let registry =
                crate::registry::format::JsonProfileRegistry::new(registry.config().clone())?;
            tokio::task::spawn_blocking(move || registry.initialize_empty())
                .await
                .map_err(|join| {
                    AppError::new(
                        AppErrorCode::RegistryWriteFailed,
                        "initialize_registry",
                        None,
                        join.to_string(),
                    )
                })??;
            Ok(ImportDiagnostics {
                imported_profiles: 0,
                error: Some(error),
            })
        }
    }
}

fn io_error(operation: &str, error: io::Error) -> AppError {
    AppError::new(
        AppErrorCode::RegistryWriteFailed,
        operation,
        None,
        "legacy registry import failed",
    )
    .with_details(error.to_string())
}

fn normalized_name(
    name: &str,
    id: &colui_domain::ProfileId,
) -> Result<ComposeProjectName, AppError> {
    let mut normalized = String::new();
    let mut separator = false;
    for byte in name.bytes() {
        if byte.is_ascii_alphanumeric() {
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
