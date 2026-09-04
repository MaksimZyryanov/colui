mod definition;
mod error;
mod inventory;
mod profile;
mod request;
mod runtime;
mod status;

use schemars::{
    r#gen::SchemaGenerator,
    schema::{Schema, SchemaObject},
    JsonSchema,
};
use serde::Deserialize;

pub(crate) fn uuid_schema(generator: &mut SchemaGenerator) -> Schema {
    let mut schema: SchemaObject = <String>::json_schema(generator).into();
    schema.format = Some("uuid".to_owned());
    schema.into()
}

pub(crate) fn deserialize_canonical_uuid<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = String::deserialize(deserializer)?;
    colui_domain::ProfileId::parse(&value)
        .map(|_| value)
        .map_err(serde::de::Error::custom)
}

pub(crate) fn deserialize_optional_canonical_uuid<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<String>::deserialize(deserializer)?;
    value
        .map(|value| {
            colui_domain::ProfileId::parse(&value)
                .map(|_| value)
                .map_err(serde::de::Error::custom)
        })
        .transpose()
}

pub(crate) struct UuidSchema;

impl JsonSchema for UuidSchema {
    fn schema_name() -> String {
        "UuidSchema".to_owned()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        uuid_schema(generator)
    }
}

pub(crate) fn rfc3339_schema(generator: &mut SchemaGenerator) -> Schema {
    let mut schema: SchemaObject = <String>::json_schema(generator).into();
    schema.format = Some("date-time".to_owned());
    schema.into()
}

pub(crate) struct Rfc3339Schema;

impl JsonSchema for Rfc3339Schema {
    fn schema_name() -> String {
        "Rfc3339Schema".to_owned()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        rfc3339_schema(generator)
    }
}

pub use definition::*;
pub use error::*;
pub use inventory::*;
pub use profile::*;
pub use request::*;
pub use runtime::*;
pub use status::*;

pub(crate) fn deserialize_rfc3339<'de, D>(deserializer: D) -> Result<String, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = <String as serde::Deserialize>::deserialize(deserializer)?;
    chrono::DateTime::parse_from_rfc3339(&value).map_err(serde::de::Error::custom)?;
    Ok(value)
}

pub(crate) fn serialize_rfc3339<S>(value: &str, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    chrono::DateTime::parse_from_rfc3339(value).map_err(serde::ser::Error::custom)?;
    serializer.serialize_str(value)
}

pub(crate) fn deserialize_optional_rfc3339<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = <Option<String> as serde::Deserialize>::deserialize(deserializer)?;
    if let Some(value) = &value {
        chrono::DateTime::parse_from_rfc3339(value).map_err(serde::de::Error::custom)?;
    }
    Ok(value)
}

pub(crate) fn serialize_optional_rfc3339<S>(
    value: &Option<String>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    if let Some(value) = value {
        chrono::DateTime::parse_from_rfc3339(value).map_err(serde::ser::Error::custom)?;
    }
    serde::Serialize::serialize(value, serializer)
}
