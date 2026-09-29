//! Lenient deserializers for fields instances send inconsistently.

use serde::Deserialize;
use serde_json::Value;

pub(super) fn deserialize_u64_or_zero<'de, D>(deserializer: D) -> std::result::Result<u64, D::Error>
where
	D: serde::Deserializer<'de>,
{
	let val = Value::deserialize(deserializer)?;
	match val {
		Value::Number(n) => n.as_i64().map_or_else(
			|| n.as_u64().map_or(Ok(0), Ok),
			|i| {
				if i < 0 { Ok(0) } else { Ok(i.cast_unsigned()) }
			},
		),
		Value::String(s) => Ok(s.parse().unwrap_or(0)),
		_ => Ok(0),
	}
}

pub(super) fn deserialize_option_u64_or_zero<'de, D>(deserializer: D) -> std::result::Result<Option<u64>, D::Error>
where
	D: serde::Deserializer<'de>,
{
	let val = Value::deserialize(deserializer)?;
	match val {
		Value::Number(n) => n.as_i64().map_or_else(
			|| n.as_u64().map_or(Ok(None), |u| Ok(Some(u))),
			|i| {
				if i < 0 { Ok(Some(0)) } else { Ok(Some(i.cast_unsigned())) }
			},
		),
		_ => Ok(None),
	}
}

/// Accepts a string, and turns anything else (including null and a missing
/// field) into `None` rather than failing the whole parent object.
pub(super) fn deserialize_string_or_none<'de, D>(deserializer: D) -> std::result::Result<Option<String>, D::Error>
where
	D: serde::Deserializer<'de>,
{
	let val = Value::deserialize(deserializer)?;
	match val {
		Value::String(s) => Ok(Some(s)),
		_ => Ok(None),
	}
}
