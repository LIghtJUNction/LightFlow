use super::text_helpers::{json_value_text, lookup_json_path, render_template, text_outputs};
use crate::api::execution::media;
use crate::api::execution::types::LeafExecution;
use crate::api::plan::TEXT_REGEX_CAPABILITY;
use crate::api::{ApiError, ApiResult};
use crate::workflow::WorkflowSpec;
use regex::Regex;

pub(super) fn execute_text_concat(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let separator = media::input_string(inputs, "separator").unwrap_or_default();
    let items = inputs
        .get("items")
        .and_then(serde_json::Value::as_array)
        .map(|items| items.iter().map(json_value_text).collect::<Vec<_>>())
        .unwrap_or_else(|| {
            ["a", "b"]
                .into_iter()
                .filter_map(|name| inputs.get(name))
                .map(json_value_text)
                .collect()
        });
    let text = items.join(&separator);
    Ok(LeafExecution {
        outputs: text_outputs(workflow, inputs, &text),
        runtime: None,
        artifacts: Vec::new(),
    })
}

pub(super) fn execute_text_template(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let template = media::input_string(inputs, "template").unwrap_or_default();
    let vars = inputs.get("vars").unwrap_or(&serde_json::Value::Null);
    let text = render_template(&template, vars);
    Ok(LeafExecution {
        outputs: text_outputs(workflow, inputs, &text),
        runtime: None,
        artifacts: Vec::new(),
    })
}

pub(super) fn execute_text_regex(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let text = media::input_string(inputs, "text").unwrap_or_default();
    let pattern = media::input_string(inputs, "pattern").unwrap_or_default();
    let replacement = media::input_string(inputs, "replacement");
    let regex = Regex::new(&pattern)
        .map_err(|error| ApiError::InvalidRequest(format!("invalid regex pattern: {error}")))?;

    let captures = regex
        .captures_iter(&text)
        .map(|captures| {
            serde_json::Value::Array(
                captures
                    .iter()
                    .map(|capture| {
                        capture
                            .map(|capture| serde_json::Value::String(capture.as_str().to_owned()))
                            .unwrap_or(serde_json::Value::Null)
                    })
                    .collect(),
            )
        })
        .collect::<Vec<_>>();
    let match_count = captures.len();
    let matched = match_count > 0;
    let result = replacement
        .as_deref()
        .map(|replacement| regex.replace_all(&text, replacement).into_owned())
        .unwrap_or_else(|| text.clone());
    let first_match = regex
        .find(&text)
        .map(|found| found.as_str().to_owned())
        .unwrap_or_default();

    let mut outputs = serde_json::Map::new();
    for output in &workflow.outputs {
        let value = match output.name.as_str() {
            "text" | "result" | "value" => serde_json::Value::String(result.clone()),
            "matched" => serde_json::Value::Bool(matched),
            "match_count" => serde_json::json!(match_count),
            "captures" => serde_json::Value::Array(captures.clone()),
            "first_match" => serde_json::Value::String(first_match.clone()),
            "capability" => serde_json::Value::String(TEXT_REGEX_CAPABILITY.to_owned()),
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

pub(super) fn execute_json_extract(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let source = inputs.get("value").unwrap_or(&serde_json::Value::Null);
    let path = media::input_string(inputs, "path").unwrap_or_default();
    let extracted = lookup_json_path(source, &path)
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let found = !extracted.is_null();
    let text = json_value_text(&extracted);

    let mut outputs = serde_json::Map::new();
    for output in &workflow.outputs {
        let value = match output.name.as_str() {
            "value" => extracted.clone(),
            "text" => serde_json::Value::String(text.clone()),
            "found" => serde_json::Value::Bool(found),
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
