use super::{IssueDto, RuntimeInventoryDto};
use colui_app::{LifecycleResult, ProjectStatus};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimePresenceDto {
    Unavailable,
    Absent,
    Present,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum RuntimeActivityDto {
    AllRunning,
    Mixed,
    NoneRunning,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum DefinitionStateDto {
    Unchecked,
    Valid,
    Invalid,
    Stale,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeProjectionDto {
    pub presence: RuntimePresenceDto,
    pub activity: Option<RuntimeActivityDto>,
    pub container_count: u32,
    pub running_container_count: u32,
    #[serde(
        serialize_with = "super::serialize_optional_rfc3339",
        deserialize_with = "super::deserialize_optional_rfc3339"
    )]
    #[schemars(with = "Option<super::Rfc3339Schema>")]
    pub observed_at: Option<String>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DefinitionProjectionDto {
    pub state: DefinitionStateDto,
    pub revision: Option<String>,
    pub service_count: Option<u32>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectStatusDto {
    #[schemars(schema_with = "super::uuid_schema")]
    #[serde(deserialize_with = "super::deserialize_canonical_uuid")]
    pub profile_id: String,
    pub runtime: RuntimeProjectionDto,
    pub definition: DefinitionProjectionDto,
    pub operation: Option<OperationDto>,
    pub issues: Vec<IssueDto>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct OperationDto {
    pub kind: OperationKindDto,
    pub phase: OperationPhaseDto,
    #[serde(
        serialize_with = "super::serialize_rfc3339",
        deserialize_with = "super::deserialize_rfc3339"
    )]
    #[schemars(schema_with = "super::rfc3339_schema")]
    pub started_at: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum OperationKindDto {
    Apply,
    Stop,
    TearDown,
    Restart,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "kebab-case")]
pub enum OperationPhaseDto {
    Queued,
    Running,
    Succeeded,
    Failed,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProfileValidationDto {
    pub valid: bool,
    pub issues: Vec<IssueDto>,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct LifecycleResultDto {
    #[schemars(schema_with = "super::uuid_schema")]
    #[serde(deserialize_with = "super::deserialize_canonical_uuid")]
    pub profile_id: String,
    pub success: bool,
    pub inventory_generation: u64,
    pub inventory: RuntimeInventoryDto,
}
impl LifecycleResultDto {
    pub fn success(profile_id: impl Into<String>) -> Self {
        let inventory = RuntimeInventoryDto::from(colui_domain::RuntimeInventory::unavailable());
        Self {
            profile_id: profile_id.into(),
            success: true,
            inventory_generation: inventory.generation,
            inventory,
        }
    }

    pub fn fixture() -> Self {
        let inventory = RuntimeInventoryDto::fixture();
        Self {
            profile_id: "00000000-0000-0000-0000-000000000001".into(),
            success: true,
            inventory_generation: inventory.generation,
            inventory,
        }
    }
}

impl From<LifecycleResult> for LifecycleResultDto {
    fn from(result: LifecycleResult) -> Self {
        let inventory_generation = result.inventory_generation();
        let inventory: RuntimeInventoryDto = result.inventory.into();
        debug_assert_eq!(inventory_generation, inventory.generation);
        Self {
            profile_id: result.profile_id.to_string(),
            success: result.success,
            inventory_generation,
            inventory,
        }
    }
}

impl From<ProjectStatus> for ProjectStatusDto {
    fn from(status: ProjectStatus) -> Self {
        Self {
            profile_id: status.profile_id.to_string(),
            runtime: RuntimeProjectionDto {
                presence: match status.runtime.presence {
                    colui_domain::RuntimePresence::Unavailable => RuntimePresenceDto::Unavailable,
                    colui_domain::RuntimePresence::Absent => RuntimePresenceDto::Absent,
                    colui_domain::RuntimePresence::Present => RuntimePresenceDto::Present,
                },
                activity: status.runtime.activity.map(|activity| match activity {
                    colui_domain::RuntimeActivity::AllRunning => RuntimeActivityDto::AllRunning,
                    colui_domain::RuntimeActivity::Mixed => RuntimeActivityDto::Mixed,
                    colui_domain::RuntimeActivity::NoneRunning => RuntimeActivityDto::NoneRunning,
                }),
                container_count: status.runtime.container_count,
                running_container_count: status.runtime.running_container_count,
                observed_at: status.runtime.observed_at.map(|timestamp| timestamp.0),
            },
            definition: DefinitionProjectionDto {
                state: match status.definition_state {
                    colui_domain::DefinitionState::Unchecked => DefinitionStateDto::Unchecked,
                    colui_domain::DefinitionState::Valid => DefinitionStateDto::Valid,
                    colui_domain::DefinitionState::Invalid => DefinitionStateDto::Invalid,
                    colui_domain::DefinitionState::Stale => DefinitionStateDto::Stale,
                },
                revision: None,
                service_count: None,
            },
            operation: None,
            issues: status
                .issues
                .into_iter()
                .map(|issue| IssueDto {
                    field: issue.field,
                    message: issue.message,
                })
                .collect(),
        }
    }
}
