use super::text_helpers::json_value_text;
use super::text_helpers::text_outputs;
use crate::api::execution::media;
use crate::api::execution::types::LeafExecution;
use crate::api::plan::{
    LLM_CLASSIFY_CAPABILITY, LLM_GENERATE_CAPABILITY, LLM_STRUCTURED_OUTPUT_CAPABILITY,
};
use crate::workflow::WorkflowSpec;

pub(super) fn execute_builtin_llm_generate(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> crate::api::ApiResult<LeafExecution> {
    let prompt = media::input_string(inputs, "prompt")
        .or_else(|| media::input_string(inputs, "text"))
        .unwrap_or_default();
    let model = media::input_string(inputs, "model").unwrap_or_else(|| "mock".to_owned());
    let text = format!("mock:{model}:{prompt}");
    let mut outputs = text_outputs(workflow, inputs, &text);
    outputs.insert("model".to_owned(), model.into());
    outputs.insert("provider".to_owned(), "mock".into());
    outputs.insert(
        "capability".to_owned(),
        serde_json::Value::String(LLM_GENERATE_CAPABILITY.to_owned()),
    );
    Ok(LeafExecution {
        outputs,
        runtime: None,
        artifacts: Vec::new(),
    })
}

pub(super) fn execute_llm_classify(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> crate::api::ApiResult<LeafExecution> {
    let text = media::input_string(inputs, "text")
        .or_else(|| media::input_string(inputs, "prompt"))
        .unwrap_or_default();
    let labels = inputs
        .get("labels")
        .and_then(serde_json::Value::as_array)
        .map(|labels| labels.iter().map(json_value_text).collect::<Vec<_>>())
        .unwrap_or_default();
    let lower = text.to_ascii_lowercase();
    let label = labels
        .iter()
        .find(|label| lower.contains(&label.to_ascii_lowercase()))
        .cloned()
        .or_else(|| labels.first().cloned())
        .unwrap_or_default();
    let confidence = if label.is_empty() { 0.0 } else { 1.0 };

    let mut outputs = serde_json::Map::new();
    for output in &workflow.outputs {
        let value = match output.name.as_str() {
            "label" => serde_json::Value::String(label.clone()),
            "confidence" => serde_json::json!(confidence),
            "capability" => serde_json::Value::String(LLM_CLASSIFY_CAPABILITY.to_owned()),
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

pub(super) fn execute_llm_structured_output(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> crate::api::ApiResult<LeafExecution> {
    let text = media::input_string(inputs, "text")
        .or_else(|| media::input_string(inputs, "prompt"))
        .unwrap_or_default();
    let object = serde_json::from_str::<serde_json::Value>(&text)
        .unwrap_or_else(|_| serde_json::json!({ "text": text }));

    let mut outputs = serde_json::Map::new();
    for output in &workflow.outputs {
        let value = match output.name.as_str() {
            "object" | "value" => object.clone(),
            "json" => serde_json::Value::String(object.to_string()),
            "capability" => serde_json::Value::String(LLM_STRUCTURED_OUTPUT_CAPABILITY.to_owned()),
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
