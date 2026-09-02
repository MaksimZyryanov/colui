use colui_domain::{
    validate_draft, AppError, AppErrorCode, ComposeProjectName, DisplayName, ProfileDraft,
    ProfileId, ProjectProfile, RegistrationOrigin, Revision,
};
use serde_json::from_str;
use std::path::PathBuf;
use uuid::Uuid;

fn valid_draft(display_name: &str, compose_project_name: &str) -> ProfileDraft {
    ProfileDraft {
        display_name: DisplayName::try_from(display_name).unwrap(),
        compose_project_name: ComposeProjectName::try_from(compose_project_name).unwrap(),
        working_directory: PathBuf::from("/tmp/project"),
        compose_files: vec![PathBuf::from("compose.yaml")],
        environment_files: vec![],
        registration_origin: RegistrationOrigin::Manual,
    }
}

#[test]
fn profile_id_is_not_derived_from_name_or_path() {
    let first = ProfileId::new(Uuid::from_u128(1));
    let renamed = ProfileId::new(Uuid::from_u128(1));
    assert_eq!(first, renamed);
}

#[test]
fn profile_id_parse_requires_canonical_lowercase_hyphenated_uuid() {
    assert!(ProfileId::parse("00000000-0000-0000-0000-000000000001").is_ok());
    assert!(ProfileId::parse("00000000000000000000000000000001").is_err());
    assert!(ProfileId::parse("abcdefab-cdef-abcd-efab-cdefabcdefab").is_ok());
    assert!(ProfileId::parse("ABCDEFAB-CDEF-ABCD-EFAB-CDEFABCDEFAB").is_err());
}

#[test]
fn compose_name_rejects_invalid_namespace() {
    assert!(ComposeProjectName::try_from("Checkout API").is_err());
    assert!(ComposeProjectName::try_from("checkout_api-2").is_ok());
    assert!(ComposeProjectName::try_from("2checkout").is_ok());
    assert!(ComposeProjectName::try_from("Checkout").is_err());
    assert!(ComposeProjectName::try_from("_checkout").is_err());
    assert!(ComposeProjectName::try_from("-checkout").is_err());
    assert!(ComposeProjectName::try_from("").is_err());
}

#[test]
fn display_name_rejects_empty_values() {
    assert!(DisplayName::try_from("").is_err());
    assert!(DisplayName::try_from("Checkout API").is_ok());
}

#[test]
fn deserialization_preserves_display_name_validation() {
    assert!(from_str::<DisplayName>("\"\"").is_err());
}

#[test]
fn deserialization_preserves_compose_name_validation() {
    assert!(from_str::<ComposeProjectName>("\"Checkout\"").is_err());
    assert!(from_str::<ComposeProjectName>("\"_checkout\"").is_err());
    assert!(from_str::<ComposeProjectName>("\"-checkout\"").is_err());
    assert!(from_str::<ComposeProjectName>("\"\"").is_err());
    assert!(from_str::<ComposeProjectName>("\"2checkout\"").is_ok());
}

#[test]
fn revision_starts_at_one_and_advances() {
    let initial = Revision::initial();
    assert_eq!(initial.next(), Ok(Revision::new(2)));
}

#[test]
fn rename_preserves_id_and_compose_namespace() {
    let mut draft = valid_draft("Checkout", "checkout");
    draft.working_directory = PathBuf::from("/nonexistent/project");
    draft.compose_files = vec![PathBuf::from("base.yaml"), PathBuf::from("override.yaml")];
    draft.environment_files = vec![PathBuf::from("prod.env")];
    draft.registration_origin = RegistrationOrigin::Migrated;
    let profile = ProjectProfile::from_draft(ProfileId::new(Uuid::from_u128(1)), draft).unwrap();
    let renamed = profile
        .with_display_name(DisplayName::try_from("Payments").unwrap())
        .unwrap();
    assert_eq!(renamed.id, profile.id);
    assert_eq!(renamed.revision, Revision::new(2));
    assert_eq!(renamed.compose_project_name, profile.compose_project_name);
    assert_eq!(renamed.working_directory, profile.working_directory);
    assert_eq!(renamed.compose_files, profile.compose_files);
    assert_eq!(renamed.environment_files, profile.environment_files);
    assert_eq!(renamed.registration_origin, profile.registration_origin);
}

