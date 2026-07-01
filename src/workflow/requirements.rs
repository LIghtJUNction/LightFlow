use serde::{Deserialize, Serialize};

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
