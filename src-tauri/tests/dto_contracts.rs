use colui_app::{DefinitionProjection, LifecycleResult};
use colui_domain::{
    AppError, AppErrorCode, DefinitionRevision, DefinitionState, ProfileId, ProjectDefinition,
    RuntimeInventory, Timestamp,
};
use colui_tauri_lib::dto::{
    AppErrorDto, DefinitionStateDto, LifecycleResultDto, OperationKindDto, OperationPhaseDto,
    ProjectDefinitionDto, ProjectStatusDto, RegistrationOriginDto, RuntimeActivityDto,
    RuntimeInventoryDto, RuntimePresenceDto, RuntimeStateDto, SessionContextDto,
    UpdateProfileRequestDto,
};
use colui_tauri_lib::schema_generation::{generate_all_schemas, write_schemas};
use std::fs;

#[test]
fn increment_five_errors_preserve_typed_subjects() {
    for (subject, expected) in [
        (
            colui_domain::AppErrorSubject::profile(
                ProfileId::parse("00000000-0000-0000-0000-000000000001").unwrap(),
            ),
            "profile",
        ),
        (
            colui_domain::AppErrorSubject::candidate("a".repeat(64)),
            "candidate",
        ),
        (
            colui_domain::AppErrorSubject {
                kind: colui_domain::AppErrorSubjectKind::Container,
                id: "b".repeat(64),
            },
            "container",
        ),
        (
            colui_domain::AppErrorSubject::registry("registry"),
            "registry",
        ),
    ] {
        let id = subject.id.clone();
        let value = serde_json::to_value(AppErrorDto::from(AppError::for_subject(
            AppErrorCode::OperationConflict,
            "test",
            subject,
            "conflict",
        )))
        .unwrap();
        assert_eq!(
            value["subject"],
            serde_json::json!({"kind": expected, "id": id})
        );
        assert!(value.get("subjectId").is_none());
    }
}

#[test]
fn registry_adapter_errors_receive_registry_wire_subject() {
    for code in [
        AppErrorCode::RegistryCorrupt,
        AppErrorCode::RegistryLocked,
        AppErrorCode::RegistryWriteFailed,
        AppErrorCode::RecoveryConflict,
    ] {
        let value = serde_json::to_value(AppErrorDto::from(AppError::new(
            code,
            "restore_registry_backup",
            None,
            "registry failure",
        )))
        .unwrap();
        assert_eq!(
            value["subject"],
            serde_json::json!({"kind":"registry", "id":"registry"})
        );
    }
}

#[test]
fn discovery_dto_keeps_conflicting_runtime_and_registered_evidence() {
    use colui_domain::*;
    let session: RuntimeSessionId =
        serde_json::from_str("\"00000000-0000-0000-0000-000000000001\"").unwrap();
    let profile = ProjectProfile::from_draft(
        ProfileId::parse("00000000-0000-0000-0000-000000000002").unwrap(),
        ProfileDraft {
            display_name: "Demo".try_into().unwrap(),
            compose_project_name: "demo".try_into().unwrap(),
            working_directory: "/registered".into(),
            compose_files: vec!["compose.yml".into()],
            environment_files: vec![],
            registration_origin: RegistrationOrigin::Manual,
        },
    )
    .unwrap();
    let groups = ["/one", "/two"].map(|dir| ComposeObservationGroup {
        compose_project_name: "demo".into(),
        working_directory: Some(dir.into()),
        config_files: vec!["compose.yml".into()],
        container_ids: vec![ContainerId("container".into())],
    });
    let candidates: Vec<colui_tauri_lib::dto::DiscoveryCandidateDto> =
        classify_candidates(session, 7, &groups, &[profile])
            .into_iter()
            .map(Into::into)
            .collect();
    let value = serde_json::to_value(candidates).unwrap();
    assert_eq!(value[0]["classification"], "name_conflict");
    assert_eq!(value[0]["inventoryGeneration"], 7);
    let evidence = value[0]["conflicts"].as_array().unwrap();
    assert!(evidence.iter().any(|v| v["source"] == "runtime_observation"
        && v["profileId"].is_null()
        && !v["configFiles"].as_array().unwrap().is_empty()));
    assert!(evidence.iter().any(|v| v["source"] == "registered_profile"
        && v["profileId"] == "00000000-0000-0000-0000-000000000002"
        && v["workingDirectory"] == "/registered"));
}

