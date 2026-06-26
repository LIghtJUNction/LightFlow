use crate::workflow::{WorkflowArtifact, WorkflowExecutionOptions};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunCatalog {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last: Option<String>,
    pub total: usize,
    pub completed_count: usize,
    pub failed_count: usize,
    pub unknown_count: usize,
    pub unknown_run_ids: Vec<String>,
    pub issues: Vec<String>,
    pub runs: Vec<RunSummary>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunListOptions {
    pub limit: Option<usize>,
    pub workflow_id: Option<String>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ArtifactListOptions {
    pub limit: Option<usize>,
    pub run_id: Option<String>,
    pub workflow_id: Option<String>,
    pub kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunSummary {
    pub run_id: String,
    pub status: String,
    pub started_at_ms: u128,
    pub completed_at_ms: u128,
    pub duration_ms: u128,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    pub workflow_ids: Vec<String>,
    pub stages: usize,
    pub run_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunTrace {
    pub run_id: String,
    pub run_dir: PathBuf,
    pub manifest: serde_json::Value,
    pub execution: serde_json::Value,
    pub events: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunEvents {
    pub run_id: String,
    pub events: Vec<serde_json::Value>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ArtifactCatalog {
    pub artifacts: Vec<RunArtifact>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RunArtifact {
    pub run_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stage_index: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_index: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub workflow_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub node_id: Option<String>,
    pub artifact: WorkflowArtifact,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct RecordedRun {
    pub run_id: String,
    pub run_dir: PathBuf,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct RemovedRun {
    pub removed: bool,
    pub run_id: String,
    pub run_dir: PathBuf,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct RecordedRunManifest {
    pub kind: String,
    pub run_id: String,
    pub status: String,
    #[serde(default = "default_stage_input_resolution")]
    pub stage_input_resolution: String,
    pub started_at_ms: u128,
    pub completed_at_ms: u128,
    pub stages: Vec<RunStageRecord>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct ReplayStages {
    pub stages: Vec<RunStageRecord>,
    pub stage_inputs_resolved: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RunStageRecord {
    pub workflow_id: String,
    pub execution: WorkflowExecutionOptions,
}

fn default_stage_input_resolution() -> String {
    "legacy".to_owned()
}
