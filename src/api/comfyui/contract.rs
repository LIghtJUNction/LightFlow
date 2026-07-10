use std::collections::BTreeSet;
use std::path::Path;
use std::time::Duration;

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use super::config;
use super::deadline::Deadline;
use super::uploads::{self, Upload, UploadBinding};
use crate::api::{ApiError, ApiResult};
use crate::workflow::WorkflowSpec;

#[derive(Debug)]
pub(super) struct RequestContract {
    pub(super) workflow: Value,
    pub(super) uploads: Vec<Upload>,
    pub(super) project_root: std::path::PathBuf,
    pub(super) server_url: String,
    pub(super) authorization: Option<String>,
    pub(super) client_id: Option<String>,
    pub(super) extra_data: Option<Map<String, Value>>,
    pub(super) output_node_ids: Option<BTreeSet<String>>,
    pub(super) output_dir: Option<super::paths::OutputDirectory>,
    pub(super) default_output_relative: std::path::PathBuf,
    pub(super) timeout: Duration,
    pub(super) poll_interval: Duration,
}

pub(super) fn parse(
    root: &Path,
    workflow_spec: &WorkflowSpec,
    inputs: &Map<String, Value>,
    deadline: &Deadline,
) -> ApiResult<RequestContract> {
    if inputs.contains_key("workflow_path") {
        return invalid(
            "workflow_path is not supported; provide an inline ComfyUI API Format workflow",
        );
    }
    let mut workflow = required_workflow(inputs)?;
    apply_node_inputs(&mut workflow, inputs.get("node_inputs"))?;
    let uploads = uploads::parse(root, &workflow, inputs.get("uploads"), deadline)?;
    let config = config::parse(root, &workflow_spec.id, inputs)?;
    Ok(RequestContract {
        workflow,
        uploads,
        project_root: config.project_root,
        server_url: config.server_url,
        authorization: config.authorization,
        client_id: config.client_id,
        extra_data: config.extra_data,
        output_node_ids: config.output_node_ids,
        output_dir: config.output_dir,
        default_output_relative: config.default_output_relative,
        timeout: config.timeout,
        poll_interval: config.poll_interval,
    })
}

pub(super) fn apply_upload_binding(
    workflow: &mut Value,
    bindings: &[UploadBinding],
    reference: &str,
) -> ApiResult<()> {
    for binding in bindings {
        let inputs = node_inputs_mut(workflow, &binding.node_id, "upload bind")?;
        inputs.insert(binding.input.clone(), Value::String(reference.to_owned()));
    }
    Ok(())
}

pub(super) fn workflow_sha256(workflow: &Value) -> ApiResult<String> {
    let bytes = serde_json::to_vec(workflow).map_err(|error| {
        ApiError::InvalidRequest(format!("serialize ComfyUI workflow: {error}"))
    })?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}

fn required_workflow(inputs: &Map<String, Value>) -> ApiResult<Value> {
    let Some(Value::Object(workflow)) = inputs.get("workflow") else {
        return invalid("workflow must be a required non-empty ComfyUI API Format JSON object");
    };
    if workflow.is_empty() {
        return invalid("workflow must be a required non-empty ComfyUI API Format JSON object");
    }
    for (node_id, node) in workflow {
        let Some(node) = node.as_object() else {
            return invalid(format!("workflow node {node_id} must be an object"));
        };
        if node
            .get("class_type")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        {
            return invalid(format!(
                "workflow node {node_id} must contain non-empty class_type"
            ));
        }
        if !node.get("inputs").is_some_and(Value::is_object) {
            return invalid(format!(
                "workflow node {node_id} must contain object inputs"
            ));
        }
    }
    Ok(Value::Object(workflow.clone()))
}

fn apply_node_inputs(workflow: &mut Value, value: Option<&Value>) -> ApiResult<()> {
    let Some(value) = value else {
        return Ok(());
    };
    let Some(overrides) = value.as_object() else {
        return invalid("node_inputs must be an object");
    };
    for (node_id, value) in overrides {
        let Some(values) = value.as_object() else {
            return invalid(format!("node_inputs.{node_id} must be an object"));
        };
        let inputs = node_inputs_mut(workflow, node_id, "node_inputs")?;
        for (name, value) in values {
            inputs.insert(name.clone(), value.clone());
        }
    }
    Ok(())
}

fn node_inputs_mut<'a>(
    workflow: &'a mut Value,
    node_id: &str,
    context: &str,
) -> ApiResult<&'a mut Map<String, Value>> {
    workflow
        .as_object_mut()
        .and_then(|workflow| workflow.get_mut(node_id))
        .and_then(Value::as_object_mut)
        .and_then(|node| node.get_mut("inputs"))
        .and_then(Value::as_object_mut)
        .ok_or_else(|| {
            ApiError::InvalidRequest(format!(
                "{context} references unknown node or inputs container {node_id}"
            ))
        })
}

fn invalid<T>(message: impl Into<String>) -> ApiResult<T> {
    Err(ApiError::InvalidRequest(message.into()))
}
