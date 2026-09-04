use super::{AppErrorDto, DaemonFingerprintDto};
use colui_domain::{
    ContainerInstance, ContainerState, InventoryFreshness, PortBinding, ProjectRuntimeSnapshot,
    RuntimeInventory,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum ContainerStateDto {
    Running,
    Stopped,
    Unknown,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PortBindingDto {
    pub host_ip: String,
    pub host_port: u16,
    pub container_port: u16,
    pub protocol: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContainerInstanceDto {
    pub id: String,
    pub name: String,
    pub image: String,
    pub state: ContainerStateDto,
    pub status_text: String,
    pub service_name: Option<String>,
    pub published_ports: Vec<PortBindingDto>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectRuntimeSnapshotDto {
    pub compose_project_name: String,
    pub working_directory: Option<String>,
    pub config_files: Vec<String>,
    pub containers: Vec<ContainerInstanceDto>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum InventoryFreshnessDto {
    Fresh,
    Stale,
    Unavailable,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeInventoryDto {
    pub generation: u64,
    pub has_snapshot: bool,
    #[serde(
        serialize_with = "super::serialize_optional_rfc3339",
        deserialize_with = "super::deserialize_optional_rfc3339"
    )]
    #[schemars(with = "Option<super::Rfc3339Schema>")]
    pub observed_at: Option<String>,
    #[schemars(with = "Option<super::UuidSchema>")]
    #[serde(deserialize_with = "super::deserialize_optional_canonical_uuid")]
    pub runtime_session_id: Option<String>,
    pub daemon_fingerprint: Option<DaemonFingerprintDto>,
    pub freshness: InventoryFreshnessDto,
    #[serde(
        serialize_with = "super::serialize_optional_rfc3339",
        deserialize_with = "super::deserialize_optional_rfc3339"
    )]
    #[schemars(with = "Option<super::Rfc3339Schema>")]
    pub last_successful_observed_at: Option<String>,
    pub containers: Vec<ContainerInstanceDto>,
    pub projects: Vec<ProjectRuntimeSnapshotDto>,
    pub standalone_containers: Vec<ContainerInstanceDto>,
    pub error: Option<AppErrorDto>,
}

impl RuntimeInventoryDto {
    pub fn fixture() -> Self {
        let mut inventory = RuntimeInventory::unavailable();
        inventory.generation = 1;
        inventory.has_snapshot = true;
        inventory.freshness = InventoryFreshness::Fresh;
        inventory.observed_at = Some(colui_domain::Timestamp("2026-09-03T00:00:00Z".into()));
        inventory.into()
    }
}

impl From<PortBinding> for PortBindingDto {
    fn from(value: PortBinding) -> Self {
        Self {
            host_ip: value.host_ip,
            host_port: value.host_port,
            container_port: value.container_port,
            protocol: value.protocol,
        }
    }
}

impl From<ContainerInstance> for ContainerInstanceDto {
    fn from(value: ContainerInstance) -> Self {
        Self {
            id: value.id.0,
            name: value.name,
            image: value.image,
            state: match value.state {
                ContainerState::Running => ContainerStateDto::Running,
                ContainerState::Stopped => ContainerStateDto::Stopped,
                ContainerState::Unknown => ContainerStateDto::Unknown,
            },
            status_text: value.status_text,
            service_name: value.service_name,
            published_ports: value.published_ports.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<ProjectRuntimeSnapshot> for ProjectRuntimeSnapshotDto {
    fn from(value: ProjectRuntimeSnapshot) -> Self {
        Self {
            compose_project_name: value.compose_project_name,
            working_directory: value.working_directory,
            config_files: value.config_files,
            containers: value.containers.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<RuntimeInventory> for RuntimeInventoryDto {
    fn from(value: RuntimeInventory) -> Self {
        Self {
            generation: value.generation,
            has_snapshot: value.has_snapshot,
            observed_at: value.observed_at.map(|value| value.0),
            runtime_session_id: value
                .runtime_session_id
                .map(|value| value.as_uuid().to_string()),
            daemon_fingerprint: value.daemon_fingerprint.map(Into::into),
            freshness: match value.freshness {
                InventoryFreshness::Fresh => InventoryFreshnessDto::Fresh,
                InventoryFreshness::Stale => InventoryFreshnessDto::Stale,
                InventoryFreshness::Unavailable => InventoryFreshnessDto::Unavailable,
            },
            last_successful_observed_at: value.last_successful_observed_at.map(|value| value.0),
            containers: value.containers.into_iter().map(Into::into).collect(),
            projects: value
                .project_snapshots
                .into_iter()
                .map(Into::into)
                .collect(),
            standalone_containers: value
                .standalone_containers
                .into_iter()
                .map(Into::into)
                .collect(),
            error: value.error.map(Into::into),
        }
    }
}
