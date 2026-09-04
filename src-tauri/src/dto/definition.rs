use super::{DefinitionStateDto, IssueDto, ProfileDetailsDto, RuntimeProjectionDto};
use colui_domain::{DefinitionState, ProjectDefinition, ServiceDefinition};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ServiceDefinitionDto {
    pub name: String,
    pub image: Option<String>,
    pub build_context: Option<String>,
    pub declared_ports: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDefinitionDto {
    #[schemars(schema_with = "super::uuid_schema")]
    #[serde(deserialize_with = "super::deserialize_canonical_uuid")]
    pub profile_id: String,
    pub definition_revision: Option<String>,
    #[serde(
        serialize_with = "super::serialize_optional_rfc3339",
        deserialize_with = "super::deserialize_optional_rfc3339"
    )]
    #[schemars(with = "Option<super::Rfc3339Schema>")]
    pub loaded_at: Option<String>,
    pub state: DefinitionStateDto,
    pub services: Vec<ServiceDefinitionDto>,
    pub issues: Vec<IssueDto>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDetailsResponseDto {
    pub profile: ProfileDetailsDto,
    pub definition: ProjectDefinitionDto,
    pub runtime: RuntimeProjectionDto,
}

impl From<ServiceDefinition> for ServiceDefinitionDto {
    fn from(value: ServiceDefinition) -> Self {
        Self {
            name: value.name,
            image: value.image,
            build_context: value.build_context,
            declared_ports: value.declared_ports,
        }
    }
}

impl From<ProjectDefinition> for ProjectDefinitionDto {
    fn from(value: ProjectDefinition) -> Self {
        let observed = value.state != DefinitionState::Unchecked;
        Self {
            profile_id: value.profile_id.to_string(),
            definition_revision: observed.then_some(value.definition_revision.0),
            loaded_at: observed.then_some(value.loaded_at.0),
            state: match value.state {
                DefinitionState::Unchecked => DefinitionStateDto::Unchecked,
                DefinitionState::Valid => DefinitionStateDto::Valid,
                DefinitionState::Invalid => DefinitionStateDto::Invalid,
                DefinitionState::Stale => DefinitionStateDto::Stale,
            },
            services: value.services.into_iter().map(Into::into).collect(),
            issues: value
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
