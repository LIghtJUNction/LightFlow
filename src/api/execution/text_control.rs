use super::text_helpers::{control_outputs, merge_objects, split_value};
use crate::api::execution::media;
use crate::api::execution::types::LeafExecution;
use crate::api::plan::{
    CONTROL_IF_CAPABILITY, CONTROL_MERGE_CAPABILITY, CONTROL_SPLIT_CAPABILITY,
    CONTROL_SWITCH_CAPABILITY,
};
use crate::workflow::WorkflowSpec;

pub(super) fn execute_control_if(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> crate::api::ApiResult<LeafExecution> {
    let condition = media::input_bool(inputs, "condition").unwrap_or(false);
    let selected = if condition { "then" } else { "else" };
    let value = if condition {
        inputs
            .get("then_value")
            .or_else(|| inputs.get("then"))
            .cloned()
    } else {
        inputs
            .get("else_value")
            .or_else(|| inputs.get("else"))
            .cloned()
    }
    .unwrap_or(serde_json::Value::Null);

    Ok(LeafExecution {
        outputs: control_outputs(workflow, inputs, value, selected, CONTROL_IF_CAPABILITY),
        runtime: None,
        artifacts: Vec::new(),
    })
}

pub(super) fn execute_control_switch(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> crate::api::ApiResult<LeafExecution> {
    let selector = media::input_string(inputs, "selector").unwrap_or_default();
    let cases = inputs.get("cases").and_then(serde_json::Value::as_object);
    let value = cases
        .and_then(|cases| cases.get(&selector))
        .cloned()
        .or_else(|| inputs.get("default").cloned())
        .unwrap_or(serde_json::Value::Null);
    let selected = if cases.is_some_and(|cases| cases.contains_key(&selector)) {
        selector.as_str()
    } else {
        "default"
    };

    Ok(LeafExecution {
        outputs: control_outputs(workflow, inputs, value, selected, CONTROL_SWITCH_CAPABILITY),
        runtime: None,
        artifacts: Vec::new(),
    })
}

pub(super) fn execute_control_merge(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> crate::api::ApiResult<LeafExecution> {
    let mode = media::input_string(inputs, "mode").unwrap_or_else(|| "first_non_null".to_owned());
    let a = inputs.get("a").cloned().unwrap_or(serde_json::Value::Null);
    let b = inputs.get("b").cloned().unwrap_or(serde_json::Value::Null);
    let value = match mode.as_str() {
        "object" => merge_objects(a, b),
        "array" => serde_json::json!([a, b]),
        _ => {
            if !a.is_null() {
                a
            } else {
                b
            }
        }
    };

    Ok(LeafExecution {
        outputs: control_outputs(workflow, inputs, value, &mode, CONTROL_MERGE_CAPABILITY),
        runtime: None,
        artifacts: Vec::new(),
    })
}

pub(super) fn execute_control_split(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> crate::api::ApiResult<LeafExecution> {
    let value = inputs
        .get("value")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let (first, rest, items) = split_value(value);

    let mut outputs = serde_json::Map::new();
    for output in &workflow.outputs {
        let value = match output.name.as_str() {
            "first" => first.clone(),
            "rest" => rest.clone(),
            "items" => items.clone(),
            "selected" => serde_json::Value::String(CONTROL_SPLIT_CAPABILITY.to_owned()),
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
