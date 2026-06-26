use super::error::McpError;
use crate::api::{
    ApiService, ArtifactListOptions, ModelListOptions, ModelStatusFilter, RunListOptions,
};
use crate::workflow::{WorkflowExecutionOptions, WorkflowPatch, WorkflowSpec};
use serde_json::{Value, json};

pub(super) fn workflow_arg(arguments: &Value) -> Result<WorkflowSpec, McpError> {
    let workflow = arguments
        .get("workflow")
        .ok_or_else(|| McpError::new(-32602, "missing object argument: workflow"))?;
    serde_json::from_value(workflow.clone()).map_err(McpError::from)
}

pub(super) fn patch_arg(arguments: &Value) -> Result<WorkflowPatch, McpError> {
    let patch = arguments
        .get("patch")
        .ok_or_else(|| McpError::new(-32602, "missing object argument: patch"))?;
    serde_json::from_value(patch.clone()).map_err(McpError::from)
}

fn optional_usize_arg(
    arguments: &Value,
    name: &str,
    context: &str,
) -> Result<Option<usize>, McpError> {
    match arguments.get(name) {
        Some(value) => {
            let Some(limit) = value.as_u64() else {
                return Err(McpError::new(
                    -32602,
                    format!("{context} {name} must be a non-negative integer"),
                ));
            };
            Ok(Some(usize::try_from(limit).map_err(|_| {
                McpError::new(-32602, format!("{context} {name} is too large"))
            })?))
        }
        None => Ok(None),
    }
}

pub(super) fn run_list_options_arg(arguments: &Value) -> Result<RunListOptions, McpError> {
    Ok(RunListOptions {
        limit: optional_usize_arg(arguments, "limit", "lightflow.run.list")?,
        workflow_id: arguments
            .get("workflow_id")
            .and_then(Value::as_str)
            .map(str::to_owned),
        status: arguments
            .get("status")
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

pub(super) fn artifact_list_options_arg(
    arguments: &Value,
) -> Result<ArtifactListOptions, McpError> {
    Ok(ArtifactListOptions {
        limit: optional_usize_arg(arguments, "limit", "lightflow.artifact.list")?,
        run_id: arguments
            .get("run_id")
            .and_then(Value::as_str)
            .map(str::to_owned),
        workflow_id: arguments
            .get("workflow_id")
            .and_then(Value::as_str)
            .map(str::to_owned),
        kind: arguments
            .get("kind")
            .and_then(Value::as_str)
            .map(str::to_owned),
    })
}

pub(super) fn model_list_options_arg(arguments: &Value) -> Result<ModelListOptions, McpError> {
    let status = match arguments.get("status").and_then(Value::as_str) {
        Some(value) => model_status_filter(value)?,
        None => ModelStatusFilter::All,
    };
    Ok(ModelListOptions {
        workflow_id: arguments
            .get("workflow_id")
            .and_then(Value::as_str)
            .map(str::to_owned),
        status,
    })
}

pub(super) fn model_status_filter(value: &str) -> Result<ModelStatusFilter, McpError> {
    ModelStatusFilter::parse(value).ok_or_else(|| {
        McpError::new(
            -32602,
            format!("unsupported model status {value}; expected all, available, or blocked"),
        )
    })
}

fn execution_options_arg(arguments: &Value) -> Result<WorkflowExecutionOptions, McpError> {
    let value = json!({
        "inputs": arguments.get("inputs").cloned().unwrap_or_else(|| json!({})),
        "disabled_nodes": arguments.get("disabled_nodes").cloned().unwrap_or_else(|| json!([])),
        "enabled_nodes": arguments.get("enabled_nodes").cloned().unwrap_or_else(|| json!([])),
        "patch": arguments.get("patch").cloned().unwrap_or(Value::Null),
    });
    serde_json::from_value(value).map_err(McpError::from)
}

pub(super) fn recorded_workflow_run(
    service: &ApiService,
    arguments: &Value,
) -> Result<Value, McpError> {
    let workflow_id = required_str(arguments, "workflow_id")?;
    let options = execution_options_arg(arguments)?;
    let started_at_ms = ApiService::now_ms();
    let execution = match service.execute_workflow(workflow_id, options.clone()) {
        Ok(execution) => execution,
        Err(error) => {
            let completed_at_ms = ApiService::now_ms();
            let error_json = json!({
                "code": error.code(),
                "message": error.message(),
            });
            let record = service.record_failed_workflow_run_with_surface(
                workflow_id,
                &options,
                &error_json,
                started_at_ms,
                completed_at_ms,
                "mcp",
            )?;
            return Err(McpError::from(error).with_data(json!({
                "run_id": record.run_id,
                "run_dir": record.run_dir,
                "trace_path": record.run_dir.join("execution.json"),
            })));
        }
    };
    let completed_at_ms = ApiService::now_ms();
    let mut value = service.execution_with_model_locks(&execution)?;
    let record = service.record_completed_workflow_run_with_surface(
        workflow_id,
        &options,
        &value,
        started_at_ms,
        completed_at_ms,
        "mcp",
    )?;
    let Some(object) = value.as_object_mut() else {
        return Err(McpError::new(
            -32603,
            "workflow execution output must be an object",
        ));
    };
    object.insert("run_id".to_owned(), record.run_id.into());
    object.insert(
        "run_dir".to_owned(),
        record.run_dir.display().to_string().into(),
    );
    object.insert(
        "trace_path".to_owned(),
        record
            .run_dir
            .join("execution.json")
            .display()
            .to_string()
            .into(),
    );
    Ok(value)
}

pub(super) fn required_str<'a>(value: &'a Value, key: &str) -> Result<&'a str, McpError> {
    value
        .get(key)
        .and_then(Value::as_str)
        .ok_or_else(|| McpError::new(-32602, format!("missing string argument: {key}")))
}
