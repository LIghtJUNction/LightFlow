use super::RuntimeRequirement;
use serde::{Deserialize, Serialize};

/// Execution result for one workflow run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowExecution {
    pub workflow_id: String,
    pub version: String,
    pub inputs: serde_json::Map<String, serde_json::Value>,
    pub outputs: serde_json::Map<String, serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<ExecutionRuntime>,
    #[serde(default)]
    pub artifacts: Vec<WorkflowArtifact>,
    pub nodes: Vec<NodeExecution>,
}

/// Runtime executor selected for a workflow or graph node execution.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ExecutionRuntime {
    pub executor_id: String,
    pub executor_kind: String,
    pub capabilities: Vec<String>,
    pub data_policy: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub declared: Vec<RuntimeRequirement>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replay_fingerprint: Option<serde_json::Value>,
}

/// Materialized file produced by a workflow run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowArtifact {
    pub id: String,
    pub kind: String,
    pub path: String,
    pub mime_type: String,
    #[serde(default)]
    pub metadata: serde_json::Map<String, serde_json::Value>,
}

/// Runtime result for one node.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeExecution {
    pub node_id: String,
    pub workflow_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub selected_workflow_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub runtime: Option<ExecutionRuntime>,
    pub status: NodeExecutionStatus,
    #[serde(default)]
    pub duration_ms: u64,
    #[serde(default)]
    pub attempts: usize,
    #[serde(default)]
    pub inputs: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub outputs: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub artifacts: Vec<WorkflowArtifact>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub nodes: Vec<NodeExecution>,
}

/// Runtime state of one node.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeExecutionStatus {
    Completed,
    Skipped,
}
