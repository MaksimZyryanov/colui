use crate::ProfileId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppErrorCode {
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

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AppError {
    pub code: AppErrorCode,
    pub operation: String,
    pub subject_id: Option<ProfileId>,
    pub message: String,
    pub details: Option<String>,
    pub retryable: bool,
}

impl AppError {
    pub fn new(
        code: AppErrorCode,
        operation: impl Into<String>,
        subject_id: Option<ProfileId>,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            operation: operation.into(),
            subject_id,
            message: message.into(),
            details: None,
            retryable: matches!(code, AppErrorCode::RegistryLocked),
        }
    }

    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }
}
