use colui_app::{
    ProfileMutation, ProfileReader, ProfileStore, RegistryDiagnostics, RegistryDiagnosticsReader,
    RegistryHealth, RegistryHealthState, RegistryRecovery, RegistrySnapshot,
    RegistrySnapshotIdentity, StoreFuture,
};
use colui_domain::{validate_draft, AppError, AppErrorCode, ProfileDraft, ProjectProfile};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
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
    history: Arc<Mutex<RegistryHistory>>,
    recovery_io: Arc<dyn RegistryRecoveryIo>,
}

pub trait RegistryRecoveryIo: Send + Sync {
    fn replace_canonical(&self, path: &Path, bytes: &[u8]) -> Result<(), AppError>;
    fn remove_artifact(&self, path: &Path) -> Result<(), io::Error>;
}

struct SystemRegistryRecoveryIo;

impl RegistryRecoveryIo for SystemRegistryRecoveryIo {
    fn replace_canonical(&self, path: &Path, bytes: &[u8]) -> Result<(), AppError> {
        atomic_write(path, bytes)
    }

    fn remove_artifact(&self, path: &Path) -> Result<(), io::Error> {
        fs::remove_file(path)
    }
}

#[derive(Default)]
struct RegistryHistory {
    last_operation_at: Option<colui_domain::Timestamp>,
    last_failure_at: Option<colui_domain::Timestamp>,
    latest_failure: Option<AppError>,
    last_recovery_result: Option<Result<(), AppError>>,
}

pub(crate) enum ImportResult {
    Skipped,
    Imported(usize),
    Malformed(AppError),
}

impl JsonProfileRegistry {
    pub fn new(config: RegistryConfig) -> Result<Self, AppError> {
        Self::with_recovery_io(config, Arc::new(SystemRegistryRecoveryIo))
    }

    pub fn with_recovery_io(
        config: RegistryConfig,
        recovery_io: Arc<dyn RegistryRecoveryIo>,
    ) -> Result<Self, AppError> {
        Ok(Self {
            config,
            history: Arc::new(Mutex::new(RegistryHistory::default())),
            recovery_io,
        })
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
            .truncate(false)
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
        let recovery_io = self.recovery_io.clone();
        Box::pin(async move {
            let history = self.history.clone();
            tokio::task::spawn_blocking(move || {
                JsonProfileRegistry {
                    config,
                    history,
                    recovery_io,
                }
                .load_bytes()
            })
            .await
            .map_err(|error| write_error("load_registry", error))?
        })
    }
}

impl ProfileStore for JsonProfileRegistry {
    fn mutate(&self, mutation: ProfileMutation) -> StoreFuture<'_, RegistrySnapshot> {
        let config = self.config.clone();
        let history = self.history.clone();
        let recovery_io = self.recovery_io.clone();
        Box::pin(async move {
            let task_history = history.clone();
            let result = tokio::task::spawn_blocking(move || {
                let registry = JsonProfileRegistry {
                    config,
                    history: task_history,
                    recovery_io,
                };
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
            .map_err(|error| write_error("mutate_registry", error))?;
            record_result(&history, &result);
            result
        })
    }
}

impl RegistryRecovery for JsonProfileRegistry {
    fn registry_health(&self) -> StoreFuture<'_, RegistryHealth> {
        let config = self.config.clone();
        let history = self.history.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || registry_health(&config, &history))
                .await
                .map_err(|error| write_error("read_registry_health", error))
        })
    }

    fn create_registry_backup(&self) -> StoreFuture<'_, RegistrySnapshotIdentity> {
        let config = self.config.clone();
        let history = self.history.clone();
        Box::pin(async move {
            let result = tokio::task::spawn_blocking(move || create_backup(&config))
                .await
                .map_err(|error| write_error("create_registry_backup", error))?;
            record_result(&history, &result);
            result
        })
    }

    fn restore_registry_backup(
        &self,
        _guard: &colui_app::RegistryRecoveryGuard,
    ) -> StoreFuture<'_, RegistrySnapshot> {
        let config = self.config.clone();
        let history = self.history.clone();
        let recovery_io = self.recovery_io.clone();
        Box::pin(async move {
            let result =
                tokio::task::spawn_blocking(move || restore_backup(&config, recovery_io.as_ref()))
                    .await
                    .map_err(|error| write_error("restore_registry_backup", error))?;
            record_result(&history, &result);
            history
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner())
                .last_recovery_result = Some(result.as_ref().map(|_| ()).map_err(Clone::clone));
            result
        })
    }
}

