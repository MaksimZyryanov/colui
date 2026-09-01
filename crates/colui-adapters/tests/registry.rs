use colui_adapters::{
    registry::{import_v1, import_v1_if_needed},
    JsonProfileRegistry, RegistryConfig,
};
use colui_app::{IdGenerator, ProfileReader, ProfileStore};
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

struct DeterministicIds(std::sync::Mutex<Vec<ProfileId>>);

impl IdGenerator for DeterministicIds {
    fn generate(&self) -> ProfileId {
        self.0.lock().unwrap().remove(0)
    }
}

fn deterministic_ids() -> DeterministicIds {
    DeterministicIds(std::sync::Mutex::new(vec![
        ProfileId::new(Uuid::from_u128(0x00112233445566778899aabbccddeeff)),
        ProfileId::new(Uuid::from_u128(0xffeeddccbbaa99887766554433221100)),
    ]))
}

#[test]
fn import_preserves_entry_and_file_order_without_touching_source() {
    let source = br#"[{"name":"Checkout","working_dir":"/tmp/checkout","config_files":["compose.yml","local.yml"],"env_files":[".env"]}]"#;
    let original = source.to_vec();
    let imported = import_v1(source, &deterministic_ids()).unwrap();
    assert_eq!(source, original.as_slice());
    assert_eq!(imported.len(), 1);
    assert_eq!(imported[0].display_name, "Checkout".try_into().unwrap());
    assert_eq!(imported[0].compose_files[1], PathBuf::from("local.yml"));
    assert_eq!(imported[0].environment_files, vec![PathBuf::from(".env")]);
    assert_eq!(
        imported[0].registration_origin,
        RegistrationOrigin::Migrated
    );
}

#[test]
fn invalid_names_are_normalized_and_collisions_are_not_merged() {
    let source = br#"[{"name":"Checkout API","working_dir":"/tmp/a","config_files":["a.yml"]},{"name":"checkout-api","working_dir":"/tmp/b","config_files":["b.yml"]}]"#;
    let imported = import_v1(source, &deterministic_ids()).unwrap();
    assert_eq!(imported.len(), 2);
    assert_eq!(
        imported[0].compose_project_name,
        imported[1].compose_project_name
    );
    assert_eq!(
        imported[0].compose_project_name,
        "checkout-api".try_into().unwrap()
    );
}

#[test]
fn empty_normalized_name_uses_profile_id_fallback() {
    let source = br#"[{"name":"!!!","working_dir":"/tmp/empty","config_files":["compose.yml"]}]"#;
    let imported = import_v1(source, &deterministic_ids()).unwrap();
    assert_eq!(
        imported[0].compose_project_name,
        "imported-00112233".try_into().unwrap()
    );
    assert_eq!(imported[0].revision, colui_domain::Revision::initial());
}

#[test]
fn malformed_import_returns_registry_corrupt_without_mutating_source() {
    let source = b"[{broken";
    let original = source.to_vec();
    let error = import_v1(source, &deterministic_ids()).unwrap_err();
    assert_eq!(error.code, AppErrorCode::RegistryCorrupt);
    assert_eq!(source, original.as_slice());
}

#[tokio::test]
async fn first_start_imports_once_and_backs_up_legacy_bytes() {
    let directory = tempfile::tempdir().unwrap();
    let config = RegistryConfig::in_directory(directory.path());
    let registry = JsonProfileRegistry::new(config.clone()).unwrap();
    let legacy = directory.path().join("projects.json");
    let backup = directory.path().join("projects.json.v1.bak");
    let source =
        br#"[{"name":"Checkout","working_dir":"/tmp/checkout","config_files":["compose.yml"]}]"#;
    std::fs::write(&legacy, source).unwrap();

    let diagnostics = import_v1_if_needed(&registry, &legacy, &backup, &deterministic_ids())
        .await
        .unwrap();
    assert_eq!(diagnostics.imported_profiles, 1);
    assert_eq!(bytes(&legacy), source);
    assert_eq!(bytes(&backup), source);
    assert_eq!(registry.load().await.unwrap().profiles.len(), 1);

    let second = import_v1_if_needed(&registry, &legacy, &backup, &deterministic_ids())
        .await
        .unwrap();
    assert_eq!(second.imported_profiles, 0);
}

#[tokio::test]
async fn malformed_first_start_creates_empty_v2_and_reports_diagnostic() {
    let directory = tempfile::tempdir().unwrap();
    let config = RegistryConfig::in_directory(directory.path());
    let registry = JsonProfileRegistry::new(config.clone()).unwrap();
    let legacy = directory.path().join("projects.json");
    let backup = directory.path().join("projects.json.v1.bak");
    let source = b"[{broken";
    std::fs::write(&legacy, source).unwrap();

    let diagnostics = import_v1_if_needed(&registry, &legacy, &backup, &deterministic_ids())
        .await
        .unwrap();
    assert_eq!(diagnostics.imported_profiles, 0);
    assert_eq!(
        diagnostics.error.unwrap().code,
        AppErrorCode::RegistryCorrupt
    );
    assert_eq!(bytes(&legacy), source);
    assert_eq!(bytes(&backup), source);
    assert_eq!(registry.load().await.unwrap().profiles.len(), 0);
    assert_eq!(registry.load().await.unwrap().registry_revision, 0);
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
