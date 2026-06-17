use crate::component::PortSpec;
use serde::{Deserialize, Serialize};

/// A directed workflow graph. Workflows can be used as nodes inside other
/// workflows, so composition is represented by one concept instead of a
/// separate "composition" asset kind.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowSpec {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub inputs: Vec<PortSpec>,
    #[serde(default)]
    pub outputs: Vec<PortSpec>,
    #[serde(default)]
    pub nodes: Vec<WorkflowNode>,
    #[serde(default)]
    pub edges: Vec<WorkflowEdge>,
}

/// One node in a workflow graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowNode {
    pub id: String,
    #[serde(flatten)]
    pub uses: WorkflowNodeTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub position: WorkflowPosition,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub config: serde_json::Value,
}

/// The executable target for a workflow node.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "uses", rename_all = "snake_case")]
pub enum WorkflowNodeTarget {
    Component { component_id: String },
    Workflow { workflow_id: String },
}

impl WorkflowNodeTarget {
    #[must_use]
    pub fn target_id(&self) -> &str {
        match self {
            Self::Component { component_id } => component_id,
            Self::Workflow { workflow_id } => workflow_id,
        }
    }
}

/// Canvas position stored with the workflow as authoring metadata.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkflowPosition {
    pub x: i64,
    pub y: i64,
}

/// Directed edge between two node ports.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkflowEdge {
    pub from: WorkflowEndpoint,
    pub to: WorkflowEndpoint,
}

/// One side of a workflow edge.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkflowEndpoint {
    pub node: String,
    pub port: String,
}

/// List response for workflow endpoints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowList {
    pub workflows: Vec<WorkflowSummary>,
}

/// Compact workflow row for browsers.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkflowSummary {
    pub id: String,
    pub name: String,
    pub inputs: usize,
    pub outputs: usize,
    pub nodes: usize,
    pub edges: usize,
}

impl From<WorkflowSpec> for WorkflowSummary {
    fn from(workflow: WorkflowSpec) -> Self {
        Self {
            id: workflow.id,
            name: workflow.name,
            inputs: workflow.inputs.len(),
            outputs: workflow.outputs.len(),
            nodes: workflow.nodes.len(),
            edges: workflow.edges.len(),
        }
    }
}

/// Validation result for a workflow graph.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkflowValidation {
    pub valid: bool,
    pub issues: Vec<String>,
    pub topological_order: Vec<String>,
}