#[test]
fn increment_five_schemas_cover_every_command_contract() {
    let schemas = generate_all_schemas();
    for name in [
        "DiscoveryCandidateDto",
        "DiscoveryConflictEvidenceDto",
        "DiscoveryListDto",
        "RegisterCandidateRequestDto",
        "IgnoreCandidateRequestDto",
        "ConfigureAutoRegistrationRequestDto",
        "AutoRegistrationConfigurationDto",
        "AutoRegistrationResultDto",
        "DiagnosticsSnapshotDto",
        "RegistrySnapshotIdentityDto",
        "RegistryHealthDto",
        "ReconnectResultDto",
        "ContainerActionRequestDto",
        "ContainerActionResultDto",
        "ContainerLogsRequestDto",
        "ContainerLogsDto",
        "OpenContainerPortRequestDto",
        "PortBindingActionDto",
        "ApplicationStateChangedDto",
        "JournalEntryDto",
    ] {
        assert!(schemas.contains_key(name), "missing schema: {name}");
    }
}

#[test]
fn profile_lifecycle_rejects_path_bearing_requests() {
    let value = serde_json::json!({
        "profileId": "00000000-0000-0000-0000-000000000001",
        "workingDirectory": "/private/secret", "composeFiles": ["secret.yml"]
    });
    assert!(serde_json::from_value::<colui_tauri_lib::dto::ProfileIdRequestDto>(value).is_err());
}

#[test]
fn browser_request_accepts_only_ids_session_and_binding_index() {
    use colui_tauri_lib::dto::OpenContainerPortRequestDto;
    let value = serde_json::json!({"containerId":"a".repeat(64), "runtimeSessionId":"00000000-0000-0000-0000-000000000001", "bindingIndex":0});
    assert!(serde_json::from_value::<OpenContainerPortRequestDto>(value.clone()).is_ok());
    for (key, input) in [
        ("url", serde_json::json!("https://evil.example")),
        ("bindingIndex", serde_json::json!(-1)),
        ("runtimeSessionId", serde_json::json!("invalid")),
    ] {
        let mut invalid = value.clone();
        invalid[key] = input;
        assert!(serde_json::from_value::<OpenContainerPortRequestDto>(invalid).is_err());
    }
}

#[test]
fn inventory_ports_include_backend_derived_actions_and_observation_evidence() {
    let binding = colui_tauri_lib::dto::PortBindingDto::from(colui_domain::PortBinding {
        host_ip: Some("0.0.0.0".into()),
        host_port: Some(49152),
        container_port: 443,
        protocol: "TCP".into(),
    });
    let json = serde_json::to_value(binding).unwrap();
    assert_eq!(json["action"]["copy"], "0.0.0.0:49152 -> 443/tcp");
    assert_eq!(json["action"]["url"], "https://127.0.0.1:49152");
    let inventory = RuntimeInventory {
        compose_observation_groups: vec![colui_domain::ComposeObservationGroup {
            compose_project_name: "demo".into(),
            working_directory: None,
            config_files: vec![],
            container_ids: vec![colui_domain::ContainerId("c1".into())],
        }],
        ..RuntimeInventory::unavailable()
    };
    let json = serde_json::to_value(RuntimeInventoryDto::from(inventory)).unwrap();
    assert_eq!(
        json["composeObservationGroups"][0]["containerIds"],
        serde_json::json!(["c1"])
    );
    assert!(json["composeObservationGroups"][0]["workingDirectory"].is_null());
}

#[test]
fn candidate_request_rejects_noncanonical_ids() {
    use colui_tauri_lib::dto::IgnoreCandidateRequestDto;
    for id in ["secret/path".to_owned(), "A".repeat(64), "a".repeat(63)] {
        assert!(serde_json::from_value::<IgnoreCandidateRequestDto>(
            serde_json::json!({"candidateId":id})
        )
        .is_err());
    }
}

