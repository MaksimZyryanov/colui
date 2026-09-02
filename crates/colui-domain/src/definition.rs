//! Runtime-free boundary types for future definition and inventory projections.

use crate::ProfileId;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct DefinitionRevision(pub String);

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct Timestamp(pub String);

#[derive(Clone, Debug, Eq, Hash, PartialEq, Serialize, Deserialize)]
pub struct ContainerId(pub String);

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RuntimePresence {
    Unavailable,
    Absent,
    Present,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RuntimeActivity {
    AllRunning,
    Mixed,
    NoneRunning,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct PortBinding {
    pub host_ip: String,
    pub host_port: u16,
    pub container_port: u16,
    pub protocol: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum ContainerState {
    Running,
    Stopped,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContainerInstance {
    pub id: ContainerId,
    pub name: String,
    pub image: String,
    pub state: ContainerState,
    pub status_text: String,
    pub service_name: Option<String>,
    pub published_ports: Vec<PortBinding>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum DefinitionState {
    Unchecked,
    Valid,
    Invalid,
    Stale,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ServiceDefinition {
    pub name: String,
    pub image: Option<String>,
    pub build_context: Option<String>,
    pub declared_ports: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProjectDefinition {
    pub profile_id: ProfileId,
    pub definition_revision: DefinitionRevision,
    pub loaded_at: Timestamp,
    pub state: DefinitionState,
    pub services: Vec<ServiceDefinition>,
    pub issues: Vec<Issue>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct Issue {
    pub message: String,
}
