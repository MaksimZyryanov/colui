use super::AppErrorDto;
use colui_domain::{DaemonFingerprint, MismatchDetails, RuntimeSessionState, SessionContext};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DaemonFingerprintDto {
    pub daemon_id: String,
    pub server_version: String,
    pub os_type: String,
    pub architecture: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct SessionContextDto {
    pub session_id: String,
    pub endpoint: String,
    pub daemon_fingerprint: DaemonFingerprintDto,
    pub connected_at: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct MismatchDetailsDto {
    pub endpoint: String,
    pub api_fingerprint: DaemonFingerprintDto,
    pub cli_fingerprint: DaemonFingerprintDto,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum RuntimeStateDto {
    Disconnected,
    Connecting,
    Ready { context: SessionContextDto },
    ContextMismatch { details: MismatchDetailsDto },
    Failed { error: AppErrorDto },
}
impl From<DaemonFingerprint> for DaemonFingerprintDto {
    fn from(v: DaemonFingerprint) -> Self {
        Self {
            daemon_id: v.daemon_id,
            server_version: v.server_version,
            os_type: v.os_type,
            architecture: v.architecture,
        }
    }
}
impl From<SessionContext> for SessionContextDto {
    fn from(v: SessionContext) -> Self {
        Self {
            session_id: v.session_id.as_uuid().to_string(),
            endpoint: v.endpoint.as_str().into(),
            daemon_fingerprint: v.daemon_fingerprint.into(),
            connected_at: v.connected_at.0,
        }
    }
}
impl From<MismatchDetails> for MismatchDetailsDto {
    fn from(v: MismatchDetails) -> Self {
        Self {
            endpoint: v.endpoint.as_str().into(),
            api_fingerprint: v.api_fingerprint.into(),
            cli_fingerprint: v.cli_fingerprint.into(),
        }
    }
}
impl From<RuntimeSessionState> for RuntimeStateDto {
    fn from(v: RuntimeSessionState) -> Self {
        match v {
            RuntimeSessionState::Disconnected => Self::Disconnected,
            RuntimeSessionState::Connecting => Self::Connecting,
            RuntimeSessionState::Ready(v) => Self::Ready { context: v.into() },
            RuntimeSessionState::ContextMismatch(v) => Self::ContextMismatch { details: v.into() },
            RuntimeSessionState::Failed(v) => Self::Failed { error: v.into() },
        }
    }
}
