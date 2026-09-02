use colui_app::LifecycleResult;
use colui_domain::{AppError, AppErrorCode, ProfileId};
use colui_tauri_lib::dto::{
    AppErrorDto, DefinitionStateDto, LifecycleResultDto, OperationKindDto, OperationPhaseDto,
    RegistrationOriginDto, RuntimeActivityDto, RuntimePresenceDto, RuntimeStateDto,
    UpdateProfileRequestDto,
};

#[test]
fn update_request_serializes_only_camel_case_metadata_and_patch() {
    let request = UpdateProfileRequestDto::fixture();
    let value = serde_json::to_value(request).unwrap();
    assert_eq!(value["profileId"], "00000000-0000-0000-0000-000000000001");
    assert_eq!(value["expectedRevision"], 3);
    assert!(value["patch"].get("id").is_none());
}

#[test]
fn lifecycle_result_contains_profile_id_and_success() {
    let value = serde_json::to_value(LifecycleResultDto::success("id-1")).unwrap();
    assert_eq!(
        value,
        serde_json::json!({"profileId":"id-1","success":true})
    );
}

#[test]
fn status_enums_use_approved_wire_values() {
    let cases = [
        (
            serde_json::to_value(RuntimePresenceDto::Unavailable).unwrap(),
            "unavailable",
        ),
        (
            serde_json::to_value(RuntimePresenceDto::Absent).unwrap(),
            "absent",
        ),
        (
            serde_json::to_value(RuntimePresenceDto::Present).unwrap(),
            "present",
        ),
        (
            serde_json::to_value(RuntimeActivityDto::AllRunning).unwrap(),
            "all-running",
        ),
        (
            serde_json::to_value(RuntimeActivityDto::Mixed).unwrap(),
            "mixed",
        ),
        (
            serde_json::to_value(RuntimeActivityDto::NoneRunning).unwrap(),
            "none-running",
        ),
        (
            serde_json::to_value(DefinitionStateDto::Unchecked).unwrap(),
            "unchecked",
        ),
        (
            serde_json::to_value(DefinitionStateDto::Valid).unwrap(),
            "valid",
        ),
        (
            serde_json::to_value(DefinitionStateDto::Invalid).unwrap(),
            "invalid",
        ),
        (
            serde_json::to_value(DefinitionStateDto::Stale).unwrap(),
            "stale",
        ),
        (
            serde_json::to_value(OperationKindDto::Apply).unwrap(),
            "apply",
        ),
        (
            serde_json::to_value(OperationKindDto::Stop).unwrap(),
            "stop",
        ),
        (
            serde_json::to_value(OperationKindDto::TearDown).unwrap(),
            "tear-down",
        ),
        (
            serde_json::to_value(OperationKindDto::Restart).unwrap(),
            "restart",
        ),
        (
            serde_json::to_value(OperationPhaseDto::Queued).unwrap(),
            "queued",
        ),
        (
            serde_json::to_value(OperationPhaseDto::Running).unwrap(),
            "running",
        ),
        (
            serde_json::to_value(OperationPhaseDto::Succeeded).unwrap(),
            "succeeded",
        ),
        (
            serde_json::to_value(OperationPhaseDto::Failed).unwrap(),
            "failed",
        ),
        (
            serde_json::to_value(RegistrationOriginDto::Manual).unwrap(),
            "manual",
        ),
        (
            serde_json::to_value(RegistrationOriginDto::Discovered).unwrap(),
            "discovered",
        ),
        (
            serde_json::to_value(RegistrationOriginDto::Migrated).unwrap(),
            "migrated",
        ),
    ];
    for (actual, expected) in cases {
        assert_eq!(actual, expected);
    }
}

#[test]
fn every_stable_error_code_uses_snake_case() {
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
        let dto = AppErrorDto::from(AppError::new(code, "test", None, "test"));
        assert_eq!(serde_json::to_value(dto).unwrap()["code"], expected);
    }
}

#[test]
fn dto_boundary_rejects_non_rfc3339_timestamps() {
    let runtime = serde_json::json!({
        "state": "ready",
        "context": {
            "sessionId": "00000000-0000-0000-0000-000000000001",
            "endpoint": "unix:///var/run/docker.sock",
            "daemonFingerprint": {
                "daemonId": "daemon",
                "serverVersion": "1",
                "osType": "linux",
                "architecture": "amd64"
            },
            "connectedAt": "not-a-timestamp"
        }
    });
    assert!(serde_json::from_value::<RuntimeStateDto>(runtime).is_err());
}

#[test]
fn lifecycle_result_preserves_application_success() {
    let dto = LifecycleResultDto::from(LifecycleResult {
        profile_id: ProfileId::parse("00000000-0000-0000-0000-000000000001").unwrap(),
        success: false,
    });
    assert!(!dto.success);
}
