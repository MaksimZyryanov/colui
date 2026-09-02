use super::IssueDto;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimePresenceDto {
    Unavailable,
    Absent,
    Present,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum RuntimeActivityDto {
    AllRunning,
    Mixed,
    NoneRunning,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
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
    pub started_at: String,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum OperationKindDto {
    Apply,
    Stop,
    TearDown,
    Restart,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
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
    pub profile_id: String,
    pub success: bool,
}
impl LifecycleResultDto {
    pub fn success(profile_id: impl Into<String>) -> Self {
        Self {
            profile_id: profile_id.into(),
            success: true,
        }
    }
}
