use super::ProfilePatchDto;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProfileRequestDto {
    #[schemars(schema_with = "super::uuid_schema")]
    pub profile_id: String,
    pub expected_revision: u64,
    pub patch: ProfilePatchDto,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct RemoveProfileRequestDto {
    #[schemars(schema_with = "super::uuid_schema")]
    pub profile_id: String,
    pub expected_revision: u64,
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize, Deserialize, JsonSchema)]
#[serde(rename_all = "camelCase")]
pub struct ProfileIdRequestDto {
    #[schemars(schema_with = "super::uuid_schema")]
    pub profile_id: String,
}
impl UpdateProfileRequestDto {
    pub fn fixture() -> Self {
        Self {
            profile_id: "00000000-0000-0000-0000-000000000001".into(),
            expected_revision: 3,
            patch: ProfilePatchDto {
                display_name: "Demo".into(),
                compose_project_name: "demo".into(),
                working_directory: "/tmp".into(),
                compose_files: vec!["compose.yml".into()],
                environment_files: vec![],
            },
        }
    }
}
