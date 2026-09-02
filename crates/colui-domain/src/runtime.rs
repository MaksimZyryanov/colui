use crate::{AppError, ContainerInstance, Timestamp};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::convert::TryFrom;
use uuid::Uuid;

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
pub struct DockerEndpoint(String);

impl DockerEndpoint {
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for DockerEndpoint {
    type Error = &'static str;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let value = value.trim();
        if value.is_empty() {
            Err("docker endpoint must not be empty")
        } else {
            Ok(Self(value.to_owned()))
        }
    }
}

impl TryFrom<String> for DockerEndpoint {
    type Error = &'static str;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::try_from(value.as_str()).map(|_| Self(value.trim().to_owned()))
    }
}

impl<'de> Deserialize<'de> for DockerEndpoint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        Self::try_from(value).map_err(serde::de::Error::custom)
    }
}

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize)]
pub struct DaemonFingerprint {
    pub daemon_id: String,
    pub server_version: String,
    pub os_type: String,
    pub architecture: String,
}

impl<'de> Deserialize<'de> for DaemonFingerprint {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct FingerprintFields {
            daemon_id: String,
            server_version: String,
            os_type: String,
            architecture: String,
        }

        let fields = FingerprintFields::deserialize(deserializer)?;
        Ok(Self::new(
            fields.daemon_id,
            fields.server_version,
            fields.os_type,
            fields.architecture,
        ))
    }
}

impl DaemonFingerprint {
    pub fn new(
        daemon_id: impl Into<String>,
        server_version: impl Into<String>,
        os_type: impl Into<String>,
        architecture: impl Into<String>,
    ) -> Self {
        Self {
            daemon_id: daemon_id.into().trim().to_owned(),
            server_version: server_version.into().trim().to_owned(),
            os_type: os_type.into().trim().to_owned(),
            architecture: architecture.into().trim().to_owned(),
        }
    }

    pub fn server_version(&self) -> &str {
        &self.server_version
    }
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct RuntimeSessionId(Uuid);

impl RuntimeSessionId {
    pub fn new(value: Uuid) -> Self {
        Self(value)
    }

    pub fn as_uuid(&self) -> &Uuid {
        &self.0
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RuntimeSessionState {
    Disconnected,
    Connecting,
    Ready(SessionContext),
    ContextMismatch(MismatchDetails),
    Failed(AppError),
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct SessionContext {
    pub session_id: RuntimeSessionId,
    pub endpoint: DockerEndpoint,
    pub daemon_fingerprint: DaemonFingerprint,
    pub connected_at: Timestamp,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct MismatchDetails {
    pub endpoint: DockerEndpoint,
    pub api_fingerprint: DaemonFingerprint,
    pub cli_fingerprint: DaemonFingerprint,
}

impl MismatchDetails {
    pub fn new(
        endpoint: DockerEndpoint,
        api_fingerprint: DaemonFingerprint,
        cli_fingerprint: DaemonFingerprint,
    ) -> Self {
        Self {
            endpoint,
            api_fingerprint,
            cli_fingerprint,
        }
    }

    pub fn api_fingerprint(&self) -> &DaemonFingerprint {
        &self.api_fingerprint
    }

    pub fn cli_fingerprint(&self) -> &DaemonFingerprint {
        &self.cli_fingerprint
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContainerDetails {
    pub instance: ContainerInstance,
    pub labels: BTreeMap<String, String>,
}
