use colui_domain::{AppError, AppErrorCode, AppErrorSubject, AppErrorSubjectKind};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AppErrorCodeDto {
    RuntimeUnavailable,
    RuntimeConnectionFailed,
    RuntimeContextMismatch,
    CandidateStale,
    DiscoveryConflict,
    ProfileNotFound,
    ProfileAlreadyRegistered,
    ProfileRevisionConflict,
    ProfileInvalid,
    DefinitionFailed,
    ComposeFailed,
    ContainerOperationFailed,
    OperationConflict,
    OperationTimeout,
    RegistryCorrupt,
    RegistryLocked,
    RegistryWriteFailed,
    RecoveryConflict,
    PermissionDenied,
    ProtocolMismatch,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AppErrorDto {
    pub code: AppErrorCodeDto,
    pub operation: String,
    pub subject: Option<AppErrorSubjectDto>,
    pub message: String,
    pub details: Option<String>,
    pub retryable: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AppErrorSubjectKindDto {
    Profile,
    Candidate,
    Container,
    Registry,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(deny_unknown_fields)]
pub struct AppErrorSubjectDto {
    pub kind: AppErrorSubjectKindDto,
    pub id: String,
}

impl From<AppErrorSubject> for AppErrorSubjectDto {
    fn from(value: AppErrorSubject) -> Self {
        Self {
            kind: match value.kind {
                AppErrorSubjectKind::Profile => AppErrorSubjectKindDto::Profile,
                AppErrorSubjectKind::Candidate => AppErrorSubjectKindDto::Candidate,
                AppErrorSubjectKind::Container => AppErrorSubjectKindDto::Container,
                AppErrorSubjectKind::Registry => AppErrorSubjectKindDto::Registry,
            },
            id: value.id,
        }
    }
}

impl From<AppErrorCode> for AppErrorCodeDto {
    fn from(code: AppErrorCode) -> Self {
        match code {
            AppErrorCode::RuntimeUnavailable => Self::RuntimeUnavailable,
            AppErrorCode::RuntimeConnectionFailed => Self::RuntimeConnectionFailed,
            AppErrorCode::RuntimeContextMismatch => Self::RuntimeContextMismatch,
            AppErrorCode::CandidateStale => Self::CandidateStale,
            AppErrorCode::DiscoveryConflict => Self::DiscoveryConflict,
            AppErrorCode::ProfileNotFound => Self::ProfileNotFound,
            AppErrorCode::ProfileAlreadyRegistered => Self::ProfileAlreadyRegistered,
            AppErrorCode::ProfileRevisionConflict => Self::ProfileRevisionConflict,
            AppErrorCode::ProfileInvalid => Self::ProfileInvalid,
            AppErrorCode::DefinitionFailed => Self::DefinitionFailed,
            AppErrorCode::ComposeFailed => Self::ComposeFailed,
            AppErrorCode::ContainerOperationFailed => Self::ContainerOperationFailed,
            AppErrorCode::OperationConflict => Self::OperationConflict,
            AppErrorCode::OperationTimeout => Self::OperationTimeout,
            AppErrorCode::RegistryCorrupt => Self::RegistryCorrupt,
            AppErrorCode::RegistryLocked => Self::RegistryLocked,
            AppErrorCode::RegistryWriteFailed => Self::RegistryWriteFailed,
            AppErrorCode::RecoveryConflict => Self::RecoveryConflict,
            AppErrorCode::PermissionDenied => Self::PermissionDenied,
            AppErrorCode::ProtocolMismatch => Self::ProtocolMismatch,
        }
    }
}

impl From<AppError> for AppErrorDto {
    fn from(error: AppError) -> Self {
        let subject = error.subject.or_else(|| {
            matches!(
                error.code,
                AppErrorCode::RegistryCorrupt
                    | AppErrorCode::RegistryLocked
                    | AppErrorCode::RegistryWriteFailed
                    | AppErrorCode::RecoveryConflict
            )
            .then(|| Box::new(AppErrorSubject::registry("registry")))
        });
        Self {
            code: error.code.into(),
            operation: error.operation,
            subject: subject.map(|subject| (*subject).into()),
            message: error.message,
            details: error.details,
            retryable: error.retryable,
        }
    }
}
