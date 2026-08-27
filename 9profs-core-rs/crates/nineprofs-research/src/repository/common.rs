use nineprofs_common::now_ms;
use serde::{Serialize, de::DeserializeOwned};

use crate::ResearchError;

pub(super) fn enum_text<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value)
        .expect("research enum serialization cannot fail")
        .trim_matches('"')
        .to_owned()
}

pub(super) fn json_text<T: Serialize>(value: &T) -> Result<String, ResearchError> {
    Ok(serde_json::to_string(value)?)
}

pub(super) fn json_column<T: DeserializeOwned>(
    value: String,
    field: &str,
) -> Result<T, ResearchError> {
    serde_json::from_str(&value)
        .map_err(|error| ResearchError::Invalid(format!("invalid persisted {field}: {error}")))
}

#[allow(dead_code)]
fn _now_for_repository_tests() -> i64 {
    now_ms()
}
