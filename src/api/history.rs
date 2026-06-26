#[path = "history/artifacts.rs"]
mod artifacts;
#[path = "history/events.rs"]
mod events;
#[path = "history/query.rs"]
mod query;
#[path = "history/recording.rs"]
mod recording;
#[path = "history/storage.rs"]
mod storage;
#[cfg(test)]
#[path = "history/tests/mod.rs"]
mod tests;
#[path = "history/types.rs"]
mod types;

use std::path::Path;

use super::ApiResult;
use crate::workflow::WorkflowExecutionOptions;

pub use types::{
    ArtifactCatalog, ArtifactListOptions, RecordedRun, RemovedRun, ReplayStages, RunArtifact,
    RunCatalog, RunEvents, RunListOptions, RunStageRecord, RunSummary, RunTrace,
};

pub(super) fn list_artifacts(root: &Path) -> ApiResult<ArtifactCatalog> {
    artifacts::list_artifacts(root)
}

pub(super) fn list_artifacts_with_options(
    root: &Path,
    options: &ArtifactListOptions,
) -> ApiResult<ArtifactCatalog> {
    artifacts::list_artifacts_with_options(root, options)
}

pub(super) fn get_run(root: &Path, selector: &str) -> ApiResult<RunTrace> {
    query::get_run(root, selector)
}

pub(super) fn get_run_events(root: &Path, selector: &str) -> ApiResult<RunEvents> {
    query::get_run_events(root, selector)
}

pub(super) fn list_runs(root: &Path) -> ApiResult<RunCatalog> {
    query::list_runs(root)
}

pub(super) fn list_runs_with_options(
    root: &Path,
    options: &RunListOptions,
) -> ApiResult<RunCatalog> {
    query::list_runs_with_options(root, options)
}

pub(super) fn remove_run(root: &Path, selector: &str) -> ApiResult<RemovedRun> {
    query::remove_run(root, selector)
}

pub(super) fn replay_stages(root: &Path, selector: &str) -> ApiResult<ReplayStages> {
    query::replay_stages(root, selector)
}

pub(super) fn now_ms() -> u128 {
    storage::now_ms()
}

pub(super) fn record_completed_workflow_run(
    root: &Path,
    workflow_id: &str,
    options: &WorkflowExecutionOptions,
    execution: &impl serde::Serialize,
    started_at_ms: u128,
    completed_at_ms: u128,
) -> ApiResult<RecordedRun> {
    recording::record_completed_workflow_run(
        root,
        workflow_id,
        options,
        execution,
        started_at_ms,
        completed_at_ms,
    )
}

pub(super) fn record_completed_workflow_run_with_surface(
    root: &Path,
    workflow_id: &str,
    options: &WorkflowExecutionOptions,
    execution: &impl serde::Serialize,
    started_at_ms: u128,
    completed_at_ms: u128,
    surface: &str,
) -> ApiResult<RecordedRun> {
    recording::record_completed_workflow_run_with_surface(
        root,
        workflow_id,
        options,
        execution,
        started_at_ms,
        completed_at_ms,
        surface,
    )
}

pub(super) fn record_completed_run_with_surface(
    root: &Path,
    stages: &[RunStageRecord],
    execution: &impl serde::Serialize,
    started_at_ms: u128,
    completed_at_ms: u128,
    surface: &str,
) -> ApiResult<RecordedRun> {
    recording::record_completed_run_with_surface(
        root,
        stages,
        execution,
        started_at_ms,
        completed_at_ms,
        surface,
    )
}

pub(super) fn record_failed_workflow_run(
    root: &Path,
    workflow_id: &str,
    options: &WorkflowExecutionOptions,
    error: &serde_json::Value,
    started_at_ms: u128,
    completed_at_ms: u128,
) -> ApiResult<RecordedRun> {
    recording::record_failed_workflow_run(
        root,
        workflow_id,
        options,
        error,
        started_at_ms,
        completed_at_ms,
    )
}

pub(super) fn record_failed_workflow_run_with_surface(
    root: &Path,
    workflow_id: &str,
    options: &WorkflowExecutionOptions,
    error: &serde_json::Value,
    started_at_ms: u128,
    completed_at_ms: u128,
    surface: &str,
) -> ApiResult<RecordedRun> {
    recording::record_failed_workflow_run_with_surface(
        root,
        workflow_id,
        options,
        error,
        started_at_ms,
        completed_at_ms,
        surface,
    )
}

pub(super) fn record_failed_run_with_surface(
    root: &Path,
    stages: &[RunStageRecord],
    error: &serde_json::Value,
    partial_execution: Option<&impl serde::Serialize>,
    started_at_ms: u128,
    completed_at_ms: u128,
    surface: &str,
) -> ApiResult<RecordedRun> {
    recording::record_failed_run_with_surface(
        root,
        stages,
        error,
        partial_execution,
        started_at_ms,
        completed_at_ms,
        surface,
    )
}

#[cfg(test)]
pub(crate) use tests::write_history_fixture;
