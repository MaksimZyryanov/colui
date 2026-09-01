use colui_adapters::{JsonProfileRegistry, RegistryConfig};
use colui_app::{ProfileReader, ProfileStore};
use colui_domain::{
    AppError, AppErrorCode, ProfileDraft, ProfileId, ProjectProfile, RegistrationOrigin,
};
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use uuid::Uuid;

#[test]
fn retry_sleep_never_exceeds_remaining_deadline() {
    let remaining = std::time::Duration::from_millis(3);
    let delay = std::time::Duration::from_millis(100);
    assert_eq!(delay.min(remaining), remaining);
}

fn test_registry() -> (TempDir, JsonProfileRegistry) {
    let directory = tempfile::tempdir().unwrap();
    let registry =
        JsonProfileRegistry::new(RegistryConfig::in_directory(directory.path())).unwrap();
    (directory, registry)
}

fn profile_with_files(files: &[&str]) -> ProjectProfile {
    ProjectProfile::from_draft(
        ProfileId::new(Uuid::new_v4()),
        ProfileDraft {
            display_name: "Demo".try_into().unwrap(),
            compose_project_name: "demo".try_into().unwrap(),
            working_directory: PathBuf::from("/tmp/demo"),
            compose_files: files.iter().map(PathBuf::from).collect(),
            environment_files: vec![],
            registration_origin: RegistrationOrigin::Manual,
        },
    )
    .unwrap()
}

fn bytes(path: &Path) -> Vec<u8> {
    std::fs::read(path).unwrap()
}

#[test]
fn lock_timeout_config_requires_five_to_ten_seconds() {
    let directory = tempfile::tempdir().unwrap();
    assert!(RegistryConfig::in_directory(directory.path())
        .try_with_timeout(std::time::Duration::from_secs(4))
        .is_err());
    assert!(RegistryConfig::in_directory(directory.path())
        .try_with_timeout(std::time::Duration::from_secs(11))
        .is_err());
    assert!(RegistryConfig::in_directory(directory.path())
        .try_with_timeout(std::time::Duration::from_secs(5))
        .is_ok());
    assert!(RegistryConfig::in_directory(directory.path())
        .try_with_timeout(std::time::Duration::from_secs(10))
        .is_ok());
}

#[cfg(unix)]
#[tokio::test]
async fn permission_failures_use_permission_denied() {
    let directory = tempfile::tempdir().unwrap();
    let config = RegistryConfig::in_directory(directory.path().join("blocked"));
    let registry = JsonProfileRegistry::new(config).unwrap();
    std::fs::create_dir(directory.path().join("blocked")).unwrap();
    let blocked = directory.path().join("blocked");
    std::fs::set_permissions(
        &blocked,
        std::os::unix::fs::PermissionsExt::from_mode(0o000),
    )
    .unwrap();
    let error = registry.load().await.unwrap_err();
    assert_eq!(error.code, AppErrorCode::PermissionDenied);
    std::fs::set_permissions(
        &blocked,
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )
    .unwrap();
}

#[tokio::test]
async fn mutation_writes_v2_and_preserves_order() {
    let (directory, registry) = test_registry();
    let result = registry
        .mutate(Box::new(|mut draft| {
            draft
                .profiles
                .push(profile_with_files(&["compose.yml", "compose.local.yml"]));
            Ok(draft)
        }))
        .await
        .unwrap();
    assert_eq!(result.profiles[0].compose_files.len(), 2);
    let json: serde_json::Value =
        serde_json::from_slice(&bytes(&directory.path().join("registry.json"))).unwrap();
    assert_eq!(json["schemaVersion"], 2);
    assert_eq!(json["registryRevision"], 1);
}

#[tokio::test]
async fn mutation_revision_is_current_revision_plus_one() {
    let (directory, registry) = test_registry();
    std::fs::write(
        directory.path().join("registry.json"),
        br#"{"schemaVersion":2,"registryRevision":41,"profiles":[]}"#,
    )
    .unwrap();
    registry
        .mutate(Box::new(|mut snapshot| {
            snapshot.profiles.push(profile_with_files(&["compose.yml"]));
            snapshot.registry_revision = 0;
            Ok(snapshot)
        }))
        .await
        .unwrap();
    let json: serde_json::Value =
        serde_json::from_slice(&bytes(&directory.path().join("registry.json"))).unwrap();
    assert_eq!(json["registryRevision"], 42);
}

#[tokio::test]
async fn max_revision_returns_write_error_without_overwrite() {
    let (directory, registry) = test_registry();
    let canonical = directory.path().join("registry.json");
    std::fs::write(
        &canonical,
        br#"{"schemaVersion":2,"registryRevision":18446744073709551615,"profiles":[]}"#,
    )
    .unwrap();
    let before = bytes(&canonical);
    let error = registry
        .mutate(Box::new(|mut snapshot| {
            snapshot.profiles.push(profile_with_files(&["compose.yml"]));
            Ok(snapshot)
        }))
        .await
        .unwrap_err();
    assert_eq!(error.code, AppErrorCode::RegistryWriteFailed);
    assert_eq!(error.message, "registry revision exhausted");
    assert_eq!(bytes(&canonical), before);
}

#[tokio::test]
async fn unknown_v2_fields_are_corrupt() {
    let (directory, registry) = test_registry();
    std::fs::write(
        directory.path().join("registry.json"),
        br#"{"schemaVersion":2,"registryRevision":0,"profiles":[],"unexpected":true}"#,
    )
    .unwrap();
    assert_eq!(
        registry.load().await.unwrap_err().code,
        AppErrorCode::RegistryCorrupt
    );
}

