use super::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RegistrySnapshotIdentityDto {
    pub registry_revision: u64,
    #[schemars(regex(pattern = "^[0-9a-f]{64}$"))]
    pub canonical_content_sha256: String,
}
impl From<colui_app::RegistrySnapshotIdentity> for RegistrySnapshotIdentityDto {
    fn from(v: colui_app::RegistrySnapshotIdentity) -> Self {
        Self {
            registry_revision: v.registry_revision,
            canonical_content_sha256: v.canonical_content_sha256,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RegistryHealthStateDto {
    Healthy,
    Missing,
    Corrupt,
    Unreadable,
    Locked,
    WriteFailure,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RegistryHealthDto {
    pub state: RegistryHealthStateDto,
    pub identity: Option<RegistrySnapshotIdentityDto>,
    pub error: Option<AppErrorDto>,
    #[schemars(with = "Option<super::Rfc3339Schema>")]
    #[serde(default, deserialize_with = "super::deserialize_optional_rfc3339")]
    pub last_operation_at: Option<String>,
    #[schemars(with = "Option<super::Rfc3339Schema>")]
    #[serde(default, deserialize_with = "super::deserialize_optional_rfc3339")]
    pub last_failure_at: Option<String>,
}
impl From<colui_app::RegistryHealth> for RegistryHealthDto {
    fn from(v: colui_app::RegistryHealth) -> Self {
        use colui_app::RegistryHealthState as S;
        Self {
            state: match v.state {
                S::Healthy => RegistryHealthStateDto::Healthy,
                S::Missing => RegistryHealthStateDto::Missing,
                S::Corrupt => RegistryHealthStateDto::Corrupt,
                S::Unreadable => RegistryHealthStateDto::Unreadable,
                S::Locked => RegistryHealthStateDto::Locked,
                S::WriteFailure => RegistryHealthStateDto::WriteFailure,
            },
            identity: v.identity.map(Into::into),
            error: v.error.map(Into::into),
            last_operation_at: v.last_operation_at.map(|v| v.0),
            last_failure_at: v.last_failure_at.map(|v| v.0),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeDiagnosticsDto {
    pub state: RuntimeStateDto,
    pub resolved_endpoint: Option<String>,
    pub api_fingerprint: Option<DaemonFingerprintDto>,
    pub cli_fingerprint: Option<DaemonFingerprintDto>,
    #[schemars(with = "Option<super::UuidSchema>")]
    #[serde(
        default,
        deserialize_with = "super::deserialize_optional_canonical_uuid"
    )]
    pub session_id: Option<String>,
    #[schemars(with = "Option<super::Rfc3339Schema>")]
    #[serde(default, deserialize_with = "super::deserialize_optional_rfc3339")]
    pub connected_at: Option<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryResultDto {
    pub success: bool,
    pub error: Option<AppErrorDto>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RegistryDiagnosticsDto {
    pub registry_path: String,
    pub backup_path: String,
    pub backup: RegistryBackupDiagnosticsDto,
    pub revision: Option<u64>,
    pub health: RegistryHealthDto,
    pub lock_timeout_ms: u64,
    pub last_recovery_result: Option<RecoveryResultDto>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum BackupValidationStateDto {
    Missing,
    Valid,
    Corrupt,
    Unreadable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RegistryBackupDiagnosticsDto {
    pub exists: bool,
    #[schemars(with = "Option<super::Rfc3339Schema>")]
    #[serde(default, deserialize_with = "super::deserialize_optional_rfc3339")]
    pub modified_at: Option<String>,
    pub state: BackupValidationStateDto,
    pub error: Option<AppErrorDto>,
}

impl From<colui_app::RegistryBackupDiagnostics> for RegistryBackupDiagnosticsDto {
    fn from(v: colui_app::RegistryBackupDiagnostics) -> Self {
        use colui_app::BackupValidationState as S;
        Self {
            exists: v.exists,
            modified_at: v.modified_at.map(|v| v.0),
            error: v.error.map(Into::into),
            state: match v.state {
                S::Missing => BackupValidationStateDto::Missing,
                S::Valid => BackupValidationStateDto::Valid,
                S::Corrupt => BackupValidationStateDto::Corrupt,
                S::Unreadable => BackupValidationStateDto::Unreadable,
            },
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ImportDiagnosticsDto {
    pub source_path: String,
    pub imported_count: u64,
    pub source_preserved: bool,
    pub error: Option<AppErrorDto>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ActiveOperationPhaseDto {
    Pending,
    Active,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ActiveOperationDto {
    pub kind: String,
    pub subject_id: String,
    #[schemars(schema_with = "super::rfc3339_schema")]
    #[serde(deserialize_with = "super::deserialize_rfc3339")]
    pub started_at: String,
    pub phase: ActiveOperationPhaseDto,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OperationsDiagnosticsDto {
    pub generation: u64,
    pub active: Vec<ActiveOperationDto>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProfileDefinitionDiagnosticsDto {
    #[schemars(schema_with = "super::uuid_schema")]
    #[serde(deserialize_with = "super::deserialize_canonical_uuid")]
    pub profile_id: String,
    pub definition: Option<ProjectDefinitionDto>,
    pub error: Option<AppErrorDto>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DefinitionsDiagnosticsDto {
    pub generation: u64,
    pub profiles: Vec<ProfileDefinitionDiagnosticsDto>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JournalEventKindDto {
    ConnectStarted,
    ConnectSucceeded,
    ConnectFailed,
    DisconnectStarted,
    DisconnectSucceeded,
    DisconnectFailed,
    ReconnectStarted,
    ReconnectSucceeded,
    ReconnectFailed,
    ManualRegistrationStarted,
    ManualRegistrationSucceeded,
    ManualRegistrationFailed,
    AutoRegistrationStarted,
    AutoRegistrationSucceeded,
    AutoRegistrationFailed,
    BackupStarted,
    BackupSucceeded,
    BackupFailed,
    RestoreStarted,
    RestoreSucceeded,
    RestoreFailed,
    ProfileLifecycleStarted,
    ProfileLifecycleSucceeded,
    ProfileLifecycleFailed,
    ContainerActionStarted,
    ContainerActionSucceeded,
    ContainerActionFailed,
    CandidateIgnored,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum JournalSeverityDto {
    Info,
    Error,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct JournalEntryDto {
    pub sequence: u64,
    #[schemars(schema_with = "super::rfc3339_schema")]
    #[serde(deserialize_with = "super::deserialize_rfc3339")]
    pub timestamp: String,
    #[schemars(with = "Option<super::UuidSchema>")]
    #[serde(
        default,
        deserialize_with = "super::deserialize_optional_canonical_uuid"
    )]
    pub runtime_session_id: Option<String>,
    pub kind: JournalEventKindDto,
    pub severity: JournalSeverityDto,
    pub subject: Option<AppErrorSubjectDto>,
    pub error_code: Option<AppErrorCodeDto>,
    #[schemars(length(max = 500))]
    pub message: String,
}

impl From<colui_app::JournalEntry> for JournalEntryDto {
    fn from(v: colui_app::JournalEntry) -> Self {
        use colui_app::JournalEventKind as K;
        Self {
            sequence: v.sequence,
            timestamp: v.timestamp.0,
            runtime_session_id: v.runtime_session_id.map(|id| id.as_uuid().to_string()),
            kind: match v.kind {
                K::ConnectStarted => JournalEventKindDto::ConnectStarted,
                K::ConnectSucceeded => JournalEventKindDto::ConnectSucceeded,
                K::ConnectFailed => JournalEventKindDto::ConnectFailed,
                K::DisconnectStarted => JournalEventKindDto::DisconnectStarted,
                K::DisconnectSucceeded => JournalEventKindDto::DisconnectSucceeded,
                K::DisconnectFailed => JournalEventKindDto::DisconnectFailed,
                K::ReconnectStarted => JournalEventKindDto::ReconnectStarted,
                K::ReconnectSucceeded => JournalEventKindDto::ReconnectSucceeded,
                K::ReconnectFailed => JournalEventKindDto::ReconnectFailed,
                K::ManualRegistrationStarted => JournalEventKindDto::ManualRegistrationStarted,
                K::ManualRegistrationSucceeded => JournalEventKindDto::ManualRegistrationSucceeded,
                K::ManualRegistrationFailed => JournalEventKindDto::ManualRegistrationFailed,
                K::AutoRegistrationStarted => JournalEventKindDto::AutoRegistrationStarted,
                K::AutoRegistrationSucceeded => JournalEventKindDto::AutoRegistrationSucceeded,
                K::AutoRegistrationFailed => JournalEventKindDto::AutoRegistrationFailed,
                K::BackupStarted => JournalEventKindDto::BackupStarted,
                K::BackupSucceeded => JournalEventKindDto::BackupSucceeded,
                K::BackupFailed => JournalEventKindDto::BackupFailed,
                K::RestoreStarted => JournalEventKindDto::RestoreStarted,
                K::RestoreSucceeded => JournalEventKindDto::RestoreSucceeded,
                K::RestoreFailed => JournalEventKindDto::RestoreFailed,
                K::ProfileLifecycleStarted => JournalEventKindDto::ProfileLifecycleStarted,
                K::ProfileLifecycleSucceeded => JournalEventKindDto::ProfileLifecycleSucceeded,
                K::ProfileLifecycleFailed => JournalEventKindDto::ProfileLifecycleFailed,
                K::ContainerActionStarted => JournalEventKindDto::ContainerActionStarted,
                K::ContainerActionSucceeded => JournalEventKindDto::ContainerActionSucceeded,
                K::ContainerActionFailed => JournalEventKindDto::ContainerActionFailed,
                K::CandidateIgnored => JournalEventKindDto::CandidateIgnored,
            },
            severity: match v.severity {
                colui_app::JournalSeverity::Info => JournalSeverityDto::Info,
                colui_app::JournalSeverity::Error => JournalSeverityDto::Error,
            },
            subject: v.subject.map(Into::into),
            error_code: v.stable_error_code.map(Into::into),
            message: v.message,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionJournalDto {
    pub entries: Vec<JournalEntryDto>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiagnosticsSnapshotDto {
    pub runtime: RuntimeDiagnosticsDto,
    pub registry: RegistryDiagnosticsDto,
    pub import: ImportDiagnosticsDto,
    pub operations: OperationsDiagnosticsDto,
    pub inventory: RuntimeInventoryDto,
    pub definitions: DefinitionsDiagnosticsDto,
    pub journal: SessionJournalDto,
}

impl From<colui_app::DiagnosticsSnapshot> for DiagnosticsSnapshotDto {
    fn from(v: colui_app::DiagnosticsSnapshot) -> Self {
        Self {
            runtime: RuntimeDiagnosticsDto {
                state: v.runtime.state.into(),
                resolved_endpoint: v.runtime.resolved_endpoint.map(|v| v.as_str().to_owned()),
                api_fingerprint: v.runtime.api_fingerprint.map(Into::into),
                cli_fingerprint: v.runtime.cli_fingerprint.map(Into::into),
                session_id: v.runtime.session_id.map(|v| v.as_uuid().to_string()),
                connected_at: v.runtime.connected_at.map(|v| v.0),
            },
            registry: RegistryDiagnosticsDto {
                registry_path: v.registry.registry_path.to_string_lossy().into_owned(),
                backup_path: v.registry.backup_path.to_string_lossy().into_owned(),
                backup: v.registry.backup.into(),
                revision: v.registry.revision,
                health: v.registry.health.into(),
                lock_timeout_ms: v.registry.lock_timeout.as_millis().min(u64::MAX as u128) as u64,
                last_recovery_result: v.registry.last_recovery_result.map(|v| RecoveryResultDto {
                    success: v.is_ok(),
                    error: v.err().map(Into::into),
                }),
            },
            import: ImportDiagnosticsDto {
                source_path: v.import.source_path.to_string_lossy().into_owned(),
                imported_count: v.import.imported_count as u64,
                source_preserved: v.import.source_preserved,
                error: v.import.error.map(Into::into),
            },
            operations: OperationsDiagnosticsDto {
                generation: v.operations.generation,
                active: v
                    .operations
                    .active
                    .into_iter()
                    .map(|v| ActiveOperationDto {
                        kind: v.kind,
                        subject_id: v.subject_id,
                        started_at: v.started_at.0,
                        phase: match v.phase {
                            colui_app::OperationPhase::Pending => ActiveOperationPhaseDto::Pending,
                            colui_app::OperationPhase::Active => ActiveOperationPhaseDto::Active,
                        },
                    })
                    .collect(),
            },
            inventory: v.inventory.into(),
            definitions: DefinitionsDiagnosticsDto {
                generation: v.definitions.generation,
                profiles: v
                    .definitions
                    .profiles
                    .into_iter()
                    .map(|v| ProfileDefinitionDiagnosticsDto {
                        profile_id: v.profile_id.to_string(),
                        definition: v.definition.map(|definition| {
                            colui_app::DefinitionProjection {
                                definition,
                                error: v.error.clone(),
                            }
                            .into()
                        }),
                        error: v.error.map(Into::into),
                    })
                    .collect(),
            },
            journal: SessionJournalDto {
                entries: v.journal.entries.into_iter().map(Into::into).collect(),
            },
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ApplicationStateScopeDto {
    Profiles,
    Discovery,
    Diagnostics,
    Runtime,
    Definitions,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ApplicationStateChangedDto {
    pub sequence: u64,
    pub scopes: Vec<ApplicationStateScopeDto>,
}
