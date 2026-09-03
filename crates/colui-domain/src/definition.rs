//! Runtime-free boundary types for future definition and inventory projections.

use crate::{AppError, DaemonFingerprint, ProfileId, RuntimeSessionId};
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
    pub field: Option<String>,
    pub message: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum InventoryFreshness {
    Fresh,
    Stale,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimeInventory {
    pub generation: u64,
    pub has_snapshot: bool,
    pub observed_at: Option<Timestamp>,
    pub runtime_session_id: Option<RuntimeSessionId>,
    pub daemon_fingerprint: Option<DaemonFingerprint>,
    pub freshness: InventoryFreshness,
    pub last_successful_observed_at: Option<Timestamp>,
    pub containers: Vec<ContainerInstance>,
    pub project_snapshots: Vec<ProjectRuntimeSnapshot>,
    pub standalone_containers: Vec<ContainerInstance>,
    pub error: Option<AppError>,
}

impl RuntimeInventory {
    pub fn unavailable() -> Self {
        Self {
            generation: 0,
            has_snapshot: false,
            observed_at: None,
            runtime_session_id: None,
            daemon_fingerprint: None,
            freshness: InventoryFreshness::Unavailable,
            last_successful_observed_at: None,
            containers: Vec::new(),
            project_snapshots: Vec::new(),
            standalone_containers: Vec::new(),
            error: None,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ProjectRuntimeSnapshot {
    pub compose_project_name: String,
    pub working_directory: Option<String>,
    pub config_files: Vec<String>,
    pub containers: Vec<ContainerInstance>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ContainerObservation {
    pub instance: ContainerInstance,
    pub compose: Option<ComposeContainerMetadata>,
}

impl ContainerObservation {
    pub fn new(instance: ContainerInstance, compose: Option<ComposeContainerMetadata>) -> Self {
        Self { instance, compose }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub struct ComposeContainerMetadata {
    pub project: String,
    pub service: Option<String>,
    pub working_directory: Option<String>,
    pub config_files: Vec<String>,
}

impl ComposeContainerMetadata {
    pub fn project(name: impl Into<String>) -> Self {
        Self {
            project: name.into(),
            service: None,
            working_directory: None,
            config_files: Vec::new(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn instance() -> ContainerInstance {
        ContainerInstance {
            id: ContainerId("container-1".to_owned()),
            name: "checkout-web".to_owned(),
            image: "checkout:latest".to_owned(),
            state: ContainerState::Running,
            status_text: "Up".to_owned(),
            service_name: None,
            published_ports: Vec::new(),
        }
    }

    #[test]
    fn pre_observation_is_distinct_from_published_generation_zero() {
        let value = RuntimeInventory::unavailable();
        assert!(!value.has_snapshot);
        assert_eq!(value.generation, 0);
        assert!(value.observed_at.is_none());
    }

    #[test]
    fn observation_keeps_compose_metadata_separate_from_instance() {
        let observation = ContainerObservation::new(
            instance(),
            Some(ComposeContainerMetadata::project("checkout")),
        );
        assert_eq!(observation.compose.unwrap().project, "checkout");
        assert!(observation.instance.service_name.is_none());
    }
}
