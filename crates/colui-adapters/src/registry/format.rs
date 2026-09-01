use colui_app::{ProfileMutation, ProfileReader, ProfileStore, RegistrySnapshot, StoreFuture};
use colui_domain::{validate_draft, AppError, AppErrorCode, ProfileDraft, ProjectProfile};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, File, OpenOptions};
use std::io;
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

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.lock_timeout = timeout;
        self
    }
}

pub struct JsonProfileRegistry {
    config: RegistryConfig,
}

impl JsonProfileRegistry {
    pub fn new(config: RegistryConfig) -> Result<Self, AppError> {
        Ok(Self { config })
    }

    pub fn config(&self) -> &RegistryConfig {
        &self.config
    }

    fn load_bytes(&self) -> Result<RegistrySnapshot, AppError> {
        match fs::read(&self.config.canonical_path) {
            Ok(bytes) => decode(&bytes),
            Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(RegistrySnapshot {
                registry_revision: 0,
                profiles: Vec::new(),
            }),
            Err(error) => Err(write_error("read_registry", error)),
        }
    }

    fn lock(&self) -> Result<File, AppError> {
        if let Some(parent) = self.config.lock_path.parent() {
            fs::create_dir_all(parent)
                .map_err(|error| write_error("create_registry_directory", error))?;
        }
        let file = OpenOptions::new()
            .create(true)
            .read(true)
            .write(true)
            .open(&self.config.lock_path)
            .map_err(|error| write_error("open_registry_lock", error))?;
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
                    std::thread::sleep(delay.min(self.config.lock_timeout));
                    delay = (delay * 2).min(Duration::from_millis(100));
                }
                Err(error) => return Err(write_error("lock_registry", error)),
            }
        }
    }
}

impl ProfileReader for JsonProfileRegistry {
    fn load(&self) -> StoreFuture<'_, RegistrySnapshot> {
        Box::pin(async move { self.load_bytes() })
    }
}

impl ProfileStore for JsonProfileRegistry {
    fn mutate(&self, mutation: ProfileMutation) -> StoreFuture<'_, RegistrySnapshot> {
        Box::pin(async move {
            let lock = self.lock()?;
            let before = self.load_bytes()?;
            let original = encode(&before)?;
            let mut after = mutation(before)?;
            validate_snapshot(&after)?;
            let changed = after.profiles != decode(&original)?.profiles;
            if changed {
                after.registry_revision = after
                    .registry_revision
                    .max(decode(&original)?.registry_revision)
                    + 1;
                let bytes = encode(&after)?;
                atomic_write(&self.config.canonical_path, &bytes)?;
                let reread = self.load_bytes()?;
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
            after.registry_revision = decode(&original)?.registry_revision;
            drop(lock);
            Ok(after)
        })
    }
}

#[derive(Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct RegistryFile {
    schema_version: u8,
    registry_revision: u64,
    profiles: Vec<ProjectProfile>,
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
        profiles: file.profiles,
    };
    validate_snapshot(&snapshot)?;
    Ok(snapshot)
}

fn encode(snapshot: &RegistrySnapshot) -> Result<Vec<u8>, AppError> {
    serde_json::to_vec_pretty(&RegistryFile {
        schema_version: 2,
        registry_revision: snapshot.registry_revision,
        profiles: snapshot.profiles.clone(),
    })
    .map_err(|error| write_error("serialize_registry", error))
}

fn validate_snapshot(snapshot: &RegistrySnapshot) -> Result<(), AppError> {
    for profile in &snapshot.profiles {
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
    fs::create_dir_all(parent).map_err(|error| write_error("create_registry_directory", error))?;
    let temp = parent.join(format!(".registry.{}.tmp", Uuid::new_v4()));
    let result = (|| {
        let mut file = File::create(&temp).map_err(|error| write_error("write_registry", error))?;
        io::Write::write_all(&mut file, bytes)
            .map_err(|error| write_error("write_registry", error))?;
        file.sync_all()
            .map_err(|error| write_error("fsync_registry", error))?;
        fs::rename(&temp, path).map_err(|error| write_error("rename_registry", error))?;
        let directory =
            File::open(parent).map_err(|error| write_error("open_registry_directory", error))?;
        directory
            .sync_all()
            .map_err(|error| write_error("fsync_registry_directory", error))
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
