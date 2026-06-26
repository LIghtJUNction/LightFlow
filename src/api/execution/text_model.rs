use std::fs;
use std::path::Path;

use crate::api::execution::media;
use crate::api::execution::types::LeafExecution;
use crate::api::plan::{MODEL_LOCK_CHECK_CAPABILITY, MODEL_SELECT_CAPABILITY};
use crate::api::{ApiError, ApiResult};
use crate::workflow::WorkflowSpec;

pub(super) fn execute_model_select(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let requirement_id = media::input_string(inputs, "requirement_id").unwrap_or_default();
    let preferred =
        media::input_string(inputs, "preferred").or_else(|| media::input_string(inputs, "variant"));
    let variants = inputs
        .get("variants")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let selected = preferred
        .as_deref()
        .and_then(|preferred| {
            variants.iter().find(|variant| {
                variant.get("id").and_then(serde_json::Value::as_str) == Some(preferred)
                    || variant.get("format").and_then(serde_json::Value::as_str) == Some(preferred)
            })
        })
        .cloned()
        .or_else(|| variants.first().cloned())
        .unwrap_or(serde_json::Value::Null);
    let variant_id = selected
        .get("id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();

    let mut outputs = serde_json::Map::new();
    for output in &workflow.outputs {
        let value = match output.name.as_str() {
            "model" | "variant" => selected.clone(),
            "variant_id" => serde_json::Value::String(variant_id.clone()),
            "requirement_id" => serde_json::Value::String(requirement_id.clone()),
            "capability" => serde_json::Value::String(MODEL_SELECT_CAPABILITY.to_owned()),
            other => inputs
                .get(other)
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        };
        outputs.insert(output.name.clone(), value);
    }
    Ok(LeafExecution {
        outputs,
        runtime: None,
        artifacts: Vec::new(),
    })
}

pub(super) fn execute_model_lock_check(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let workflow_id = media::input_string(inputs, "workflow_id").unwrap_or_default();
    let requirement_id = media::input_string(inputs, "requirement_id").unwrap_or_default();
    let key = format!("{workflow_id}::{requirement_id}");
    let lock_path = root.join("lfw.lock");
    let lock = if lock_path.exists() {
        serde_json::from_slice::<serde_json::Value>(&fs::read(&lock_path)?)
            .map_err(|error| ApiError::InvalidRequest(format!("invalid lfw.lock: {error}")))?
    } else {
        serde_json::Value::Null
    };
    let entry = lock
        .get("models")
        .and_then(|models| models.get(&key))
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let path = entry
        .get("local_paths")
        .and_then(serde_json::Value::as_array)
        .and_then(|paths| paths.first())
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let locked = !entry.is_null();
    let exists = path
        .as_str()
        .map(|path| Path::new(path).exists())
        .unwrap_or(false);

    let mut outputs = serde_json::Map::new();
    for output in &workflow.outputs {
        let value = match output.name.as_str() {
            "locked" => serde_json::Value::Bool(locked),
            "exists" => serde_json::Value::Bool(exists),
            "key" => serde_json::Value::String(key.clone()),
            "path" => path.clone(),
            "entry" => entry.clone(),
            "capability" => serde_json::Value::String(MODEL_LOCK_CHECK_CAPABILITY.to_owned()),
            other => inputs
                .get(other)
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        };
        outputs.insert(output.name.clone(), value);
    }

    Ok(LeafExecution {
        outputs,
        runtime: None,
        artifacts: Vec::new(),
    })
}
