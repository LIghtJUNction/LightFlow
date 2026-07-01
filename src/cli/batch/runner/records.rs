use crate::workflow::WorkflowArtifact;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct BatchManifest {
    pub(super) run_id: String,
    pub(super) workflow_id: Option<String>,
    pub(super) max_gpu_jobs: usize,
    pub(super) max_cpu_jobs: usize,
    pub(super) batch_size: usize,
    pub(super) retries: u32,
    pub(super) reserve_mem: Option<String>,
    pub(super) reserve_vram: Option<String>,
    pub(super) max_load: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct BatchJobDefinition {
    #[serde(default)]
    pub(super) id: Option<String>,
    #[serde(default)]
    pub(super) workflow_id: Option<String>,
    #[serde(default)]
    pub(super) inputs: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub(super) disabled_nodes: Vec<String>,
    #[serde(default)]
    pub(super) enabled_nodes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(super) struct BatchJobRecord {
    pub(super) id: String,
    pub(super) workflow_id: String,
    #[serde(default)]
    pub(super) inputs: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub(super) disabled_nodes: Vec<String>,
    #[serde(default)]
    pub(super) enabled_nodes: Vec<String>,
    pub(super) status: BatchJobStatus,
    pub(super) attempts: u32,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) outputs: Option<serde_json::Map<String, serde_json::Value>>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub(super) artifacts: Vec<WorkflowArtifact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) error: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) started_at_ms: Option<u128>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub(super) completed_at_ms: Option<u128>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum BatchJobStatus {
    Queued,
    Running,
    Completed,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub(super) struct BatchRunSummary {
    pub(super) run_id: String,
    pub(super) run_dir: String,
    pub(super) total: usize,
    pub(super) completed: usize,
    pub(super) failed: usize,
    pub(super) queued: usize,
    pub(super) max_gpu_jobs: usize,
    pub(super) max_cpu_jobs: usize,
    pub(super) batch_size: usize,
    pub(super) resource_policy: serde_json::Value,
}

#[derive(Debug, Clone)]
pub(super) struct JobOutcome {
    pub(super) index: usize,
    pub(super) outputs: Option<serde_json::Map<String, serde_json::Value>>,
    pub(super) artifacts: Vec<WorkflowArtifact>,
    pub(super) error: Option<String>,
}