impl RegistryDiagnosticsReader for JsonProfileRegistry {
    fn registry_diagnostics(&self) -> colui_app::DiagnosticsFuture<'_, RegistryDiagnostics> {
        Box::pin(async move {
            let health = self.registry_health().await?;
            let path = backup_path(&self.config);
            let backup = tokio::task::spawn_blocking(move || backup_diagnostics(&path))
                .await
                .map_err(|_| write_error("registry_diagnostics", "backup observation failed"))?;
            Ok(RegistryDiagnostics {
                registry_path: self.config.canonical_path.clone(),
                backup_path: backup_path(&self.config),
                backup,
                revision: health
                    .identity
                    .as_ref()
                    .map(|value| value.registry_revision),
                health,
                lock_timeout: self.config.lock_timeout,
                last_recovery_result: self
                    .history
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner())
                    .last_recovery_result
                    .clone(),
            })
        })
    }
}

fn backup_diagnostics(path: &Path) -> colui_app::RegistryBackupDiagnostics {
    use colui_app::{BackupValidationState as S, RegistryBackupDiagnostics};
    let metadata = fs::metadata(path);
    let modified_at = metadata
        .as_ref()
        .ok()
        .and_then(|m| m.modified().ok())
        .map(|time| {
            colui_domain::Timestamp(chrono::DateTime::<chrono::Utc>::from(time).to_rfc3339())
        });
    let (state, error) = match fs::read(path) {
        Ok(bytes) => match decode(&bytes) {
            Ok(_) => (S::Valid, None),
            Err(error) => (S::Corrupt, Some(error)),
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => (S::Missing, None),
        Err(error) => (S::Unreadable, Some(io_error("read_registry_backup", error))),
    };
    RegistryBackupDiagnostics {
        exists: metadata.is_ok() || matches!(state, S::Valid | S::Corrupt),
        modified_at,
        state,
        error,
    }
}

fn registry_health(config: &RegistryConfig, history: &Mutex<RegistryHistory>) -> RegistryHealth {
    let history = history
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    let fields = || {
        (
            history.last_operation_at.clone(),
            history.last_failure_at.clone(),
            history.latest_failure.clone(),
        )
    };
    match fs::read(&config.canonical_path) {
        Ok(bytes) => match decode(&bytes) {
            Ok(snapshot) => {
                let (last_operation_at, last_failure_at, error) = fields();
                RegistryHealth {
                    state: match error.as_ref().map(|error| error.code) {
                        Some(AppErrorCode::RegistryLocked) => RegistryHealthState::Locked,
                        Some(_) => RegistryHealthState::WriteFailure,
                        None => RegistryHealthState::Healthy,
                    },
                    identity: Some(identity(&snapshot, &bytes)),
                    error,
                    last_operation_at,
                    last_failure_at,
                }
            }
            Err(error) => {
                let (last_operation_at, last_failure_at, _) = fields();
                RegistryHealth {
                    state: RegistryHealthState::Corrupt,
                    identity: None,
                    error: Some(error),
                    last_operation_at,
                    last_failure_at,
                }
            }
        },
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            let (last_operation_at, last_failure_at, error) = fields();
            RegistryHealth {
                state: RegistryHealthState::Missing,
                identity: None,
                error,
                last_operation_at,
                last_failure_at,
            }
        }
        Err(error) => {
            let (last_operation_at, last_failure_at, _) = fields();
            RegistryHealth {
                state: RegistryHealthState::Unreadable,
                identity: None,
                error: Some(io_error("read_registry_health", error)),
                last_operation_at,
                last_failure_at,
            }
        }
    }
}

fn create_backup(config: &RegistryConfig) -> Result<RegistrySnapshotIdentity, AppError> {
    let registry = JsonProfileRegistry::new(config.clone())?;
    let _lock = registry.lock()?;
    let bytes = fs::read(&config.canonical_path)
        .map_err(|error| io_error("read_registry_backup_source", error))?;
    let snapshot = decode(&bytes)?;
    atomic_write(&backup_path(config), &bytes)?;
    Ok(identity(&snapshot, &bytes))
}

fn restore_backup(
    config: &RegistryConfig,
    recovery_io: &dyn RegistryRecoveryIo,
) -> Result<RegistrySnapshot, AppError> {
    let result = restore_backup_attempt(config, recovery_io);
    let prune = prune_artifacts(config, recovery_io);
    match (result, prune) {
        (Err(error), _) => Err(error),
        (Ok(_), Err(error)) => Err(error),
        (Ok(snapshot), Ok(())) => Ok(snapshot),
    }
}

