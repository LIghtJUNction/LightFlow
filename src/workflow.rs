use serde::{Deserialize, Serialize};

mod builder;
mod execution;
mod patch_types;
mod port_macro;
mod program;
mod requirements;
pub use builder::WorkflowBuilder;
pub use execution::{
    ExecutionRuntime, NodeExecution, NodeExecutionStatus, WorkflowArtifact, WorkflowExecution,
};
pub use patch_types::{WorkflowExecutionOptions, WorkflowNodePatch, WorkflowPatch};
pub use program::{ContextWorkflow, Runnable, Workflow, WorkflowState};
pub use requirements::{
    CargoDependency, CargoDependencySource, ModelProvider, ModelRequirement, ModelVariant,
    RuntimeRequirement, WorkflowDependencyRequirement,
};
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub default: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub step: Option<f64>,
    #[serde(default, rename = "enum", skip_serializing_if = "Vec::is_empty")]
    pub enum_values: Vec<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub widget: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact_kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model_requirement: Option<String>,
}

impl PortSpec {
    #[must_use]
    pub fn new(name: impl Into<String>, ty: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ty: ty.into(),
            description: None,
            required: None,
            default: None,
            min: None,
            max: None,
            step: None,
            enum_values: Vec::new(),
            widget: None,
            artifact_kind: None,
            model_requirement: None,
        }
    }
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

/// Converts a Cargo package name into its canonical LightFlow workflow id.
#[must_use]
pub fn workflow_id_from_package_name(package_name: &str) -> String {
    let suffix = package_name
        .strip_prefix("lightflow-")
        .unwrap_or(package_name)
        .replace('-', "_");
    format!("lightflow.{suffix}")
}

/// Builds a workflow definition from the calling crate's Cargo identity.
#[doc(hidden)]
#[must_use]
pub fn workflow_from_package(package_name: &str, package_version: &str) -> WorkflowBuilder {
    workflow_with_identity(workflow_id_from_package_name(package_name), package_version)
}

/// Builds a workflow with an explicit identity for internal tests and synthetic fixtures.
#[doc(hidden)]
#[must_use]
pub(crate) fn workflow_with_identity(
    id: impl Into<String>,
    version: impl Into<String>,
) -> WorkflowBuilder {
    WorkflowBuilder {
        spec: WorkflowSpec {
            id: id.into(),
            version: version.into(),
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

#[doc(hidden)]
pub const fn assert_workflow_json_literal(source: &str) {
    let bytes = source.as_bytes();
    assert!(!bytes.is_empty(), "JSON literal cannot be empty");
    if bytes[0] == b'"' {
        assert!(
            bytes.len() >= 2 && bytes[bytes.len() - 1] == b'"',
            "JSON strings must use ordinary quoted string literals"
        );
        return;
    }
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        assert!(
            byte.is_ascii_digit()
                || byte == b'-'
                || byte == b'+'
                || byte == b'.'
                || byte == b'e'
                || byte == b'E',
            "JSON numbers cannot use Rust suffixes or non-JSON tokens"
        );
        index += 1;
    }
}

#[doc(hidden)]
pub const fn assert_workflow_json_string_literal(source: &str) {
    let bytes = source.as_bytes();
    assert!(
        bytes.len() >= 2 && bytes[0] == b'"' && bytes[bytes.len() - 1] == b'"',
        "JSON object keys must use ordinary quoted string literals"
    );
}

#[doc(hidden)]
#[must_use]
pub fn parse_workflow_json_literal(source: &str) -> serde_json::Value {
    serde_json::from_str(source).expect("validated workflow JSON literal")
}

#[doc(hidden)]
#[must_use]
pub fn parse_workflow_json_string_literal(source: &str) -> String {
    serde_json::from_str(source).expect("validated workflow JSON string literal")
}

/// Starts a workflow definition using the calling Cargo package name and version.
///
/// Port metadata is declared with the port. Output-only metadata is deliberately
/// restricted to descriptions, artifact kinds, and model bindings.
///
/// ```compile_fail
/// use lightflow::preload::*;
///
/// let _ = workflow! {
///     output "value": "json" { required: true }
/// };
/// ```
///
/// ```compile_fail
/// use lightflow::preload::*;
/// let _ = workflow! { input "value": "json" { required: bool::default() } };
/// ```
///
/// ```compile_fail
/// use lightflow::preload::*;
/// let _ = workflow! {
///     input "value": "json" { description: "first", description: "second" }
/// };
/// ```
///
/// ```compile_fail
/// use lightflow::preload::*;
/// fn minimum() -> f64 { 0.0 }
/// let _ = workflow! { input "value": "number" { range: [minimum(), 1.0, 0.1] } };
/// ```
///
/// ```compile_fail
/// use lightflow::preload::*;
/// let _ = workflow! {
///     input "value": "json" { default: {"items": [bool::default()]} }
/// };
/// ```
///
/// ```compile_fail
/// use lightflow::preload::*;
/// fn make_value() -> i32 { 1 }
/// let _ = workflow! { input "value": "json" { choices: [1, make_value()] } };
/// ```
///
/// ```compile_fail
/// use lightflow::preload::*;
/// let _ = workflow! { input "value": "json" { default: {enabled: true} } };
/// ```
///
/// ```compile_fail
/// use lightflow::preload::*;
/// let _ = workflow! { input "value": "json" { default: 1 + 2 } };
/// ```
///
/// ```compile_fail
/// use lightflow::preload::*;
/// let _ = workflow! { input "value": "json" { default: 1u32 } };
/// ```
#[macro_export]
macro_rules! workflow {
    () => {
        $crate::workflow::workflow_from_package(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"))
    };
    (@ports $builder:ident;) => {
        $builder
    };
    (@ports $builder:ident; input $name:literal : $ty:literal { $($metadata:tt)* } $($rest:tt)*) => {{
        let __lightflow_builder = $builder.input($name, $ty);
        let __lightflow_builder = $crate::__lightflow_input_metadata!(
            __lightflow_builder, $name; [no no no no no no no no]; $($metadata)* ,);
        $crate::workflow!(@ports __lightflow_builder; $($rest)*)
    }};
    (@ports $builder:ident; input $name:literal : $ty:literal $($rest:tt)*) => {{
        let __lightflow_builder = $builder.input($name, $ty);
        $crate::workflow!(@ports __lightflow_builder; $($rest)*)
    }};
    (@ports $builder:ident; output $name:literal : $ty:literal { $($metadata:tt)* } $($rest:tt)*) => {{
        let __lightflow_builder = $builder.output($name, $ty);
        let __lightflow_builder = $crate::__lightflow_output_metadata!(
            __lightflow_builder, $name; [no no no]; $($metadata)* ,);
        $crate::workflow!(@ports __lightflow_builder; $($rest)*)
    }};
    (@ports $builder:ident; output $name:literal : $ty:literal $($rest:tt)*) => {{
        let __lightflow_builder = $builder.output($name, $ty);
        $crate::workflow!(@ports __lightflow_builder; $($rest)*)
    }};

    ($($ports:tt)+) => {{
        let __lightflow_builder =
            $crate::workflow::workflow_from_package(env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION"));
        $crate::workflow!(@ports __lightflow_builder; $($ports)+)
    }};
}
