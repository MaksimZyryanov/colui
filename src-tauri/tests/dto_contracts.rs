use colui_app::LifecycleResult;
use colui_domain::{AppError, AppErrorCode, ProfileId};
use colui_tauri_lib::dto::{
    AppErrorDto, DefinitionStateDto, LifecycleResultDto, OperationKindDto, OperationPhaseDto,
    ProjectStatusDto, RegistrationOriginDto, RuntimeActivityDto, RuntimePresenceDto,
    RuntimeStateDto, SessionContextDto, UpdateProfileRequestDto,
};
use colui_tauri_lib::schema_generation::{generate_all_schemas, write_schemas};
use std::fs;

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
fn dto_boundary_rejects_noncanonical_profile_ids() {
    let request = serde_json::json!({
        "profileId": "00000000000000000000000000000001"
    });
    assert!(serde_json::from_value::<colui_tauri_lib::dto::ProfileIdRequestDto>(request).is_err());
}

#[test]
fn session_context_deserializes_only_canonical_session_ids() {
    let base = serde_json::json!({
        "endpoint": "unix:///var/run/docker.sock",
        "daemonFingerprint": {
            "daemonId": "daemon",
            "serverVersion": "1",
            "osType": "linux",
            "architecture": "amd64"
        },
        "connectedAt": "2026-09-02T12:00:00Z"
    });
    let valid = serde_json::json!({
        "sessionId": "00000000-0000-0000-0000-000000000001",
        "endpoint": base["endpoint"],
        "daemonFingerprint": base["daemonFingerprint"],
        "connectedAt": base["connectedAt"]
    });
    assert!(serde_json::from_value::<SessionContextDto>(valid).is_ok());
    for session_id in [
        "00000000000000000000000000000001",
        "00000000-0000-0000-0000-00000000000A",
    ] {
        let mut value = base.clone();
        value["sessionId"] = serde_json::json!(session_id);
        assert!(serde_json::from_value::<SessionContextDto>(value).is_err());
    }
}

#[test]
fn project_status_deserializes_only_canonical_profile_ids() {
    for (profile_id, valid) in [
        ("00000000-0000-0000-0000-000000000001", true),
        ("00000000000000000000000000000001", false),
        ("00000000-0000-0000-0000-00000000000A", false),
    ] {
        let value = serde_json::json!({
            "profileId": profile_id,
            "runtime": {
                "presence": "unavailable",
                "activity": null,
                "containerCount": 0,
                "runningContainerCount": 0,
                "observedAt": null
            },
            "definition": {"state": "unchecked"},
            "operation": null,
            "issues": []
        });
        let result = serde_json::from_value::<ProjectStatusDto>(value);
        assert_eq!(result.is_ok(), valid);
    }
}

#[test]
fn lifecycle_result_deserializes_only_canonical_profile_ids() {
    for (profile_id, valid) in [
        ("00000000-0000-0000-0000-000000000001", true),
        ("00000000000000000000000000000001", false),
        ("00000000-0000-0000-0000-00000000000A", false),
    ] {
        let value = serde_json::json!({"profileId": profile_id, "success": true});
        assert_eq!(
            serde_json::from_value::<LifecycleResultDto>(value).is_ok(),
            valid
        );
    }
}

#[test]
fn lifecycle_result_preserves_application_success() {
    let dto = LifecycleResultDto::from(LifecycleResult {
        profile_id: ProfileId::parse("00000000-0000-0000-0000-000000000001").unwrap(),
        success: false,
    });
    assert!(!dto.success);
}

#[test]
fn issue_dto_preserves_optional_field_mapping() {
    let value = serde_json::to_value(colui_tauri_lib::dto::IssueDto {
        field: Some("composeProjectName".to_owned()),
        message: "must be lowercase".to_owned(),
    })
    .unwrap();
    assert_eq!(
        value,
        serde_json::json!({"field":"composeProjectName","message":"must be lowercase"})
    );
}

