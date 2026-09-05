use super::*;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryClassificationDto {
    NewUnambiguous,
    NameConflict,
    IncompleteMetadata,
    AlreadyRegistered,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum DiscoveryConflictSourceDto {
    RuntimeObservation,
    RegisteredProfile,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryConflictEvidenceDto {
    pub source: DiscoveryConflictSourceDto,
    #[schemars(with = "Option<super::UuidSchema>")]
    #[serde(deserialize_with = "super::deserialize_optional_canonical_uuid")]
    pub profile_id: Option<String>,
    pub working_directory: Option<String>,
    pub config_files: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryCandidateDto {
    #[schemars(regex(pattern = "^[0-9a-f]{64}$"))]
    #[serde(deserialize_with = "deserialize_candidate_id")]
    pub candidate_id: String,
    #[schemars(schema_with = "super::uuid_schema")]
    #[serde(deserialize_with = "super::deserialize_canonical_uuid")]
    pub runtime_session_id: String,
    pub inventory_generation: u64,
    pub compose_project_name: String,
    pub working_directory: Option<String>,
    pub config_files: Vec<String>,
    pub container_count: u32,
    pub classification: DiscoveryClassificationDto,
    pub conflicts: Vec<DiscoveryConflictEvidenceDto>,
    pub ignored: bool,
}

impl From<colui_domain::DiscoveryCandidate> for DiscoveryCandidateDto {
    fn from(v: colui_domain::DiscoveryCandidate) -> Self {
        use colui_domain::{DiscoveryClassification as C, DiscoveryConflictSource as S};
        Self {
            candidate_id: v.candidate_id.to_string(),
            runtime_session_id: v.runtime_session_id.as_uuid().to_string(),
            inventory_generation: v.inventory_generation,
            compose_project_name: v.compose_project_name,
            working_directory: v.working_directory,
            config_files: v.config_files,
            container_count: v.container_count,
            classification: match v.classification {
                C::NewUnambiguous => DiscoveryClassificationDto::NewUnambiguous,
                C::NameConflict => DiscoveryClassificationDto::NameConflict,
                C::IncompleteMetadata => DiscoveryClassificationDto::IncompleteMetadata,
                C::AlreadyRegistered => DiscoveryClassificationDto::AlreadyRegistered,
            },
            conflicts: v
                .conflicts
                .into_iter()
                .map(|c| DiscoveryConflictEvidenceDto {
                    source: match c.source {
                        S::RuntimeObservation => DiscoveryConflictSourceDto::RuntimeObservation,
                        S::RegisteredProfile => DiscoveryConflictSourceDto::RegisteredProfile,
                    },
                    profile_id: c.profile_id.map(|id| id.to_string()),
                    working_directory: c.working_directory,
                    config_files: c.config_files,
                })
                .collect(),
            ignored: v.ignored,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct DiscoveryListDto {
    pub candidates: Vec<DiscoveryCandidateDto>,
    pub auto_registration_enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct RegisterCandidateRequestDto {
    #[schemars(regex(pattern = "^[0-9a-f]{64}$"))]
    #[serde(deserialize_with = "deserialize_candidate_id")]
    pub candidate_id: String,
    #[schemars(schema_with = "super::uuid_schema")]
    #[serde(deserialize_with = "super::deserialize_canonical_uuid")]
    pub runtime_session_id: String,
    pub inventory_generation: u64,
    pub compose_project_name: String,
    pub working_directory: Option<String>,
    pub config_files: Vec<String>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct IgnoreCandidateRequestDto {
    #[schemars(regex(pattern = "^[0-9a-f]{64}$"))]
    #[serde(deserialize_with = "deserialize_candidate_id")]
    pub candidate_id: String,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ConfigureAutoRegistrationRequestDto {
    pub enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AutoRegistrationConfigurationDto {
    pub enabled: bool,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct AutoRegistrationResultDto {
    pub profiles: Vec<ProfileSummaryDto>,
    pub errors: Vec<AppErrorDto>,
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct AutoRegistrationRequestDto {
    #[schemars(schema_with = "super::uuid_schema")]
    #[serde(deserialize_with = "super::deserialize_canonical_uuid")]
    pub runtime_session_id: String,
    pub inventory_generation: u64,
}

fn deserialize_candidate_id<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<String, D::Error> {
    let value = String::deserialize(deserializer)?;
    if value.len() == 64
        && value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        Ok(value)
    } else {
        Err(serde::de::Error::custom(
            "candidateId must be lowercase SHA-256 hex",
        ))
    }
}

impl RegisterCandidateRequestDto {
    pub fn into_domain(
        self,
    ) -> Result<colui_app::RegisterCandidateRequest, colui_domain::AppError> {
        Ok(colui_app::RegisterCandidateRequest {
            candidate_id: decode_candidate_id(self.candidate_id)?,
            runtime_session_id: super::decode_session_id(self.runtime_session_id)?,
            inventory_generation: self.inventory_generation,
            compose_project_name: self.compose_project_name,
            working_directory: self.working_directory,
            config_files: self.config_files,
        })
    }
}

pub(crate) fn decode_candidate_id(
    value: String,
) -> Result<colui_domain::CandidateId, colui_domain::AppError> {
    if value.len() != 64
        || !value
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    {
        return Err(super::invalid_request());
    }
    serde_json::from_value(serde_json::Value::String(value)).map_err(|_| super::invalid_request())
}
