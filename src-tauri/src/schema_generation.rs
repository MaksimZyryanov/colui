use crate::dto::*;
use schemars::schema_for;
use serde_json::{json, Value};
use std::{collections::BTreeMap, fs, io, path::Path};

pub fn generate_all_schemas() -> BTreeMap<String, Value> {
    [
        ("AppErrorCodeDto", schema_for!(AppErrorCodeDto)),
        ("AppErrorDto", schema_for!(AppErrorDto)),
        (
            "DefinitionProjectionDto",
            schema_for!(DefinitionProjectionDto),
        ),
        ("DefinitionStateDto", schema_for!(DefinitionStateDto)),
        ("DaemonFingerprintDto", schema_for!(DaemonFingerprintDto)),
        ("IssueDto", schema_for!(IssueDto)),
        ("LifecycleResultDto", schema_for!(LifecycleResultDto)),
        ("MismatchDetailsDto", schema_for!(MismatchDetailsDto)),
        ("OperationDto", schema_for!(OperationDto)),
        ("OperationKindDto", schema_for!(OperationKindDto)),
        ("OperationPhaseDto", schema_for!(OperationPhaseDto)),
        ("ProfileDetailsDto", schema_for!(ProfileDetailsDto)),
        ("ProfileDraftDto", schema_for!(ProfileDraftDto)),
        ("ProfileIdRequestDto", schema_for!(ProfileIdRequestDto)),
        ("ProfilePatchDto", schema_for!(ProfilePatchDto)),
        ("ProfileSummaryDto", schema_for!(ProfileSummaryDto)),
        ("ProfileValidationDto", schema_for!(ProfileValidationDto)),
        ("RegistrationOriginDto", schema_for!(RegistrationOriginDto)),
        (
            "RemoveProfileRequestDto",
            schema_for!(RemoveProfileRequestDto),
        ),
        ("RuntimeActivityDto", schema_for!(RuntimeActivityDto)),
        ("RuntimePresenceDto", schema_for!(RuntimePresenceDto)),
        ("RuntimeProjectionDto", schema_for!(RuntimeProjectionDto)),
        ("RuntimeStateDto", schema_for!(RuntimeStateDto)),
        ("SessionContextDto", schema_for!(SessionContextDto)),
        ("ProjectStatusDto", schema_for!(ProjectStatusDto)),
        (
            "UpdateProfileRequestDto",
            schema_for!(UpdateProfileRequestDto),
        ),
    ]
    .into_iter()
    .map(|(name, schema)| (name.to_owned(), serde_json::to_value(schema).unwrap()))
    .collect()
}

pub fn write_schemas(root: impl AsRef<Path>) -> io::Result<()> {
    let root = root.as_ref();
    fs::create_dir_all(root.join("fixtures"))?;
    remove_known_artifacts(root)?;
    remove_known_artifacts(&root.join("fixtures"))?;
    for (name, schema) in generate_all_schemas() {
        let bytes = serde_json::to_vec_pretty(&schema).unwrap();
        fs::write(
            root.join(format!("{name}.json")),
            [bytes.as_slice(), b"\n"].concat(),
        )?;
    }
    for (name, fixture) in fixtures() {
        let bytes = serde_json::to_vec_pretty(&fixture).unwrap();
        fs::write(
            root.join("fixtures").join(name),
            [bytes.as_slice(), b"\n"].concat(),
        )?;
    }
    Ok(())
}

fn remove_known_artifacts(directory: &Path) -> io::Result<()> {
    let names: Vec<String> = if directory.ends_with("fixtures") {
        fixtures()
            .into_iter()
            .map(|(name, _)| name.to_owned())
            .collect()
    } else {
        generate_all_schemas()
            .keys()
            .map(|name| format!("{name}.json"))
            .collect::<Vec<_>>()
    };
    for name in names {
        let path = directory.join(name);
        if path.is_file() {
            fs::remove_file(path)?;
        }
    }
    Ok(())
}

fn fixtures() -> Vec<(&'static str, Value)> {
    let fingerprint = json!({"daemonId":"daemon-1","serverVersion":"27.0","osType":"linux","architecture":"amd64"});
    vec![
        (
            "profile_summary_valid.json",
            json!({"id":"00000000-0000-0000-0000-000000000001","revision":1,"displayName":"Demo","composeProjectName":"demo","workingDirectory":"/tmp/demo","registrationOrigin":"manual"}),
        ),
        (
            "profile_summary_invalid_missing_id.json",
            json!({"revision":1,"displayName":"Demo","composeProjectName":"demo","workingDirectory":"/tmp/demo","registrationOrigin":"manual"}),
        ),
        (
            "runtime_unavailable.json",
            json!({"state":"failed","error":{"code":"runtime_unavailable","operation":"connect_runtime","subjectId":null,"message":"runtime unavailable","details":null,"retryable":false}}),
        ),
        (
            "runtime_context_mismatch.json",
            json!({"state":"contextMismatch","details":{"endpoint":"unix:///var/run/docker.sock","apiFingerprint":fingerprint,"cliFingerprint":{"daemonId":"daemon-2","serverVersion":"27.0","osType":"linux","architecture":"amd64"}}}),
        ),
        (
            "project_status_runtime_unavailable.json",
            json!({"profileId":"00000000-0000-0000-0000-000000000001","runtime":{"presence":"unavailable","activity":null,"containerCount":0,"runningContainerCount":0,"observedAt":null},"definition":{"state":"unchecked","revision":null,"serviceCount":null},"operation":null,"issues":[]}),
        ),
        (
            "lifecycle_result_valid.json",
            json!({"profileId":"00000000-0000-0000-0000-000000000001","success":true}),
        ),
        (
            "profile_validation_valid.json",
            json!({"valid":true,"issues":[]}),
        ),
        (
            "profile_summary_invalid_enum.json",
            json!({"id":"id-1","revision":1,"displayName":"Demo","composeProjectName":"demo","workingDirectory":"/tmp/demo","registrationOrigin":"unknown"}),
        ),
        (
            "profile_summary_invalid_uuid.json",
            json!({"id":"not-a-uuid","revision":1,"displayName":"Demo","composeProjectName":"demo","workingDirectory":"/tmp/demo","registrationOrigin":"manual"}),
        ),
        (
            "project_status_invalid_runtime_combination.json",
            json!({"profileId":"00000000-0000-0000-0000-000000000001","runtime":{"presence":"unavailable","activity":"mixed","containerCount":2,"runningContainerCount":1,"observedAt":"2026-09-02T00:00:00Z"},"definition":{"state":"unchecked","revision":null,"serviceCount":null},"operation":null,"issues":[]}),
        ),
        (
            "session_context_invalid_timestamp.json",
            json!({"sessionId":"00000000-0000-0000-0000-000000000001","endpoint":"unix:///tmp/docker.sock","daemonFingerprint":fingerprint,"connectedAt":"not-a-timestamp"}),
        ),
        (
            "profile_details_invalid_missing_required.json",
            json!({"profile":{"id":"id-1"},"composeFiles":[]}),
        ),
    ]
}