fn restore_backup_attempt(
    config: &RegistryConfig,
    recovery_io: &dyn RegistryRecoveryIo,
) -> Result<RegistrySnapshot, AppError> {
    let backup_path = backup_path(config);
    let initial_backup =
        fs::read(&backup_path).map_err(|error| io_error("read_registry_backup", error))?;
    let initial_hash = digest(&initial_backup);
    let backup = decode(&initial_backup)?;
    let registry = JsonProfileRegistry::new(config.clone())?;
    let _lock = registry.lock()?;
    let locked_backup =
        fs::read(&backup_path).map_err(|error| io_error("reread_registry_backup", error))?;
    if digest(&locked_backup) != initial_hash {
        return Err(AppError::new(
            AppErrorCode::RecoveryConflict,
            "restore_registry_backup",
            None,
            "registry backup changed during restore",
        ));
    }
    let canonical_bytes = match fs::read(&config.canonical_path) {
        Ok(bytes) => Some(bytes),
        Err(error) if error.kind() == io::ErrorKind::NotFound => None,
        Err(error) => return Err(io_error("read_registry_for_restore", error)),
    };
    let canonical = canonical_bytes
        .as_deref()
        .and_then(|bytes| decode(bytes).ok());
    let authority_revision = canonical
        .as_ref()
        .map(|value| value.registry_revision)
        .unwrap_or(backup.registry_revision)
        .max(backup.registry_revision);
    let mut restored = backup;
    restored.registry_revision = authority_revision
        .checked_add(1)
        .ok_or_else(|| write_error("restore_registry_backup", "registry revision exhausted"))?;
    for profile in &mut restored.profiles {
        let canonical_revision = canonical
            .as_ref()
            .and_then(|snapshot| {
                snapshot
                    .profiles
                    .iter()
                    .find(|current| current.id == profile.id)
            })
            .map(|current| current.revision.value())
            .unwrap_or(0);
        profile.revision = colui_domain::Revision::new(
            profile
                .revision
                .value()
                .max(canonical_revision)
                .checked_add(1)
                .ok_or_else(|| {
                    write_error("restore_registry_backup", "profile revision exhausted")
                })?,
        );
    }
    let artifact = canonical_bytes
        .as_ref()
        .map(|bytes| preserve_canonical(config, bytes))
        .transpose()?;
    let result = (|| {
        recovery_io.replace_canonical(&config.canonical_path, &encode(&restored)?)?;
        let reread = registry.load_bytes()?;
        if reread != restored {
            return Err(write_error(
                "verify_registry_restore",
                "registry changed during restore",
            ));
        }
        Ok(reread)
    })();
    if result.is_err() {
        if let Some(path) = artifact {
            let _ = recovery_io.remove_artifact(&path);
        }
    }
    result
}

fn backup_path(config: &RegistryConfig) -> PathBuf {
    config.canonical_path.with_extension("json.bak")
}

fn identity(snapshot: &RegistrySnapshot, bytes: &[u8]) -> RegistrySnapshotIdentity {
    RegistrySnapshotIdentity {
        registry_revision: snapshot.registry_revision,
        canonical_content_sha256: digest(bytes),
    }
}

fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn record_result<T>(history: &Mutex<RegistryHistory>, result: &Result<T, AppError>) {
    let now = colui_domain::Timestamp(chrono::Utc::now().to_rfc3339());
    let mut history = history
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());
    history.last_operation_at = Some(now.clone());
    match result {
        Ok(_) => history.latest_failure = None,
        Err(error) => {
            history.last_failure_at = Some(now);
            history.latest_failure = Some(error.clone());
        }
    }
}

fn preserve_canonical(config: &RegistryConfig, bytes: &[u8]) -> Result<PathBuf, AppError> {
    let parent = config
        .canonical_path
        .parent()
        .ok_or_else(|| write_error("preserve_registry", "registry path has no parent"))?;
    let path = parent.join(format!(
        "registry.pre-restore.{}.{}.json",
        chrono::Utc::now().format("%Y%m%dT%H%M%S%.fZ"),
        Uuid::new_v4()
    ));
    let mut options = OpenOptions::new();
    options.create_new(true).write(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&path)
        .map_err(|error| io_error("preserve_registry", error))?;
    file.write_all(bytes)
        .map_err(|error| io_error("preserve_registry", error))?;
    file.sync_all()
        .map_err(|error| io_error("fsync_preserved_registry", error))?;
    Ok(path)
}

fn prune_artifacts(
    config: &RegistryConfig,
    recovery_io: &dyn RegistryRecoveryIo,
) -> Result<(), AppError> {
    let parent = config
        .canonical_path
        .parent()
        .ok_or_else(|| write_error("prune_registry_artifacts", "registry path has no parent"))?;
    let mut artifacts = fs::read_dir(parent)
        .map_err(|error| io_error("prune_registry_artifacts", error))?
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("registry.pre-restore.")
        })
        .collect::<Vec<_>>();
    artifacts.sort_by_key(|entry| entry.file_name());
    let remove_count = artifacts.len().saturating_sub(3);
    for entry in artifacts.into_iter().take(remove_count) {
        recovery_io
            .remove_artifact(&entry.path())
            .map_err(|error| io_error("prune_registry_artifacts", error))?;
    }
    Ok(())
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
        let mut options = OpenOptions::new();
        options.create_new(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options
            .open(&temp)
            .map_err(|error| io_error("write_registry", error))?;
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
