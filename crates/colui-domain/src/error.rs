use crate::ProfileId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppErrorCode {
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

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AppErrorSubjectKind {
    Profile,
    Candidate,
    Container,
    Registry,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AppErrorSubject {
    pub kind: AppErrorSubjectKind,
    pub id: String,
}

impl AppErrorSubject {
    pub fn profile(id: ProfileId) -> Self {
        Self {
            kind: AppErrorSubjectKind::Profile,
            id: id.to_string(),
        }
    }

    pub fn candidate(id: impl ToString) -> Self {
        Self {
            kind: AppErrorSubjectKind::Candidate,
            id: id.to_string(),
        }
    }

    pub fn registry(id: impl Into<String>) -> Self {
        Self {
            kind: AppErrorSubjectKind::Registry,
            id: id.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct AppError {
    pub code: AppErrorCode,
    pub operation: String,
    pub subject: Option<Box<AppErrorSubject>>,
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
        let subject = subject_id
            .clone()
            .map(AppErrorSubject::profile)
            .map(Box::new);
        Self {
            code,
            operation: operation.into(),
            subject,
            subject_id,
            message: message.into(),
            details: None,
            retryable: matches!(
                code,
                AppErrorCode::RecoveryConflict
                    | AppErrorCode::RegistryLocked
                    | AppErrorCode::RegistryWriteFailed
            ),
        }
    }

    pub fn for_subject(
        code: AppErrorCode,
        operation: impl Into<String>,
        subject: AppErrorSubject,
        message: impl Into<String>,
    ) -> Self {
        Self {
            code,
            operation: operation.into(),
            subject: Some(Box::new(subject)),
            subject_id: None,
            message: message.into(),
            details: None,
            retryable: matches!(
                code,
                AppErrorCode::RecoveryConflict
                    | AppErrorCode::RegistryLocked
                    | AppErrorCode::RegistryWriteFailed
            ),
        }
    }

    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = Some(details.into());
        self
    }
}
