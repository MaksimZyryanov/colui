mod error;
mod profile;
mod request;
mod runtime;
mod status;

use schemars::{
    r#gen::SchemaGenerator,
    schema::{Schema, SchemaObject, SubschemaValidation},
    JsonSchema,
};
use serde_json::json;

pub(crate) fn uuid_schema(generator: &mut SchemaGenerator) -> Schema {
    let mut schema: SchemaObject = <String>::json_schema(generator).into();
    schema.format = Some("uuid".to_owned());
    schema.into()
}

pub(crate) fn optional_uuid_schema(generator: &mut SchemaGenerator) -> Schema {
    SchemaObject {
        subschemas: Some(Box::new(SubschemaValidation {
            any_of: Some(vec![uuid_schema(generator), null_schema()]),
            ..Default::default()
        })),
        ..Default::default()
    }
    .into()
}

pub(crate) fn rfc3339_schema(generator: &mut SchemaGenerator) -> Schema {
    let mut schema: SchemaObject = <String>::json_schema(generator).into();
    schema.format = Some("date-time".to_owned());
    schema.into()
}

pub(crate) fn optional_rfc3339_schema(generator: &mut SchemaGenerator) -> Schema {
    SchemaObject {
        subschemas: Some(Box::new(SubschemaValidation {
            any_of: Some(vec![rfc3339_schema(generator), null_schema()]),
            ..Default::default()
        })),
        ..Default::default()
    }
    .into()
}

fn null_schema() -> Schema {
    SchemaObject {
        const_value: Some(json!(null)),
        ..Default::default()
    }
    .into()
}

pub use error::*;
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