#[test]
fn reconnect_action_logs_and_registry_keep_exact_payloads() {
    use colui_tauri_lib::dto::*;
    let reconnect = ReconnectResultDto::from(colui_app::ReconnectResult {
        runtime_state: colui_domain::RuntimeSessionState::Disconnected,
        inventory: None,
    });
    assert_eq!(
        serde_json::to_value(reconnect).unwrap(),
        serde_json::json!({"runtimeState":{"state":"disconnected"},"inventory":null})
    );
    let action = ContainerActionResultDto::from(colui_app::ContainerActionResult {
        container_id: colui_domain::ContainerId("container".into()),
        action: colui_app::ContainerAction::Restart,
        observation: colui_app::ContainerActionObservation::IndeterminateAfterSessionChange,
        inventory: RuntimeInventory {
            generation: 42,
            ..RuntimeInventory::unavailable()
        },
    });
    let value = serde_json::to_value(action).unwrap();
    assert_eq!(value["observation"], "indeterminate_after_session_change");
    assert_eq!(value["inventory"]["generation"], 42);
    let logs = ContainerLogsDto::from(colui_app::ContainerLogs {
        container_id: colui_domain::ContainerId("container".into()),
        text: "first\n\u{fffd}\0last\n".into(),
        retained_bytes: 14,
        truncated: true,
        observed_at: Timestamp("2026-09-05T00:00:00Z".into()),
    });
    let json = serde_json::to_value(logs).unwrap();
    assert_eq!(json["text"], "first\n\u{fffd}\0last\n");
    assert_eq!(json["retainedBytes"], 14);
    let health = RegistryHealthDto::from(colui_app::RegistryHealth {
        state: colui_app::RegistryHealthState::Corrupt,
        identity: None,
        error: None,
        last_operation_at: None,
        last_failure_at: None,
    });
    let value = serde_json::to_value(health).unwrap();
    assert!(value.as_object().unwrap().contains_key("identity"));
    assert!(value["identity"].is_null());
}

#[test]
fn operational_journal_preserves_codes_and_subjects_without_secret_sources() {
    tauri::async_runtime::block_on(async {
        let session = colui_app::DiscoverySession::new();
        for (kind, code) in [
            (
                colui_app::JournalEventKind::ConnectFailed,
                AppErrorCode::RuntimeConnectionFailed,
            ),
            (
                colui_app::JournalEventKind::DisconnectFailed,
                AppErrorCode::OperationConflict,
            ),
            (
                colui_app::JournalEventKind::ReconnectFailed,
                AppErrorCode::RuntimeUnavailable,
            ),
            (
                colui_app::JournalEventKind::ManualRegistrationFailed,
                AppErrorCode::CandidateStale,
            ),
            (
                colui_app::JournalEventKind::AutoRegistrationFailed,
                AppErrorCode::RegistryCorrupt,
            ),
            (
                colui_app::JournalEventKind::BackupFailed,
                AppErrorCode::RegistryWriteFailed,
            ),
            (
                colui_app::JournalEventKind::RestoreFailed,
                AppErrorCode::RecoveryConflict,
            ),
            (
                colui_app::JournalEventKind::ProfileLifecycleFailed,
                AppErrorCode::ComposeFailed,
            ),
            (
                colui_app::JournalEventKind::ContainerActionFailed,
                AppErrorCode::ContainerOperationFailed,
            ),
        ] {
            let subject = colui_domain::AppErrorSubject {
                kind: colui_domain::AppErrorSubjectKind::Container,
                id: "a".repeat(64),
            };
            let error = AppError::for_subject(
                code,
                "SECRET_OPERATION",
                subject.clone(),
                "SECRET_PATH_ENV_LABEL_REGISTRY_LOG",
            )
            .with_details("SECRET_UPSTREAM");
            session
                .record_operation(kind, None, Some(subject), Some(&error))
                .await;
        }
        let entries: Vec<colui_tauri_lib::dto::JournalEntryDto> = session
            .journal()
            .await
            .entries
            .into_iter()
            .map(Into::into)
            .collect();
        let value = serde_json::to_value(entries).unwrap();
        assert!(!value.to_string().contains("SECRET"));
        assert_eq!(value[0]["errorCode"], "runtime_connection_failed");
        assert_eq!(value[8]["subject"]["kind"], "container");
        assert_eq!(value[8]["sequence"], 9);
        assert!(value
            .as_array()
            .unwrap()
            .iter()
            .all(|v| v["errorCode"].is_string()
                && v["message"].as_str().unwrap().chars().count() <= 500));
    });
}

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
    assert_eq!(value["profileId"], "id-1");
    assert_eq!(value["success"], true);
    assert_eq!(value["inventoryGeneration"], 0);
}