#[test]
fn generated_schema_files_are_deterministic() {
    let generated = generate_all_schemas();
    assert_eq!(
        generated.get("LifecycleResultDto").unwrap()["properties"]["profileId"]["type"],
        "string"
    );
    assert_eq!(generated, generate_all_schemas());
    let bytes = serde_json::to_vec_pretty(&generated["LifecycleResultDto"]).unwrap();
    assert_eq!(bytes.last(), Some(&b'}'));
    assert_eq!(
        bytes,
        serde_json::to_vec_pretty(&generated["LifecycleResultDto"]).unwrap()
    );
    assert_eq!(generated.len(), 26);
    assert_eq!(
        generated["ProjectStatusDto"]["properties"]
            .as_object()
            .unwrap()
            .keys()
            .collect::<Vec<_>>(),
        vec!["definition", "issues", "operation", "profileId", "runtime"]
    );

    let first = std::env::temp_dir().join(format!("colui-schemas-first-{}", std::process::id()));
    let second = std::env::temp_dir().join(format!("colui-schemas-second-{}", std::process::id()));
    write_schemas(&first).unwrap();
    write_schemas(&second).unwrap();
    for entry in fs::read_dir(&first).unwrap() {
        let name = entry.unwrap().file_name();
        let first_path = first.join(&name);
        let second_path = second.join(&name);
        if first_path.is_dir() {
            for fixture in fs::read_dir(&first_path).unwrap() {
                let fixture_name = fixture.unwrap().file_name();
                assert_eq!(
                    fs::read(first_path.join(&fixture_name)).unwrap(),
                    fs::read(second_path.join(&fixture_name)).unwrap()
                );
            }
        } else {
            assert_eq!(
                fs::read(first_path).unwrap(),
                fs::read(second_path).unwrap()
            );
        }
    }
    fs::remove_dir_all(first).unwrap();
    fs::remove_dir_all(second).unwrap();
}

#[test]
fn schema_writer_removes_obsolete_generated_files() {
    let output = std::env::temp_dir().join(format!("colui-schemas-{}", std::process::id()));
    fs::create_dir_all(&output).unwrap();
    fs::write(output.join("LifecycleResultDto.json"), "{}\n").unwrap();
    fs::write(output.join("keep.json"), "keep\n").unwrap();

    write_schemas(&output).unwrap();

    assert_ne!(
        fs::read(output.join("LifecycleResultDto.json")).unwrap(),
        b"{}\n"
    );
    assert!(output.join("keep.json").exists());
    fs::remove_dir_all(output).unwrap();
}

#[test]
fn generated_schemas_constrain_uuid_and_rfc3339_fields() {
    let schemas = generate_all_schemas();
    assert_eq!(
        schemas["ProfileSummaryDto"]["properties"]["id"]["format"],
        "uuid"
    );
    assert_eq!(
        schemas["RuntimeStateDto"]["definitions"]["SessionContextDto"]["properties"]["sessionId"]
            ["format"],
        "uuid"
    );
    assert_eq!(
        schemas["RuntimeStateDto"]["definitions"]["SessionContextDto"]["properties"]["connectedAt"]
            ["format"],
        "date-time"
    );
    assert_eq!(
        schemas["ProjectStatusDto"]["definitions"]["RuntimeProjectionDto"]["properties"]
            ["observedAt"]["anyOf"][0]["$ref"],
        "#/definitions/Rfc3339Schema"
    );
    assert_eq!(
        schemas["ProjectStatusDto"]["definitions"]["Rfc3339Schema"]["format"],
        "date-time"
    );
    assert!(!schemas["AppErrorDto"]["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|field| field == "subjectId"));
    assert!(!schemas["RuntimeProjectionDto"]["required"]
        .as_array()
        .unwrap()
        .iter()
        .any(|field| field == "observedAt"));
}

#[test]
fn generated_fixture_manifest_has_expected_negative_cases() {
    let output = std::env::temp_dir().join(format!("colui-fixtures-{}", std::process::id()));
    write_schemas(&output).unwrap();
    let fixtures = fs::read_dir(output.join("fixtures"))
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .collect::<std::collections::BTreeSet<_>>();
    assert!(fixtures.contains("profile_summary_invalid_uuid.json"));
    assert!(fixtures.contains("project_status_invalid_runtime_combination.json"));
    assert_eq!(fixtures.len(), 16);
    fs::remove_dir_all(output).unwrap();
}
