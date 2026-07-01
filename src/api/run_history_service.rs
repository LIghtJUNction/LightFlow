use super::{
    ApiResult, ApiService, ArtifactCatalog, ArtifactListOptions, RecordedRun, RemovedRun,
    RunCatalog, RunEvents, RunListOptions, RunStageRecord, RunTrace, history,
};
use crate::workflow::WorkflowExecutionOptions;

mod replay;

impl ApiService {
    /// List recorded local runs.
    pub fn list_runs(&self) -> ApiResult<RunCatalog> {
        history::list_runs(&self.repo_root)
    }

    /// List recorded local runs with optional presentation filters.
    pub fn list_runs_with_options(&self, options: &RunListOptions) -> ApiResult<RunCatalog> {
        history::list_runs_with_options(&self.repo_root, options)
    }

    /// Read a recorded run trace by id, or `last`.
    pub fn get_run(&self, selector: &str) -> ApiResult<RunTrace> {
        history::get_run(&self.repo_root, selector)
    }

    /// Read only events for a recorded run.
    pub fn get_run_events(&self, selector: &str) -> ApiResult<RunEvents> {
        history::get_run_events(&self.repo_root, selector)
    }

    /// Remove a recorded run by id, or `last`.
    pub fn remove_run(&self, selector: &str) -> ApiResult<RemovedRun> {
        history::remove_run(&self.repo_root, selector)
    }

    /// List artifacts produced by recorded runs.
    pub fn list_artifacts(&self) -> ApiResult<ArtifactCatalog> {
        history::list_artifacts(&self.repo_root)
    }

    /// List artifacts produced by recorded runs with optional filters.
    pub fn list_artifacts_with_options(
        &self,
        options: &ArtifactListOptions,
    ) -> ApiResult<ArtifactCatalog> {
        history::list_artifacts_with_options(&self.repo_root, options)
    }

    /// Record a completed HTTP/API workflow run in project-local history.
    pub fn record_completed_workflow_run(
        &self,
        workflow_id: &str,
        options: &WorkflowExecutionOptions,
        execution: &impl serde::Serialize,
        started_at_ms: u128,
        completed_at_ms: u128,
    ) -> ApiResult<RecordedRun> {
        history::record_completed_workflow_run(
            &self.repo_root,
            workflow_id,
            options,
            execution,
            started_at_ms,
            completed_at_ms,
        )
    }

    /// Record a completed workflow run with an explicit adapter surface label.
    pub fn record_completed_workflow_run_with_surface(
        &self,
        workflow_id: &str,
        options: &WorkflowExecutionOptions,
        execution: &impl serde::Serialize,
        started_at_ms: u128,
        completed_at_ms: u128,
        surface: &str,
    ) -> ApiResult<RecordedRun> {
        history::record_completed_workflow_run_with_surface(
            &self.repo_root,
            workflow_id,
            options,
            execution,
            started_at_ms,
            completed_at_ms,
            surface,
        )
    }

    /// Record a completed staged run with an explicit adapter surface label.
    pub fn record_completed_run_with_surface(
        &self,
        stages: &[RunStageRecord],
        execution: &impl serde::Serialize,
        started_at_ms: u128,
        completed_at_ms: u128,
        surface: &str,
    ) -> ApiResult<RecordedRun> {
        history::record_completed_run_with_surface(
            &self.repo_root,
            stages,
            execution,
            started_at_ms,
            completed_at_ms,
            surface,
        )
    }

    /// Record a failed HTTP/API workflow run in project-local history.
    pub fn record_failed_workflow_run(
        &self,
        workflow_id: &str,
        options: &WorkflowExecutionOptions,
        error: &serde_json::Value,
        started_at_ms: u128,
        completed_at_ms: u128,
    ) -> ApiResult<RecordedRun> {
        history::record_failed_workflow_run(
            &self.repo_root,
            workflow_id,
            options,
            error,
            started_at_ms,
            completed_at_ms,
        )
    }

    /// Record a failed workflow run with an explicit adapter surface label.
    pub fn record_failed_workflow_run_with_surface(
        &self,
        workflow_id: &str,
        options: &WorkflowExecutionOptions,
        error: &serde_json::Value,
        started_at_ms: u128,
        completed_at_ms: u128,
        surface: &str,
    ) -> ApiResult<RecordedRun> {
        history::record_failed_workflow_run_with_surface(
            &self.repo_root,
            workflow_id,
            options,
            error,
            started_at_ms,
            completed_at_ms,
            surface,
        )
    }

    /// Record a failed staged run with an explicit adapter surface label.
    pub fn record_failed_run_with_surface(
        &self,
        stages: &[RunStageRecord],
        error: &serde_json::Value,
        partial_execution: Option<&impl serde::Serialize>,
        started_at_ms: u128,
        completed_at_ms: u128,
        surface: &str,
    ) -> ApiResult<RecordedRun> {
        history::record_failed_run_with_surface(
            &self.repo_root,
            stages,
            error,
            partial_execution,
            started_at_ms,
            completed_at_ms,
            surface,
        )
    }

    /// Current Unix epoch timestamp in milliseconds for run manifests.
    #[must_use]
    pub fn now_ms() -> u128 {
        history::now_ms()
    }
}
