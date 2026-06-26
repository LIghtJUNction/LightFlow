use super::RecordedRun;
use super::storage;
use super::types::RunStageRecord;
use crate::api::{ApiError, ApiResult};
use crate::workflow::WorkflowExecutionOptions;
use serde::Serialize;
use serde_json::Value;
use std::fs;
use std::path::Path;

pub(super) fn record_completed_workflow_run(
    root: &Path,
    workflow_id: &str,
    options: &WorkflowExecutionOptions,
    execution: &impl Serialize,
    started_at_ms: u128,
    completed_at_ms: u128,
) -> ApiResult<RecordedRun> {
    let stages = [RunStageRecord {
        workflow_id: workflow_id.to_owned(),
        execution: options.clone(),
    }];
    record_completed_run(root, &stages, execution, started_at_ms, completed_at_ms)
}

pub(super) fn record_completed_workflow_run_with_surface(
    root: &Path,
    workflow_id: &str,
    options: &WorkflowExecutionOptions,
    execution: &impl Serialize,
    started_at_ms: u128,
    completed_at_ms: u128,
    surface: &str,
) -> ApiResult<RecordedRun> {
    let stages = [RunStageRecord {
        workflow_id: workflow_id.to_owned(),
        execution: options.clone(),
    }];
    record_completed_run_with_surface(
        root,
        &stages,
        execution,
        started_at_ms,
        completed_at_ms,
        surface,
    )
}

pub(super) fn record_completed_run(
    root: &Path,
    stages: &[RunStageRecord],
    execution: &impl Serialize,
    started_at_ms: u128,
    completed_at_ms: u128,
) -> ApiResult<RecordedRun> {
    record_completed_run_with_surface(
        root,
        stages,
        execution,
        started_at_ms,
        completed_at_ms,
        "http",
    )
}

pub(super) fn record_completed_run_with_surface(
    root: &Path,
    stages: &[RunStageRecord],
    execution: &impl Serialize,
    started_at_ms: u128,
    completed_at_ms: u128,
    surface: &str,
) -> ApiResult<RecordedRun> {
    let run_id = storage::unique_run_id(root, completed_at_ms)?;
    let run_dir = storage::run_dir(root, &run_id);
    let execution = serde_json::to_value(execution)
        .map_err(|error| ApiError::InvalidRequest(format!("invalid execution JSON: {error}")))?;
    fs::create_dir_all(&run_dir)?;
    storage::write_run_manifest(
        &run_dir,
        &run_id,
        "completed",
        stages,
        started_at_ms,
        completed_at_ms,
    )?;
    storage::write_json_pretty(&run_dir.join("execution.json"), &execution)?;
    storage::append_event(
        &run_dir,
        serde_json::json!({
            "event": "run_started",
            "run_id": run_id,
            "at_ms": started_at_ms,
            "surface": surface,
        }),
    )?;
    super::events::append_execution_events(&run_dir, &run_id, &execution)?;
    storage::append_event(
        &run_dir,
        serde_json::json!({
            "event": "run_finished",
            "run_id": run_id,
            "at_ms": completed_at_ms,
            "surface": surface,
        }),
    )?;
    storage::write_text(&storage::runs_root(root).join("last"), &run_id)?;
    Ok(RecordedRun { run_id, run_dir })
}

pub(super) fn record_failed_workflow_run(
    root: &Path,
    workflow_id: &str,
    options: &WorkflowExecutionOptions,
    error: &Value,
    started_at_ms: u128,
    completed_at_ms: u128,
) -> ApiResult<RecordedRun> {
    let stages = [RunStageRecord {
        workflow_id: workflow_id.to_owned(),
        execution: options.clone(),
    }];
    record_failed_run(root, &stages, error, started_at_ms, completed_at_ms)
}

pub(super) fn record_failed_workflow_run_with_surface(
    root: &Path,
    workflow_id: &str,
    options: &WorkflowExecutionOptions,
    error: &Value,
    started_at_ms: u128,
    completed_at_ms: u128,
    surface: &str,
) -> ApiResult<RecordedRun> {
    let stages = [RunStageRecord {
        workflow_id: workflow_id.to_owned(),
        execution: options.clone(),
    }];
    record_failed_run_with_surface(
        root,
        &stages,
        error,
        None::<&Value>,
        started_at_ms,
        completed_at_ms,
        surface,
    )
}

pub(super) fn record_failed_run(
    root: &Path,
    stages: &[RunStageRecord],
    error: &Value,
    started_at_ms: u128,
    completed_at_ms: u128,
) -> ApiResult<RecordedRun> {
    record_failed_run_with_surface(
        root,
        stages,
        error,
        None::<&Value>,
        started_at_ms,
        completed_at_ms,
        "http",
    )
}

pub(super) fn record_failed_run_with_surface(
    root: &Path,
    stages: &[RunStageRecord],
    error: &Value,
    partial_execution: Option<&impl Serialize>,
    started_at_ms: u128,
    completed_at_ms: u128,
    surface: &str,
) -> ApiResult<RecordedRun> {
    let run_id = storage::unique_run_id(root, completed_at_ms)?;
    let run_dir = storage::run_dir(root, &run_id);
    let partial_execution = partial_execution
        .map(serde_json::to_value)
        .transpose()
        .map_err(|error| {
            ApiError::InvalidRequest(format!("invalid partial execution JSON: {error}"))
        })?;
    let mut execution = serde_json::json!({
        "status": "failed",
        "error": error,
        "stages": stages,
    });
    if let Some(partial_execution) = partial_execution {
        execution["partial_execution"] = partial_execution;
    }
    fs::create_dir_all(&run_dir)?;
    storage::write_run_manifest(
        &run_dir,
        &run_id,
        "failed",
        stages,
        started_at_ms,
        completed_at_ms,
    )?;
    storage::write_json_pretty(&run_dir.join("execution.json"), &execution)?;
    storage::append_event(
        &run_dir,
        serde_json::json!({
            "event": "run_started",
            "run_id": run_id,
            "at_ms": started_at_ms,
            "surface": surface,
        }),
    )?;
    if let Some(partial_execution) = execution.get("partial_execution") {
        super::events::append_execution_events(&run_dir, &run_id, partial_execution)?;
    }
    storage::append_event(
        &run_dir,
        serde_json::json!({
            "event": "run_failed",
            "run_id": run_id,
            "at_ms": completed_at_ms,
            "surface": surface,
            "error": error,
        }),
    )?;
    storage::write_text(&storage::runs_root(root).join("last"), &run_id)?;
    Ok(RecordedRun { run_id, run_dir })
}
