use crate::api::{ApiError, ApiResult};
use std::path::PathBuf;

pub(in crate::api::execution) fn input_mask_path(
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<PathBuf> {
    input_string(inputs, "mask_path")
        .or_else(|| input_string(inputs, "mask"))
        .map(PathBuf::from)
        .ok_or_else(|| ApiError::InvalidRequest("mask_path is required".to_owned()))
}

pub(in crate::api::execution) fn input_image_path(
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<PathBuf> {
    input_string(inputs, "image_path")
        .or_else(|| input_string(inputs, "path"))
        .map(PathBuf::from)
        .ok_or_else(|| ApiError::InvalidRequest("image_path is required".to_owned()))
}

pub(in crate::api::execution) fn input_string(
    inputs: &serde_json::Map<String, serde_json::Value>,
    name: &str,
) -> Option<String> {
    inputs.get(name).and_then(|value| match value {
        serde_json::Value::String(value) => Some(value.clone()),
        value if !value.is_null() => Some(value.to_string()),
        _ => None,
    })
}

pub(in crate::api::execution) fn input_u32(
    inputs: &serde_json::Map<String, serde_json::Value>,
    name: &str,
) -> Option<u32> {
    inputs
        .get(name)
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
}

pub(in crate::api::execution) fn input_u64(
    inputs: &serde_json::Map<String, serde_json::Value>,
    name: &str,
) -> Option<u64> {
    inputs.get(name).and_then(serde_json::Value::as_u64)
}

pub(in crate::api::execution) fn input_bool(
    inputs: &serde_json::Map<String, serde_json::Value>,
    name: &str,
) -> Option<bool> {
    inputs.get(name).and_then(|value| match value {
        serde_json::Value::Bool(value) => Some(*value),
        serde_json::Value::String(value) => match value.as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        },
        _ => None,
    })
}
