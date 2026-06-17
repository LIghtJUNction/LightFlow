use serde::{Deserialize, Serialize};

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
    pub nodes: Vec<WorkflowNode>,
    #[serde(default)]
    pub edges: Vec<WorkflowEdge>,
}

/// A named typed input or output.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
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

/// One node in a composite workflow graph.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WorkflowNode {
    pub id: String,
    pub workflow_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub title: Option<String>,
    #[serde(default)]
    pub position: WorkflowPosition,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub config: serde_json::Value,
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
    pub inputs: usize,
    pub outputs: usize,
    pub dependencies: usize,
    pub models: usize,
    pub nodes: usize,
    pub edges: usize,
}

impl From<WorkflowSpec> for WorkflowSummary {
    fn from(workflow: WorkflowSpec) -> Self {
        Self {
            id: workflow.id,
            version: workflow.version,
            name: workflow.name,
            inputs: workflow.inputs.len(),
            outputs: workflow.outputs.len(),
            dependencies: workflow.dependencies.len(),
            models: workflow.models.len(),
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

/// Start a Rust workflow definition.
#[must_use]
pub fn workflow(id: impl Into<String>) -> WorkflowBuilder {
    WorkflowBuilder {
        spec: WorkflowSpec {
            id: id.into(),
            version: default_version(),
            name: String::new(),
            description: None,
            inputs: Vec::new(),
            outputs: Vec::new(),
            config_schema: serde_json::Value::Null,
            dependencies: Vec::new(),
            models: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
        },
    }
}

/// Builder used by source-controlled Rust workflow files.
#[derive(Debug, Clone)]
pub struct WorkflowBuilder {
    spec: WorkflowSpec,
}

impl WorkflowBuilder {
    #[must_use]
    pub fn version(mut self, version: impl Into<String>) -> Self {
        self.spec.version = version.into();
        self
    }

    #[must_use]
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.spec.name = name.into();
        self
    }

    #[must_use]
    pub fn description(mut self, description: impl Into<String>) -> Self {
        self.spec.description = Some(description.into());
        self
    }

    #[must_use]
    pub fn input(mut self, name: impl Into<String>, ty: impl Into<String>) -> Self {
        self.spec.inputs.push(PortSpec {
            name: name.into(),
            ty: ty.into(),
        });
        self
    }

    #[must_use]
    pub fn output(mut self, name: impl Into<String>, ty: impl Into<String>) -> Self {
        self.spec.outputs.push(PortSpec {
            name: name.into(),
            ty: ty.into(),
        });
        self
    }

    #[must_use]
    pub fn depends_on(
        mut self,
        workflow_id: impl Into<String>,
        version: impl Into<String>,
    ) -> Self {
        self.spec.dependencies.push(WorkflowDependencyRequirement {
            workflow_id: workflow_id.into(),
            version: Some(version.into()),
        });
        self
    }

    #[must_use]
    pub fn model(mut self, id: impl Into<String>, capability: impl Into<String>) -> Self {
        self.spec.models.push(ModelRequirement {
            id: id.into(),
            capability: capability.into(),
            variants: Vec::new(),
        });
        self
    }

    #[must_use]
    pub fn hf_model(
        mut self,
        requirement_id: impl Into<String>,
        variant_id: impl Into<String>,
        capability: impl Into<String>,
        format: impl Into<String>,
        repo: impl Into<String>,
        file: impl Into<String>,
    ) -> Self {
        let requirement_id = requirement_id.into();
        let capability = capability.into();
        let variant = ModelVariant {
            id: variant_id.into(),
            provider: ModelProvider::HuggingFace,
            format: format.into(),
            repo: repo.into(),
            file: Some(file.into()).filter(|file| !file.is_empty()),
        };

        if let Some(requirement) = self
            .spec
            .models
            .iter_mut()
            .find(|requirement| requirement.id == requirement_id)
        {
            requirement.variants.push(variant);
        } else {
            self.spec.models.push(ModelRequirement {
                id: requirement_id,
                capability,
                variants: vec![variant],
            });
        }
        self
    }

    #[must_use]
    pub fn node(mut self, id: impl Into<String>, workflow_id: impl Into<String>) -> Self {
        self.spec.nodes.push(WorkflowNode {
            id: id.into(),
            workflow_id: workflow_id.into(),
            title: None,
            position: WorkflowPosition::default(),
            config: serde_json::Value::Null,
        });
        self
    }

    #[must_use]
    pub fn edge(
        mut self,
        from_node: impl Into<String>,
        from_port: impl Into<String>,
        to_node: impl Into<String>,
        to_port: impl Into<String>,
    ) -> Self {
        self.spec.edges.push(WorkflowEdge {
            from: WorkflowEndpoint {
                node: from_node.into(),
                port: from_port.into(),
            },
            to: WorkflowEndpoint {
                node: to_node.into(),
                port: to_port.into(),
            },
        });
        self
    }

    #[must_use]
    pub fn build(self) -> WorkflowSpec {
        self.spec
    }
}

impl From<WorkflowBuilder> for WorkflowSpec {
    fn from(builder: WorkflowBuilder) -> Self {
        builder.spec
    }
}