#[tokio::test]
async fn unknown_profile_fields_are_corrupt() {
    let (directory, registry) = test_registry();
    std::fs::write(
        directory.path().join("registry.json"),
        br#"{"schemaVersion":2,"registryRevision":0,"profiles":[{"unexpected":true}]}"#,
    )
    .unwrap();
    assert_eq!(
        registry.load().await.unwrap_err().code,
        AppErrorCode::RegistryCorrupt
    );
}

#[tokio::test]
async fn corrupt_registry_is_not_overwritten() {
    let (directory, registry) = test_registry();
    let canonical = directory.path().join("registry.json");
    std::fs::write(&canonical, b"{broken").unwrap();
    let error = registry.load().await.unwrap_err();
    assert_eq!(error.code, AppErrorCode::RegistryCorrupt);
    assert_eq!(bytes(&canonical), b"{broken");
}

#[tokio::test]
async fn missing_registry_loads_empty_without_writing() {
    let (directory, registry) = test_registry();
    let snapshot = registry.load().await.unwrap();
    assert_eq!(snapshot.registry_revision, 0);
    assert!(!directory.path().join("registry.json").exists());
}

#[tokio::test]
async fn unchanged_mutation_does_not_write() {
    let (directory, registry) = test_registry();
    registry
        .mutate(Box::new(|mut snapshot| {
            snapshot.profiles.push(profile_with_files(&["compose.yml"]));
            Ok(snapshot)
        }))
        .await
        .unwrap();
    let canonical = directory.path().join("registry.json");
    let before = bytes(&canonical);
    registry.mutate(Box::new(Ok)).await.unwrap();
    assert_eq!(bytes(&canonical), before);
}

#[tokio::test]
async fn invalid_mutation_leaves_canonical_bytes_unchanged() {
    let (directory, registry) = test_registry();
    registry
        .mutate(Box::new(|mut snapshot| {
            snapshot.profiles.push(profile_with_files(&["compose.yml"]));
            Ok(snapshot)
        }))
        .await
        .unwrap();
    let canonical = directory.path().join("registry.json");
    let before = bytes(&canonical);
    let error = registry
        .mutate(Box::new(|_snapshot| {
            Err(AppError::new(
                AppErrorCode::ProfileInvalid,
                "test",
                None,
                "invalid",
            ))
        }))
        .await
        .unwrap_err();
    assert_eq!(error.code, AppErrorCode::ProfileInvalid);
    assert_eq!(bytes(&canonical), before);
}

#[tokio::test]
async fn revision_conflict_error_leaves_canonical_bytes_unchanged() {
    let (directory, registry) = test_registry();
    let profile = profile_with_files(&["missing-compose.yml"]);
    registry
        .mutate(Box::new(move |mut snapshot| {
            snapshot.profiles.push(profile);
            Ok(snapshot)
        }))
        .await
        .unwrap();
    let canonical = directory.path().join("registry.json");
    let before = bytes(&canonical);
    let error = registry
        .mutate(Box::new(|_snapshot| {
            Err(AppError::new(
                AppErrorCode::ProfileRevisionConflict,
                "test_mutation",
                None,
                "profile revision conflict",
            ))
        }))
        .await
        .unwrap_err();
    assert_eq!(error.code, AppErrorCode::ProfileRevisionConflict);
    assert_eq!(bytes(&canonical), before);
}

#[tokio::test]
async fn invalid_paths_remain_persisted() {
    let (directory, registry) = test_registry();
    registry
        .mutate(Box::new(|mut snapshot| {
            snapshot
                .profiles
                .push(profile_with_files(&["does-not-exist.yml"]));
            Ok(snapshot)
        }))
        .await
        .unwrap();
    let loaded = registry.load().await.unwrap();
    assert_eq!(
        loaded.profiles[0].compose_files,
        vec![PathBuf::from("does-not-exist.yml")]
    );
    assert!(directory.path().join("registry.json").exists());
}

#[tokio::test]
async fn lock_timeout_is_retryable() {
    let (directory, registry) = test_registry();
    let lock_path = directory.path().join("registry.lock");
    let lock = std::fs::File::create(&lock_path).unwrap();
    fs2::FileExt::lock_exclusive(&lock).unwrap();
    let releaser = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_millis(50));
        drop(lock);
    });
    registry.mutate(Box::new(Ok)).await.unwrap();
    releaser.join().unwrap();
}

#[tokio::test]
async fn lock_expiration_returns_retryable_error() {
    let (directory, _registry) = test_registry();
    let lock_path = directory.path().join("registry.lock");
    let lock = std::fs::File::create(&lock_path).unwrap();
    fs2::FileExt::lock_exclusive(&lock).unwrap();
    let locked = JsonProfileRegistry::new(
        RegistryConfig::in_directory(directory.path())
            .try_with_timeout(std::time::Duration::from_secs(5))
            .unwrap(),
    )
    .unwrap();
    let handle = std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(6));
        drop(lock);
    });
    let started = std::time::Instant::now();
    let error = locked.mutate(Box::new(Ok)).await.unwrap_err();
    assert!(
        started.elapsed()
            < std::time::Duration::from_secs(5) + std::time::Duration::from_millis(500)
    );
    assert_eq!(error.code, AppErrorCode::RegistryLocked);
    assert!(error.retryable);
    handle.join().unwrap();
}
