use super::is_false;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Runtime inputs and node toggles for one workflow execution.
#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkflowExecutionOptions {
    #[serde(default)]
    pub inputs: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub disabled_nodes: Vec<String>,
    #[serde(default)]
    pub enabled_nodes: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub patch: Option<WorkflowPatch>,
}

/// Runtime patch applied at workflow node boundaries.
#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkflowPatch {
    #[serde(default)]
    pub nodes: BTreeMap<String, WorkflowNodePatch>,
}

/// Patch behavior for one workflow graph node.
#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkflowNodePatch {
    #[serde(default, skip_serializing_if = "is_false")]
    pub disable: bool,
    #[serde(default, skip_serializing_if = "is_false")]
    pub enable: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub replace_with: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback_workflow_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry: Option<usize>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout_ms: Option<u64>,
}
