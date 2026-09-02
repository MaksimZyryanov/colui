use colui_domain::{AppError, AppErrorCode};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AppErrorCodeDto {
    RuntimeUnavailable,
    RuntimeConnectionFailed,
    RuntimeContextMismatch,
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
    PermissionDenied,
    ProtocolMismatch,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AppErrorDto {
    pub code: AppErrorCodeDto,
    pub operation: String,
    #[schemars(schema_with = "super::optional_uuid_schema")]
    pub subject_id: Option<String>,
    pub message: String,
    pub details: Option<String>,
    pub retryable: bool,
}

impl From<AppErrorCode> for AppErrorCodeDto {
    fn from(code: AppErrorCode) -> Self {
        match code {
            AppErrorCode::RuntimeUnavailable => Self::RuntimeUnavailable,
            AppErrorCode::RuntimeConnectionFailed => Self::RuntimeConnectionFailed,
            AppErrorCode::RuntimeContextMismatch => Self::RuntimeContextMismatch,
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
            AppErrorCode::PermissionDenied => Self::PermissionDenied,
            AppErrorCode::ProtocolMismatch => Self::ProtocolMismatch,
        }
    }
}

impl From<AppError> for AppErrorDto {
    fn from(error: AppError) -> Self {
        Self {
            code: error.code.into(),
            operation: error.operation,
            subject_id: error.subject_id.map(|id| id.to_string()),
            message: error.message,
            details: error.details,
            retryable: error.retryable,
        }
    }
}
