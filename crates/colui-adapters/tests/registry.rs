use colui_adapters::{
    registry::{import_v1, import_v1_if_needed},
    JsonProfileRegistry, RegistryConfig,
};
use colui_app::{
    DefinitionInvalidator, IdGenerator, ProfileReader, ProfileStore, RegistryHealthState,
    RegistryRecovery, RestoreRegistryBackup,
};
use colui_domain::{
    AppError, AppErrorCode, ProfileDraft, ProfileId, ProjectProfile, RegistrationOrigin,
};
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
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

#[derive(Clone)]
struct DeterministicIds(std::sync::Arc<std::sync::Mutex<Vec<ProfileId>>>);

impl IdGenerator for DeterministicIds {
    fn generate(&self) -> ProfileId {
        self.0.lock().unwrap().remove(0)
    }
}

fn deterministic_ids() -> DeterministicIds {
    DeterministicIds(std::sync::Arc::new(std::sync::Mutex::new(vec![
        ProfileId::new(Uuid::from_u128(0x00112233445566778899aabbccddeeff)),
        ProfileId::new(Uuid::from_u128(0xffeeddccbbaa99887766554433221100)),
    ])))
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
fn normalization_preserves_compose_underscores() {
    let source = br#"[{"name":"Checkout_API","working_dir":"/tmp/a","config_files":["a.yml"]}]"#;
    let imported = import_v1(source, &deterministic_ids()).unwrap();
    assert_eq!(
        imported[0].compose_project_name,
        "checkout_api".try_into().unwrap()
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

#[tokio::test]
async fn concurrent_first_start_imports_only_once() {
    let directory = tempfile::tempdir().unwrap();
    let config = RegistryConfig::in_directory(directory.path());
    let registry = std::sync::Arc::new(JsonProfileRegistry::new(config).unwrap());
    let legacy = directory.path().join("projects.json");
    let backup = directory.path().join("projects.json.v1.bak");
    std::fs::write(
        &legacy,
        br#"[{"name":"Checkout","working_dir":"/tmp/checkout","config_files":["compose.yml"]}]"#,
    )
    .unwrap();
    let first_registry = registry.clone();
    let second_registry = registry.clone();
    let first = tokio::spawn(async move {
        import_v1_if_needed(&first_registry, &legacy, &backup, &deterministic_ids()).await
    });
    let legacy = directory.path().join("projects.json");
    let backup = directory.path().join("projects.json.v1.bak");
    let second = tokio::spawn(async move {
        import_v1_if_needed(&second_registry, &legacy, &backup, &deterministic_ids()).await
    });
    let first = first.await.unwrap().unwrap();
    let second = second.await.unwrap().unwrap();
    assert_eq!(first.imported_profiles + second.imported_profiles, 1);
    assert_eq!(registry.load().await.unwrap().profiles.len(), 1);
}

#[cfg(unix)]
#[tokio::test]
async fn legacy_read_failures_use_permission_denied() {
    let directory = tempfile::tempdir().unwrap();
    let blocked = directory.path().join("blocked");
    std::fs::create_dir(&blocked).unwrap();
    std::fs::set_permissions(
        &blocked,
        std::os::unix::fs::PermissionsExt::from_mode(0o000),
    )
    .unwrap();
    let registry =
        JsonProfileRegistry::new(RegistryConfig::in_directory(directory.path())).unwrap();
    let result = import_v1_if_needed(
        &registry,
        &blocked.join("projects.json"),
        &directory.path().join("backup"),
        &deterministic_ids(),
    )
    .await;
    std::fs::set_permissions(
        &blocked,
        std::os::unix::fs::PermissionsExt::from_mode(0o700),
    )
    .unwrap();
    assert_eq!(result.unwrap_err().code, AppErrorCode::PermissionDenied);
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
async fn duplicate_profile_ids_are_corrupt_on_load_and_mutation() {
    let (directory, registry) = test_registry();
    let profile = profile_with_files(&["compose.yml"]);
    registry
        .mutate(Box::new({
            let profile = profile.clone();
            move |mut snapshot| {
                snapshot.profiles.push(profile);
                Ok(snapshot)
            }
        }))
        .await
        .unwrap();
    let error = registry
        .mutate(Box::new({
            let profile = profile.clone();
            move |mut snapshot| {
                snapshot.profiles.push(profile);
                Ok(snapshot)
            }
        }))
        .await
        .unwrap_err();
    assert_eq!(error.code, AppErrorCode::RegistryCorrupt);

    let id = profile.id.to_string();
    let record = serde_json::json!({
        "schemaVersion": 2,
        "registryRevision": 1,
        "profiles": [
            {"id": id, "revision": 1, "displayName": "Demo", "composeProjectName": "demo", "workingDirectory": "/tmp/demo", "composeFiles": ["compose.yml"], "environmentFiles": [], "registrationOrigin": "Manual"},
            {"id": id, "revision": 1, "displayName": "Demo 2", "composeProjectName": "demo-2", "workingDirectory": "/tmp/demo", "composeFiles": ["compose-2.yml"], "environmentFiles": [], "registrationOrigin": "Manual"}
        ]
    });
    std::fs::write(
        directory.path().join("registry.json"),
        serde_json::to_vec(&record).unwrap(),
    )
    .unwrap();
    assert_eq!(
        registry.load().await.unwrap_err().code,
        AppErrorCode::RegistryCorrupt
    );

    assert_eq!(
        registry.load().await.unwrap_err().code,
        AppErrorCode::RegistryCorrupt
    );
}

#[tokio::test]
async fn duplicate_paths_are_rejected_by_mutation_and_round_trip() {
    let (_directory, registry) = test_registry();
    let mut profile = profile_with_files(&["compose.yml"]);
    profile.environment_files.push(PathBuf::from("compose.yml"));
    let error = registry
        .mutate(Box::new(move |mut snapshot| {
            snapshot.profiles.push(profile);
            Ok(snapshot)
        }))
        .await
        .unwrap_err();
    assert_eq!(error.code, AppErrorCode::ProfileInvalid);
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

#[tokio::test]
async fn backup_is_explicit_exact_and_has_private_permissions() {
    let (directory, registry) = test_registry();
    registry
        .mutate(Box::new(|mut snapshot| {
            snapshot.profiles.push(profile_with_files(&["compose.yml"]));
            Ok(snapshot)
        }))
        .await
        .unwrap();
    let canonical = directory.path().join("registry.json");
    let backup = directory.path().join("registry.json.bak");
    assert!(!backup.exists());
    let identity = registry.create_registry_backup().await.unwrap();
    assert_eq!(bytes(&backup), bytes(&canonical));
    assert_eq!(identity.registry_revision, 1);
    assert_eq!(identity.canonical_content_sha256.len(), 64);
    #[cfg(unix)]
    assert_eq!(
        std::fs::metadata(&backup).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[tokio::test]
async fn health_distinguishes_missing_corrupt_and_valid_identity() {
    let (directory, registry) = test_registry();
    let missing = registry.registry_health().await.unwrap();
    assert_eq!(missing.state, RegistryHealthState::Missing);
    assert!(missing.identity.is_none());

    std::fs::write(directory.path().join("registry.json"), b"{broken").unwrap();
    let corrupt = registry.registry_health().await.unwrap();
    assert_eq!(corrupt.state, RegistryHealthState::Corrupt);
    assert!(corrupt.identity.is_none());

    std::fs::write(
        directory.path().join("registry.json"),
        br#"{"schemaVersion":2,"registryRevision":7,"profiles":[]}"#,
    )
    .unwrap();
    let healthy = registry.registry_health().await.unwrap();
    assert_eq!(healthy.state, RegistryHealthState::Healthy);
    assert_eq!(healthy.identity.unwrap().registry_revision, 7);
}

#[tokio::test]
async fn restore_replaces_corrupt_canonical_without_silent_read_repair() {
    let (directory, registry) = test_registry();
    let canonical = directory.path().join("registry.json");
    std::fs::write(
        &canonical,
        br#"{"schemaVersion":2,"registryRevision":4,"profiles":[]}"#,
    )
    .unwrap();
    registry.create_registry_backup().await.unwrap();
    std::fs::write(&canonical, b"{broken").unwrap();
    assert_eq!(
        registry.load().await.unwrap_err().code,
        AppErrorCode::RegistryCorrupt
    );
    assert_eq!(bytes(&canonical), b"{broken");

    let locks =
        colui_adapters::OperationLockManager::new(directory.path().join("registry.recovery.lock"));
    let invalidator = CountingInvalidator::default();
    let restored = RestoreRegistryBackup::new(&registry, &locks, &invalidator)
        .execute()
        .await
        .unwrap();
    assert_eq!(invalidator.0.load(std::sync::atomic::Ordering::Relaxed), 1);
    assert_eq!(restored.registry_revision, 5);
    assert_eq!(registry.load().await.unwrap().registry_revision, 5);
    let artifacts = std::fs::read_dir(directory.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("registry.pre-restore.")
        })
        .collect::<Vec<_>>();
    assert_eq!(artifacts.len(), 1);
    assert_eq!(bytes(&artifacts[0].path()), b"{broken");
}

#[derive(Default)]
struct CountingInvalidator(std::sync::atomic::AtomicUsize);

impl DefinitionInvalidator for CountingInvalidator {
    fn invalidate_all(&self) {
        self.0.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

async fn restore(
    directory: &TempDir,
    registry: &JsonProfileRegistry,
) -> colui_app::RegistrySnapshot {
    let locks =
        colui_adapters::OperationLockManager::new(directory.path().join("registry.recovery.lock"));
    RestoreRegistryBackup::new(registry, &locks, &CountingInvalidator::default())
        .execute()
        .await
        .unwrap()
}

#[tokio::test]
async fn restore_missing_canonical_advances_from_backup_only() {
    let (directory, registry) = test_registry();
    let canonical = directory.path().join("registry.json");
    std::fs::write(
        &canonical,
        br#"{"schemaVersion":2,"registryRevision":9,"profiles":[]}"#,
    )
    .unwrap();
    registry.create_registry_backup().await.unwrap();
    std::fs::remove_file(&canonical).unwrap();
    assert_eq!(restore(&directory, &registry).await.registry_revision, 10);
}

#[tokio::test]
async fn restore_advances_matching_profile_above_canonical_and_backup() {
    let (directory, registry) = test_registry();
    let profile = profile_with_files(&["compose.yml"]);
    let id = profile.id.to_string();
    let record = |registry_revision, profile_revision| {
        serde_json::json!({
            "schemaVersion": 2, "registryRevision": registry_revision,
            "profiles": [{"id": id, "revision": profile_revision, "displayName": "Demo", "composeProjectName": "demo", "workingDirectory": "/tmp/demo", "composeFiles": ["compose.yml"], "environmentFiles": [], "registrationOrigin": "Manual"}]
        })
    };
    let canonical = directory.path().join("registry.json");
    std::fs::write(&canonical, serde_json::to_vec(&record(4, 7)).unwrap()).unwrap();
    registry.create_registry_backup().await.unwrap();
    std::fs::write(&canonical, serde_json::to_vec(&record(12, 20)).unwrap()).unwrap();
    let restored = restore(&directory, &registry).await;
    assert_eq!(restored.registry_revision, 13);
    assert_eq!(restored.profiles[0].revision.value(), 21);
}

#[tokio::test]
async fn restore_rejects_backup_replacement_while_waiting_for_registry_lock() {
    let (directory, registry) = test_registry();
    let canonical = directory.path().join("registry.json");
    std::fs::write(
        &canonical,
        br#"{"schemaVersion":2,"registryRevision":1,"profiles":[]}"#,
    )
    .unwrap();
    registry.create_registry_backup().await.unwrap();
    let registry_lock = std::fs::File::create(directory.path().join("registry.lock")).unwrap();
    fs2::FileExt::lock_exclusive(&registry_lock).unwrap();
    let registry = std::sync::Arc::new(registry);
    let task_registry = registry.clone();
    let lock_path = directory.path().join("registry.recovery.lock");
    let task = tokio::spawn(async move {
        let locks = colui_adapters::OperationLockManager::new(lock_path);
        RestoreRegistryBackup::new(
            task_registry.as_ref(),
            &locks,
            &CountingInvalidator::default(),
        )
        .execute()
        .await
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    std::fs::write(
        directory.path().join("registry.json.bak"),
        br#"{"schemaVersion":2,"registryRevision":2,"profiles":[]}"#,
    )
    .unwrap();
    drop(registry_lock);
    assert_eq!(
        task.await.unwrap().unwrap_err().code,
        AppErrorCode::RecoveryConflict
    );
}

#[tokio::test]
async fn operation_lease_precedes_registry_lock_and_blocks_local_recovery() {
    let (directory, registry) = test_registry();
    let registry_lock = std::fs::File::create(directory.path().join("registry.lock")).unwrap();
    fs2::FileExt::lock_exclusive(&registry_lock).unwrap();
    let locks = std::sync::Arc::new(colui_adapters::OperationLockManager::new(
        directory.path().join("registry.recovery.lock"),
    ));
    let registry = std::sync::Arc::new(registry);
    let task_locks = locks.clone();
    let task_registry = registry.clone();
    let task = tokio::spawn(async move {
        let _operation =
            colui_app::OperationLockManager::acquire_mutation(task_locks.as_ref()).unwrap();
        task_registry.mutate(Box::new(Ok)).await
    });
    tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    assert_eq!(
        colui_app::OperationLockManager::acquire_recovery(locks.as_ref())
            .unwrap_err()
            .code,
        AppErrorCode::OperationConflict
    );
    drop(registry_lock);
    task.await.unwrap().unwrap();
}

#[tokio::test]
async fn every_restore_attempt_prunes_completed_artifacts_to_three() {
    let (directory, registry) = test_registry();
    for index in 0..5 {
        std::fs::write(
            directory
                .path()
                .join(format!("registry.pre-restore.2026010{index}.x.json")),
            b"old",
        )
        .unwrap();
    }
    std::fs::write(directory.path().join("registry.json.bak"), b"{broken").unwrap();
    let locks =
        colui_adapters::OperationLockManager::new(directory.path().join("registry.recovery.lock"));
    RestoreRegistryBackup::new(&registry, &locks, &CountingInvalidator::default())
        .execute()
        .await
        .unwrap_err();
    let count = std::fs::read_dir(directory.path())
        .unwrap()
        .filter_map(Result::ok)
        .filter(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .starts_with("registry.pre-restore.")
        })
        .count();
    assert_eq!(count, 3);
}

#[tokio::test]
async fn successful_operation_clears_retained_write_failure_but_keeps_failure_timestamp() {
    let (directory, registry) = test_registry();
    let canonical = directory.path().join("registry.json");
    std::fs::write(
        &canonical,
        br#"{"schemaVersion":2,"registryRevision":18446744073709551615,"profiles":[]}"#,
    )
    .unwrap();
    registry
        .mutate(Box::new(|mut value| {
            value.profiles.push(profile_with_files(&["compose.yml"]));
            Ok(value)
        }))
        .await
        .unwrap_err();
    let failed = registry.registry_health().await.unwrap();
    assert_eq!(failed.state, RegistryHealthState::WriteFailure);
    assert!(failed.last_failure_at.is_some());
    std::fs::write(
        &canonical,
        br#"{"schemaVersion":2,"registryRevision":1,"profiles":[]}"#,
    )
    .unwrap();
    registry.create_registry_backup().await.unwrap();
    let healthy = registry.registry_health().await.unwrap();
    assert_eq!(healthy.state, RegistryHealthState::Healthy);
    assert!(healthy.last_failure_at.is_some());
}

#[cfg(unix)]
#[tokio::test]
async fn restore_rejects_unreadable_canonical_without_replacing_it() {
    let (directory, registry) = test_registry();
    let canonical = directory.path().join("registry.json");
    std::fs::write(
        &canonical,
        br#"{"schemaVersion":2,"registryRevision":1,"profiles":[]}"#,
    )
    .unwrap();
    registry.create_registry_backup().await.unwrap();
    std::fs::set_permissions(
        &canonical,
        std::os::unix::fs::PermissionsExt::from_mode(0o000),
    )
    .unwrap();
    let locks =
        colui_adapters::OperationLockManager::new(directory.path().join("registry.recovery.lock"));
    let result = RestoreRegistryBackup::new(&registry, &locks, &CountingInvalidator::default())
        .execute()
        .await;
    std::fs::set_permissions(
        &canonical,
        std::os::unix::fs::PermissionsExt::from_mode(0o600),
    )
    .unwrap();
    assert_eq!(result.unwrap_err().code, AppErrorCode::PermissionDenied);
}
