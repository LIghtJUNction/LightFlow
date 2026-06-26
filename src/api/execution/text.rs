#[path = "text_control.rs"]
mod text_control;
#[path = "text_helpers.rs"]
mod text_helpers;
#[path = "text_llm.rs"]
mod text_llm;
#[path = "text_model.rs"]
mod text_model;
#[path = "text_transform.rs"]
mod text_transform;

use crate::api::ApiResult;
use crate::api::execution::types::LeafExecution;
use crate::workflow::WorkflowSpec;

pub(super) fn execute_text_concat(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    text_transform::execute_text_concat(workflow, inputs)
}

pub(super) fn execute_text_template(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    text_transform::execute_text_template(workflow, inputs)
}

pub(super) fn execute_text_regex(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    text_transform::execute_text_regex(workflow, inputs)
}

pub(super) fn execute_json_extract(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    text_transform::execute_json_extract(workflow, inputs)
}

pub(super) fn execute_model_select(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    text_model::execute_model_select(workflow, inputs)
}

pub(super) fn execute_model_lock_check(
    root: &std::path::Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    text_model::execute_model_lock_check(root, workflow, inputs)
}

pub(super) fn execute_builtin_llm_generate(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    text_llm::execute_builtin_llm_generate(workflow, inputs)
}

pub(super) fn execute_llm_classify(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    text_llm::execute_llm_classify(workflow, inputs)
}

pub(super) fn execute_llm_structured_output(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    text_llm::execute_llm_structured_output(workflow, inputs)
}

pub(super) fn execute_control_if(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    text_control::execute_control_if(workflow, inputs)
}

pub(super) fn execute_control_switch(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    text_control::execute_control_switch(workflow, inputs)
}

pub(super) fn execute_control_merge(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    text_control::execute_control_merge(workflow, inputs)
}

pub(super) fn execute_control_split(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    text_control::execute_control_split(workflow, inputs)
}
