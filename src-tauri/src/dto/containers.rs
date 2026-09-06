use super::RuntimeInventoryDto;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContainerActionDto {
    Start,
    Stop,
    Restart,
}

impl From<ContainerActionDto> for colui_app::ContainerAction {
    fn from(v: ContainerActionDto) -> Self {
        match v {
            ContainerActionDto::Start => Self::Start,
            ContainerActionDto::Stop => Self::Stop,
            ContainerActionDto::Restart => Self::Restart,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum ContainerActionObservationDto {
    ConfirmedInSession,
    IndeterminateAfterSessionChange,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContainerActionRequestDto {
    pub container_id: String,
    pub action: ContainerActionDto,
    #[schemars(schema_with = "super::uuid_schema")]
    #[serde(deserialize_with = "super::deserialize_canonical_uuid")]
    pub runtime_session_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContainerActionResultDto {
    pub container_id: String,
    pub action: ContainerActionDto,
    pub observation: ContainerActionObservationDto,
    pub inventory: RuntimeInventoryDto,
}

impl From<colui_app::ContainerActionResult> for ContainerActionResultDto {
    fn from(v: colui_app::ContainerActionResult) -> Self {
        Self {
            container_id: v.container_id.0,
            action: match v.action {
                colui_app::ContainerAction::Start => ContainerActionDto::Start,
                colui_app::ContainerAction::Stop => ContainerActionDto::Stop,
                colui_app::ContainerAction::Restart => ContainerActionDto::Restart,
            },
            observation: match v.observation {
                colui_app::ContainerActionObservation::ConfirmedInSession => {
                    ContainerActionObservationDto::ConfirmedInSession
                }
                colui_app::ContainerActionObservation::IndeterminateAfterSessionChange => {
                    ContainerActionObservationDto::IndeterminateAfterSessionChange
                }
            },
            inventory: v.inventory.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ContainerLogsRequestDto {
    pub container_id: String,
    #[schemars(schema_with = "super::uuid_schema")]
    #[serde(deserialize_with = "super::deserialize_canonical_uuid")]
    pub runtime_session_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ContainerLogsDto {
    pub container_id: String,
    pub text: String,
    #[schemars(range(max = 262144))]
    pub retained_bytes: u32,
    pub truncated: bool,
    #[schemars(schema_with = "super::rfc3339_schema")]
    #[serde(
        deserialize_with = "super::deserialize_rfc3339",
        serialize_with = "super::serialize_rfc3339"
    )]
    pub observed_at: String,
}

impl From<colui_app::ContainerLogs> for ContainerLogsDto {
    fn from(v: colui_app::ContainerLogs) -> Self {
        Self {
            container_id: v.container_id.0,
            text: v.text,
            retained_bytes: v.retained_bytes,
            truncated: v.truncated,
            observed_at: v.observed_at.0,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct OpenContainerPortRequestDto {
    pub container_id: String,
    #[schemars(schema_with = "super::uuid_schema")]
    #[serde(deserialize_with = "super::deserialize_canonical_uuid")]
    pub runtime_session_id: String,
    pub binding_index: u32,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct PortBindingActionDto {
    pub copy: String,
    pub url: Option<String>,
}

impl From<colui_app::PortBindingAction> for PortBindingActionDto {
    fn from(v: colui_app::PortBindingAction) -> Self {
        Self {
            copy: v.copy,
            url: v.url,
        }
    }
}
