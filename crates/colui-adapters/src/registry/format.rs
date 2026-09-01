use colui_app::{ProfileMutation, ProfileReader, ProfileStore, RegistrySnapshot, StoreFuture};
use colui_domain::{validate_draft, AppError, AppErrorCode, ProfileDraft, ProjectProfile};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};
use uuid::Uuid;

#[derive(Clone, Debug)]
pub struct RegistryConfig {
    pub canonical_path: PathBuf,
    pub lock_path: PathBuf,
    pub lock_timeout: Duration,
}

impl RegistryConfig {
    pub fn in_directory(directory: impl AsRef<Path>) -> Self {
        let directory = directory.as_ref();
        Self {
            canonical_path: directory.join("registry.json"),
            lock_path: directory.join("registry.lock"),
            lock_timeout: Duration::from_secs(5),
        }
    }

    pub fn try_with_timeout(mut self, timeout: Duration) -> Result<Self, AppError> {
        if !(Duration::from_secs(5)..=Duration::from_secs(10)).contains(&timeout) {
            return Err(AppError::new(
                AppErrorCode::RegistryWriteFailed,
                "configure_registry_lock",
                None,
                "registry lock timeout must be between five and ten seconds",
            ));
        }
        self.lock_timeout = timeout;
        Ok(self)
    }
}

pub struct JsonProfileRegistry {
    config: RegistryConfig,
}

pub(crate) enum ImportResult {
    Skipped,
    Imported(usize),
    Malformed(AppError),
}

impl JsonProfileRegistry {
    pub fn new(config: RegistryConfig) -> Result<Self, AppError> {
        Ok(Self { config })
    }

    pub fn config(&self) -> &RegistryConfig {
        &self.config
    }

    pub(crate) fn run_import<F>(
        &self,
        legacy_path: &Path,
        backup_path: &Path,
        importer: F,
    ) -> Result<ImportResult, AppError>
    where
        F: FnOnce(&[u8]) -> Result<Vec<ProjectProfile>, AppError>,
    {
        let lock = self.lock()?;
        if self.config.canonical_path.exists() {
            drop(lock);
            return Ok(ImportResult::Skipped);
        }
        let source = match fs::read(legacy_path) {
            Ok(source) => source,
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                drop(lock);
                return Ok(ImportResult::Skipped);
            }
            Err(error) => return Err(io_error("read_legacy_registry", error)),
        };
        atomic_backup(backup_path, &source)?;
        match importer(&source) {
            Ok(profiles) => {
                let count = profiles.len();
                atomic_write(
                    &self.config.canonical_path,
                    &encode(&RegistrySnapshot {
                        registry_revision: 1,
                        profiles,
                    })?,
                )?;
                drop(lock);
                Ok(ImportResult::Imported(count))
            }
            Err(error) => {
                atomic_write(
                    &self.config.canonical_path,
                    &encode(&RegistrySnapshot {
                        registry_revision: 0,
                        profiles: Vec::new(),
                    })?,
                )?;
                drop(lock);
                Ok(ImportResult::Malformed(error))
            }
        }
    }

    fn load_bytes(&self) -> Result<RegistrySnapshot, AppError> {
        match fs::read(&self.config.canonical_path) {
            Ok(bytes) => decode(&bytes),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(RegistrySnapshot {
                registry_revision: 0,
                profiles: Vec::new(),
            }),
            Err(error) => Err(io_error("read_registry", error)),
        }
    }

    fn lock(&self) -> Result<File, AppError> {
        if let Some(parent) = self.config.lock_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| io_error("create_registry_directory", error))?;
        }
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&self.config.lock_path)
            .map_err(|error| io_error("open_registry_lock", error))?;
        let started = Instant::now();
        let mut delay = Duration::from_millis(5);
        loop {
            match file.try_lock_exclusive() {
                Ok(()) => return Ok(file),
                Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                    if started.elapsed() >= self.config.lock_timeout {
                        return Err(AppError::new(
                            AppErrorCode::RegistryLocked,
                            "lock_registry",
                            None,
                            "registry is locked",
                        ));
                    }
                    let remaining = self.config.lock_timeout.saturating_sub(started.elapsed());
                    std::thread::sleep(delay.min(remaining));
                    delay = (delay * 2).min(Duration::from_millis(100));
                }
                Err(error) => return Err(io_error("lock_registry", error)),
            }
        }
    }
}

