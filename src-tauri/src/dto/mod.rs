mod error;
mod profile;
mod request;
mod runtime;
mod status;

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