#[test]
fn inventory_dto_contains_snapshot_marker_and_full_container_list() {
    let json = serde_json::to_value(RuntimeInventoryDto::fixture()).unwrap();
    assert_eq!(json["hasSnapshot"], true);
    assert!(json["containers"].is_array());
    assert!(json["projects"].is_array());
    assert!(json["standaloneContainers"].is_array());
}

#[test]
fn definition_dto_keeps_definition_and_sanitized_error_together() {
    let profile_id = ProfileId::parse("00000000-0000-0000-0000-000000000001").unwrap();
    let dto = ProjectDefinitionDto::from(DefinitionProjection {
        definition: ProjectDefinition {
            profile_id: profile_id.clone(),
            definition_revision: DefinitionRevision("retained".into()),
            loaded_at: Timestamp("2026-09-03T00:00:00Z".into()),
            state: DefinitionState::Stale,
            services: vec![],
            issues: vec![],
        },
        error: Some(AppError::new(
            AppErrorCode::DefinitionFailed,
            "definition",
            Some(profile_id),
            "compose config failed",
        )),
    });
    let value = serde_json::to_value(dto).unwrap();
    assert_eq!(value["state"], "stale");
    assert_eq!(value["definitionRevision"], "retained");
    assert_eq!(value["error"]["code"], "definition_failed");
    let serialized = value.to_string();
    assert!(!serialized.contains("/Users/"));
    assert!(!serialized.contains("private.env"));
    assert!(!serialized.contains("top-secret"));
}

#[test]
fn lifecycle_generation_matches_embedded_inventory() {
    let dto = LifecycleResultDto::fixture();
    assert_eq!(dto.inventory_generation, dto.inventory.generation);
}

#[test]
fn inventory_rejects_noncanonical_runtime_session_ids() {
    let mut value =
        serde_json::to_value(RuntimeInventoryDto::from(RuntimeInventory::unavailable())).unwrap();
    value["runtimeSessionId"] = serde_json::json!("00000000000000000000000000000001");
    assert!(serde_json::from_value::<RuntimeInventoryDto>(value).is_err());
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
fn absent_project_status_serializes_and_decodes_without_runtime_timestamp() {
    let profile = colui_domain::ProjectProfile::from_draft(
        colui_domain::ProfileId::parse("00000000-0000-0000-0000-000000000001").unwrap(),
        colui_domain::ProfileDraft {
            display_name: "Demo".try_into().unwrap(),
            compose_project_name: "demo".try_into().unwrap(),
            working_directory: "/tmp/demo".into(),
            compose_files: vec!["compose.yml".into()],
            environment_files: vec![],
            registration_origin: colui_domain::RegistrationOrigin::Manual,
        },
    )
    .unwrap();
    let inventory = RuntimeInventory {
        generation: 1,
        has_snapshot: true,
        observed_at: Some(colui_domain::Timestamp("2026-09-03T00:00:00Z".into())),
        ..RuntimeInventory::unavailable()
    };
    let dto = ProjectStatusDto::from(colui_app::project_status_from_inventory(
        &profile, inventory,
    ));
    let value = serde_json::to_value(dto).unwrap();

    assert_eq!(value["runtime"]["presence"], "absent");
    assert!(value["runtime"]["activity"].is_null());
    assert!(value["runtime"]["observedAt"].is_null());
    assert!(serde_json::from_value::<ProjectStatusDto>(value).is_ok());
}

#[test]
fn lifecycle_result_deserializes_only_canonical_profile_ids() {
    for (profile_id, valid) in [
        ("00000000-0000-0000-0000-000000000001", true),
        ("00000000000000000000000000000001", false),
        ("00000000-0000-0000-0000-00000000000A", false),
    ] {
        let value = serde_json::json!({"profileId": profile_id, "success": true, "inventoryGeneration": 0, "inventory": RuntimeInventoryDto::from(RuntimeInventory::unavailable())});
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
        inventory: RuntimeInventory::unavailable(),
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
    assert_eq!(generated.len(), 79);
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
    assert_eq!(fixtures.len(), 18);
    fs::remove_dir_all(output).unwrap();
}
