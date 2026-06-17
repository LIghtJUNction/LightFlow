use serde::{Deserialize, Serialize};

/// A reusable executable unit in LightFlow.
///
/// Components are the only leaf-level building block. They may wrap model
/// calls, ordinary Rust functions, external commands, HTTP calls, or any future
/// runtime adapter, but those adapters are not separate workflow concepts.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentSpec {
    pub id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default)]
    pub inputs: Vec<PortSpec>,
    #[serde(default)]
    pub outputs: Vec<PortSpec>,
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub config_schema: serde_json::Value,
}

/// A named typed input or output.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct PortSpec {
    pub name: String,
    #[serde(rename = "type")]
    pub ty: String,
}

/// List response for component endpoints.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentList {
    pub components: Vec<ComponentSummary>,
}

/// Compact component row for browsers.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct ComponentSummary {
    pub id: String,
    pub name: String,
    pub inputs: usize,
    pub outputs: usize,
}

impl From<ComponentSpec> for ComponentSummary {
    fn from(component: ComponentSpec) -> Self {
        Self {
            id: component.id,
            name: component.name,
            inputs: component.inputs.len(),
            outputs: component.outputs.len(),
        }
    }
}
