use super::storage;
use crate::api::ApiResult;
use serde_json::{Value, json};
use std::path::Path;

pub(super) fn append_execution_events(
    run_dir: &Path,
    run_id: &str,
    execution: &Value,
) -> ApiResult<()> {
    if execution.get("pipeline").and_then(Value::as_bool) == Some(true) {
        for (stage_index, stage) in execution
            .get("stages")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            append_stage_node_events(run_dir, run_id, stage_index, stage)?;
            append_stage_completed_event(run_dir, run_id, stage_index, stage)?;
        }
        return Ok(());
    }

    append_stage_node_events(run_dir, run_id, 0, execution)
}

fn append_stage_completed_event(
    run_dir: &Path,
    run_id: &str,
    stage_index: usize,
    execution: &Value,
) -> ApiResult<()> {
    storage::append_event(
        run_dir,
        json!({
            "event": "stage_completed",
            "run_id": run_id,
            "stage_index": stage_index,
            "workflow_id": execution.get("workflow_id").cloned().unwrap_or_default(),
            "outputs": execution.get("outputs").cloned().unwrap_or_else(|| json!({})),
            "artifacts": execution.get("artifacts").cloned().unwrap_or_else(|| json!([])),
            "runtime": execution.get("runtime").cloned().unwrap_or(Value::Null),
        }),
    )
}

fn append_stage_node_events(
    run_dir: &Path,
    run_id: &str,
    stage_index: usize,
    execution: &Value,
) -> ApiResult<()> {
    let workflow_id = execution
        .get("workflow_id")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let Some(nodes) = execution.get("nodes").and_then(Value::as_array) else {
        return Ok(());
    };
    for (node_index, node) in nodes.iter().enumerate() {
        let status = node
            .get("status")
            .and_then(Value::as_str)
            .unwrap_or("unknown");
        let event = if status == "skipped" {
            "node_skipped"
        } else {
            "node_completed"
        };
        storage::append_event(
            run_dir,
            json!({
                "event": event,
                "run_id": run_id,
                "stage_index": stage_index,
                "node_index": node_index,
                "workflow_id": workflow_id,
                "node_id": node.get("node_id").cloned().unwrap_or_default(),
                "node_workflow_id": node.get("workflow_id").cloned().unwrap_or_default(),
                "selected_workflow_id": node.get("selected_workflow_id").cloned().unwrap_or(Value::Null),
                "status": status,
                "duration_ms": node.get("duration_ms").cloned().unwrap_or(0.into()),
                "attempts": node.get("attempts").cloned().unwrap_or(0.into()),
                "inputs": node.get("inputs").cloned().unwrap_or_else(|| json!({})),
                "outputs": node.get("outputs").cloned().unwrap_or_else(|| json!({})),
                "runtime": node.get("runtime").cloned().unwrap_or(Value::Null),
                "artifacts": node.get("artifacts").cloned().unwrap_or_else(|| json!([])),
            }),
        )?;
    }
    Ok(())
}
