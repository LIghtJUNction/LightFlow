use serde::{Deserialize, Serialize};

mod builder;
pub use builder::WorkflowBuilder;
fn default_version() -> String {
    "0.1.0".to_owned()
}

/// A LightFlow workflow.
///
/// A workflow can be a reusable leaf unit or a composite graph. Leaf workflows
/// declare ports and optional configuration but have no nodes. Composite
/// workflows declare nodes that reference other workflows.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowSpec {
    pub id: String,
    #[serde(default = "default_version")]
    pub version: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub inputs: Vec<PortSpec>,
    #[serde(default)]
    pub outputs: Vec<PortSpec>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub config_schema: serde_json::Value,
    #[serde(default)]
    pub dependencies: Vec<WorkflowDependencyRequirement>,
    #[serde(default)]
    pub models: Vec<ModelRequirement>,
    #[serde(default)]
    pub runtimes: Vec<RuntimeRequirement>,
    #[serde(default)]
    pub nodes: Vec<WorkflowNode>,
    #[serde(default)]
    pub edges: Vec<WorkflowEdge>,
}

/// A named typed input or output.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PortSpec {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
}

/// Explicit workflow dependency constraint.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkflowDependencyRequirement {
    pub workflow_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub install: Option<CargoDependency>,
}

/// Cargo dependency metadata for installing a workflow dependency.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CargoDependency {
    pub crate_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<CargoDependencySource>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub package: Option<String>,
}

/// Where Cargo should resolve an installable workflow dependency.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum CargoDependencySource {
    Path(String),
    Git(String),
}

/// A model resource needed by a workflow.
///
/// Requirements are intentionally capability-oriented. A workflow can describe
/// what kind of model it needs without forcing every user to download the same
/// concrete file.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModelRequirement {
    pub id: String,
    pub capability: String,
    #[serde(default)]
    pub variants: Vec<ModelVariant>,
}

/// One concrete model option that can satisfy a model requirement.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ModelVariant {
    pub id: String,
    pub provider: ModelProvider,
    pub format: String,
    pub repo: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

/// Supported model resource provider.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModelProvider {
    HuggingFace,
}

impl ModelProvider {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::HuggingFace => "hugging_face",
        }
    }
}

/// Runtime capability needed to execute a leaf workflow.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RuntimeRequirement {
    pub id: String,
    pub capability: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
}

/// One node in a composite workflow graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowNode {
    pub id: String,
    #[serde(default, skip_serializing_if = "is_workflow_node_kind")]
    pub kind: WorkflowNodeKind,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub workflow_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub condition: Option<WorkflowCondition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub then_workflow_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub else_workflow_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default, skip_serializing_if = "is_false")]
    pub disabled: bool,
    #[serde(default)]
    pub position: WorkflowPosition,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub config: serde_json::Value,
}

/// Kind of workflow graph node.
#[derive(Debug, Clone, Copy, Default, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkflowNodeKind {
    #[default]
    Workflow,
    If,
}

const fn is_workflow_node_kind(value: &WorkflowNodeKind) -> bool {
    matches!(value, WorkflowNodeKind::Workflow)
}

/// Runtime condition used by a control-flow node.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "op")]
pub enum WorkflowCondition {
    InputEquals {
        input: String,
        value: serde_json::Value,
    },
}

const fn is_false(value: &bool) -> bool {
    !*value
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
    pub version: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    pub inputs: usize,
    pub outputs: usize,
    pub dependencies: usize,
    pub models: usize,
    pub runtimes: usize,
    pub nodes: usize,
    pub edges: usize,
}

impl From<WorkflowSpec> for WorkflowSummary {
    fn from(workflow: WorkflowSpec) -> Self {
        Self {
            id: workflow.id,
            version: workflow.version,
            name: workflow.name,
            category: workflow.category,
            inputs: workflow.inputs.len(),
            outputs: workflow.outputs.len(),
            dependencies: workflow.dependencies.len(),
            models: workflow.models.len(),
            runtimes: workflow.runtimes.len(),
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

/// Recursive dependency report for one workflow.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkflowDependencyReport {
    pub workflow_id: String,
    pub complete: bool,
    pub workflows: Vec<String>,
    pub resolved: Vec<ResolvedWorkflowDependency>,
    pub workflow_order: Vec<String>,
    pub missing_workflows: Vec<String>,
    pub version_mismatches: Vec<WorkflowVersionMismatch>,
    pub cycles: Vec<Vec<String>>,
}

/// Runtime inputs and node toggles for one workflow execution.
#[derive(Debug, Clone, Default, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkflowExecutionOptions {
    #[serde(default)]
    pub inputs: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub disabled_nodes: Vec<String>,
    #[serde(default)]
    pub enabled_nodes: Vec<String>,
}

/// Execution result for one workflow run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowExecution {
    pub workflow_id: String,
    pub version: String,
    pub inputs: serde_json::Map<String, serde_json::Value>,
    pub outputs: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub artifacts: Vec<WorkflowArtifact>,
    pub nodes: Vec<NodeExecution>,
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
    pub status: NodeExecutionStatus,
    #[serde(default)]
    pub inputs: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub outputs: serde_json::Map<String, serde_json::Value>,
    #[serde(default)]
    pub artifacts: Vec<WorkflowArtifact>,
}

/// Runtime state of one node.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeExecutionStatus {
    Completed,
    Skipped,
}

/// One resolved local workflow dependency with the currently available version.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ResolvedWorkflowDependency {
    pub workflow_id: String,
    pub version: String,
}

/// A workflow exists but does not satisfy a declared version requirement.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkflowVersionMismatch {
    pub workflow_id: String,
    pub required: String,
    pub found: String,
    pub required_by: String,
}

/// Start a Rust workflow definition.
#[must_use]
pub fn workflow(id: impl Into<String>) -> WorkflowBuilder {
    WorkflowBuilder {
        spec: WorkflowSpec {
            id: id.into(),
            version: default_version(),
            name: String::new(),
            category: None,
            description: None,
            inputs: Vec::new(),
            outputs: Vec::new(),
            config_schema: serde_json::Value::Null,
            dependencies: Vec::new(),
            models: Vec::new(),
            runtimes: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
        },
    }
}