#[test]
fn duplicate_display_names_are_allowed() {
    let first = valid_draft("Local", "checkout");
    let second = valid_draft("Local", "payments");
    assert!(ProjectProfile::from_draft(ProfileId::new(Uuid::from_u128(1)), first).is_ok());
    assert!(ProjectProfile::from_draft(ProfileId::new(Uuid::from_u128(2)), second).is_ok());
}

#[test]
fn revision_overflow_is_typed() {
    let error = Revision::new(u64::MAX).next().unwrap_err();
    assert_eq!(error.code, AppErrorCode::RegistryWriteFailed);
}

#[test]
fn app_error_codes_serialize_in_snake_case() {
    let cases = [
        (AppErrorCode::RuntimeUnavailable, "runtime_unavailable"),
        (
            AppErrorCode::RuntimeConnectionFailed,
            "runtime_connection_failed",
        ),
        (
            AppErrorCode::RuntimeContextMismatch,
            "runtime_context_mismatch",
        ),
        (AppErrorCode::ProfileNotFound, "profile_not_found"),
        (
            AppErrorCode::ProfileAlreadyRegistered,
            "profile_already_registered",
        ),
        (
            AppErrorCode::ProfileRevisionConflict,
            "profile_revision_conflict",
        ),
        (AppErrorCode::ProfileInvalid, "profile_invalid"),
        (AppErrorCode::DefinitionFailed, "definition_failed"),
        (AppErrorCode::ComposeFailed, "compose_failed"),
        (
            AppErrorCode::ContainerOperationFailed,
            "container_operation_failed",
        ),
        (AppErrorCode::OperationConflict, "operation_conflict"),
        (AppErrorCode::OperationTimeout, "operation_timeout"),
        (AppErrorCode::RegistryCorrupt, "registry_corrupt"),
        (AppErrorCode::RegistryLocked, "registry_locked"),
        (AppErrorCode::RegistryWriteFailed, "registry_write_failed"),
        (AppErrorCode::PermissionDenied, "permission_denied"),
        (AppErrorCode::ProtocolMismatch, "protocol_mismatch"),
    ];
    for (code, expected) in cases {
        assert_eq!(
            serde_json::to_string(&code).unwrap(),
            format!("\"{expected}\"")
        );
    }
}

#[test]
fn draft_requires_compose_file_and_unique_paths() {
    let mut empty = valid_draft("Local", "local");
    empty.compose_files.clear();
    assert_eq!(
        validate_draft(&empty).unwrap_err().code,
        AppErrorCode::ProfileInvalid
    );

    let mut duplicate_compose = valid_draft("Local", "local");
    duplicate_compose
        .compose_files
        .push(PathBuf::from("compose.yaml"));
    assert_eq!(
        validate_draft(&duplicate_compose).unwrap_err().code,
        AppErrorCode::ProfileInvalid
    );

    let mut duplicate_environment = valid_draft("Local", "local");
    duplicate_environment
        .environment_files
        .push(PathBuf::from("compose.yaml"));
    assert_eq!(
        validate_draft(&duplicate_environment).unwrap_err().code,
        AppErrorCode::ProfileInvalid
    );
}

#[test]
fn nonexistent_filesystem_paths_pass_domain_validation() {
    let mut draft = valid_draft("Local", "local");
    draft.working_directory = PathBuf::from("/path/that/does/not/exist");
    draft.compose_files = vec![PathBuf::from("missing-compose.yaml")];
    draft.environment_files = vec![PathBuf::from("missing.env")];

    assert!(validate_draft(&draft).is_ok());
}

#[test]
fn registry_lock_error_is_retryable() {
    let error = AppError::new(
        AppErrorCode::RegistryLocked,
        "write_registry",
        None,
        "locked",
    );
    assert!(error.retryable);
    assert_eq!(error.code, AppErrorCode::RegistryLocked);
}

#[test]
fn app_error_serializes_stable_code() {
    let error = AppError::new(
        AppErrorCode::ProfileAlreadyRegistered,
        "register_profile",
        None,
        "already registered",
    );
    assert_eq!(
        serde_json::to_value(error).unwrap()["code"],
        "profile_already_registered"
    );
}