impl ProfileReader for JsonProfileRegistry {
    fn load(&self) -> StoreFuture<'_, RegistrySnapshot> {
        let config = self.config.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || JsonProfileRegistry { config }.load_bytes())
                .await
                .map_err(|error| write_error("load_registry", error))?
        })
    }
}

impl ProfileStore for JsonProfileRegistry {
    fn mutate(&self, mutation: ProfileMutation) -> StoreFuture<'_, RegistrySnapshot> {
        let config = self.config.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                let registry = JsonProfileRegistry { config };
                let lock = registry.lock()?;
                let before = registry.load_bytes()?;
                let persisted_revision = before.registry_revision;
                let original = encode(&before)?;
                let mut after = mutation(before)?;
                validate_snapshot(&after)?;
                let changed = after.profiles != decode(&original)?.profiles;
                if changed {
                    after.registry_revision =
                        persisted_revision.checked_add(1).ok_or_else(|| {
                            AppError::new(
                                AppErrorCode::RegistryWriteFailed,
                                "write_registry",
                                None,
                                "registry revision exhausted",
                            )
                        })?;
                    let bytes = encode(&after)?;
                    atomic_write(&registry.config.canonical_path, &bytes)?;
                    let reread = registry.load_bytes()?;
                    if reread != after {
                        return Err(AppError::new(
                            AppErrorCode::RegistryWriteFailed,
                            "reread_registry",
                            None,
                            "registry changed during atomic write",
                        ));
                    }
                    drop(lock);
                    return Ok(reread);
                }
                after.registry_revision = persisted_revision;
                drop(lock);
                Ok(after)
            })
            .await
            .map_err(|error| write_error("mutate_registry", error))?
        })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
struct RegistryFile {
    schema_version: u8,
    registry_revision: u64,
    profiles: Vec<RegistryProfile>,
}

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
#[serde(rename_all = "camelCase")]
struct RegistryProfile {
    id: colui_domain::ProfileId,
    revision: colui_domain::Revision,
    display_name: colui_domain::DisplayName,
    compose_project_name: colui_domain::ComposeProjectName,
    working_directory: PathBuf,
    compose_files: Vec<PathBuf>,
    environment_files: Vec<PathBuf>,
    registration_origin: colui_domain::RegistrationOrigin,
}

impl From<&ProjectProfile> for RegistryProfile {
    fn from(profile: &ProjectProfile) -> Self {
        Self {
            id: profile.id.clone(),
            revision: profile.revision,
            display_name: profile.display_name.clone(),
            compose_project_name: profile.compose_project_name.clone(),
            working_directory: profile.working_directory.clone(),
            compose_files: profile.compose_files.clone(),
            environment_files: profile.environment_files.clone(),
            registration_origin: profile.registration_origin.clone(),
        }
    }
}

impl From<RegistryProfile> for ProjectProfile {
    fn from(profile: RegistryProfile) -> Self {
        Self {
            id: profile.id,
            revision: profile.revision,
            display_name: profile.display_name,
            compose_project_name: profile.compose_project_name,
            working_directory: profile.working_directory,
            compose_files: profile.compose_files,
            environment_files: profile.environment_files,
            registration_origin: profile.registration_origin,
        }
    }
}

fn decode(bytes: &[u8]) -> Result<RegistrySnapshot, AppError> {
    let file: RegistryFile = serde_json::from_slice(bytes).map_err(|error| {
        AppError::new(
            AppErrorCode::RegistryCorrupt,
            "load_registry",
            None,
            "registry is corrupt",
        )
        .with_details(error.to_string())
    })?;
    if file.schema_version != 2 {
        return Err(AppError::new(
            AppErrorCode::RegistryCorrupt,
            "load_registry",
            None,
            "unsupported registry schema",
        ));
    }
    let snapshot = RegistrySnapshot {
        registry_revision: file.registry_revision,
        profiles: file
            .profiles
            .into_iter()
            .map(ProjectProfile::from)
            .collect(),
    };
    validate_snapshot(&snapshot)?;
    Ok(snapshot)
}

fn encode(snapshot: &RegistrySnapshot) -> Result<Vec<u8>, AppError> {
    serde_json::to_vec_pretty(&RegistryFile {
        schema_version: 2,
        registry_revision: snapshot.registry_revision,
        profiles: snapshot
            .profiles
            .iter()
            .map(RegistryProfile::from)
            .collect(),
    })
    .map_err(|error| write_error("serialize_registry", error))
}

fn validate_snapshot(snapshot: &RegistrySnapshot) -> Result<(), AppError> {
    let mut ids = std::collections::HashSet::new();
    for profile in &snapshot.profiles {
        if !ids.insert(profile.id.clone()) {
            return Err(AppError::new(
                AppErrorCode::RegistryCorrupt,
                "validate_registry",
                Some(profile.id.clone()),
                "registry contains duplicate profile ID",
            ));
        }
        validate_draft(&ProfileDraft {
            display_name: profile.display_name.clone(),
            compose_project_name: profile.compose_project_name.clone(),
            working_directory: profile.working_directory.clone(),
            compose_files: profile.compose_files.clone(),
            environment_files: profile.environment_files.clone(),
            registration_origin: profile.registration_origin.clone(),
        })?;
    }
    Ok(())
}

fn atomic_write(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let parent = path.parent().ok_or_else(|| {
        AppError::new(
            AppErrorCode::RegistryWriteFailed,
            "write_registry",
            None,
            "registry path has no parent",
        )
    })?;
    fs::create_dir_all(parent).map_err(|error| io_error("create_registry_directory", error))?;
    let temp = parent.join(format!(".registry.{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut file = File::create(&temp).map_err(|error| io_error("write_registry", error))?;
        io::Write::write_all(&mut file, bytes)
            .map_err(|error| io_error("write_registry", error))?;
        file.sync_all()
            .map_err(|error| io_error("fsync_registry", error))?;
        fs::rename(&temp, path).map_err(|error| io_error("rename_registry", error))?;
        let directory =
            File::open(parent).map_err(|error| io_error("open_registry_directory", error))?;
        directory
            .sync_all()
            .map_err(|error| io_error("fsync_registry_directory", error))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn atomic_backup(path: &Path, bytes: &[u8]) -> Result<(), AppError> {
    let parent = path.parent().ok_or_else(|| {
        AppError::new(
            AppErrorCode::RegistryWriteFailed,
            "backup_legacy_registry",
            None,
            "backup path has no parent",
        )
    })?;
    fs::create_dir_all(parent).map_err(|error| io_error("create_backup_directory", error))?;
    let temp = parent.join(format!(".backup.{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut file =
            File::create(&temp).map_err(|error| io_error("backup_legacy_registry", error))?;
        file.write_all(bytes)
            .map_err(|error| io_error("backup_legacy_registry", error))?;
        file.sync_all()
            .map_err(|error| io_error("fsync_legacy_backup", error))?;
        fs::rename(&temp, path).map_err(|error| io_error("rename_legacy_backup", error))?;
        let directory =
            File::open(parent).map_err(|error| io_error("open_backup_directory", error))?;
        directory
            .sync_all()
            .map_err(|error| io_error("fsync_backup_directory", error))
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temp);
    }
    result
}

fn write_error(operation: &str, error: impl std::fmt::Display) -> AppError {
    AppError::new(
        AppErrorCode::RegistryWriteFailed,
        operation,
        None,
        "registry persistence failed",
    )
    .with_details(error.to_string())
}

fn io_error(operation: &str, error: io::Error) -> AppError {
    let code = if error.kind() == io::ErrorKind::PermissionDenied {
        AppErrorCode::PermissionDenied
    } else {
        AppErrorCode::RegistryWriteFailed
    };
    AppError::new(code, operation, None, "registry persistence failed")
        .with_details(error.to_string())
}
