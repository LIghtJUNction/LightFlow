//! Framework-independent LightFlow backend service.

use crate::component::{ComponentList, ComponentSpec, ComponentSummary, PortSpec};
use crate::workflow::{
    WorkflowList, WorkflowNodeTarget, WorkflowSpec, WorkflowSummary, WorkflowValidation,
};
use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

const COMPONENT_DIR: &str = "components";
const WORKFLOW_DIR: &str = "workflows";
const LIGHTFLOW_DIR: &str = "lightflow";

/// Backend service state independent of any web framework.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ApiService {
    repo_root: PathBuf,
}

impl ApiService {
    /// Create a service rooted at a LightFlow repository.
    #[must_use]
    pub fn new(repo_root: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
        }
    }

    /// Repository root used for project file discovery.
    #[must_use]
    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    /// List component specs.
    pub fn list_components(&self) -> ApiResult<ComponentList> {
        let components = self
            .component_specs()?
            .into_values()
            .map(ComponentSummary::from)
            .collect();
        Ok(ComponentList { components })
    }

    /// Read one component spec.
    pub fn get_component(&self, component_id: &str) -> ApiResult<ComponentSpec> {
        self.component_specs()?
            .remove(component_id)
            .ok_or_else(|| ApiError::NotFound(format!("component {component_id}")))
    }

    /// Save one component spec under `lightflow/components/<id>.json`.
    pub fn save_component(&self, component: ComponentSpec) -> ApiResult<ComponentSpec> {
        validate_component(&component)?;
        let path = self.component_path(&component.id)?;
        write_json_atomic(&path, &component)?;
        Ok(component)
    }

    /// List workflow specs.
    pub fn list_workflows(&self) -> ApiResult<WorkflowList> {
        let workflows = self
            .workflow_specs()?
            .into_values()
            .map(WorkflowSummary::from)
            .collect();
        Ok(WorkflowList { workflows })
    }

    /// Read one workflow spec.
    pub fn get_workflow(&self, workflow_id: &str) -> ApiResult<WorkflowSpec> {
        self.workflow_specs()?
            .remove(workflow_id)
            .ok_or_else(|| ApiError::NotFound(format!("workflow {workflow_id}")))
    }

    /// Save one workflow spec under `lightflow/workflows/<id>.json`.
    pub fn save_workflow(&self, workflow: WorkflowSpec) -> ApiResult<WorkflowSpec> {
        let validation = self.validate_workflow(&workflow);
        if !validation.valid {
            return Err(ApiError::InvalidRequest(validation.issues.join("; ")));
        }
        let path = self.workflow_path(&workflow.id)?;
        write_json_atomic(&path, &workflow)?;
        Ok(workflow)
    }

    /// Validate a workflow against current component and workflow specs.
    pub fn validate_workflow(&self, workflow: &WorkflowSpec) -> WorkflowValidation {
        let components = self.component_specs().unwrap_or_default();
        let workflows = self.workflow_specs().unwrap_or_default();
        validate_workflow_spec(workflow, &components, &workflows)
    }

    fn component_specs(&self) -> ApiResult<BTreeMap<String, ComponentSpec>> {
        let mut components = builtin_components()
            .into_iter()
            .map(|component| (component.id.clone(), component))
            .collect::<BTreeMap<_, _>>();
        for component in read_specs::<ComponentSpec>(&self.repo_root, COMPONENT_DIR)? {
            validate_component(&component)?;
            components.insert(component.id.clone(), component);
        }
        Ok(components)
    }

    fn workflow_specs(&self) -> ApiResult<BTreeMap<String, WorkflowSpec>> {
        let mut workflows = builtin_workflows()
            .into_iter()
            .map(|workflow| (workflow.id.clone(), workflow))
            .collect::<BTreeMap<_, _>>();
        for workflow in read_specs::<WorkflowSpec>(&self.repo_root, WORKFLOW_DIR)? {
            workflows.insert(workflow.id.clone(), workflow);
        }
        Ok(workflows)
    }

    fn component_path(&self, component_id: &str) -> ApiResult<PathBuf> {
        validate_id_segment(component_id, "component id")?;
        Ok(self
            .repo_root
            .join(LIGHTFLOW_DIR)
            .join(COMPONENT_DIR)
            .join(format!("{component_id}.json")))
    }

    fn workflow_path(&self, workflow_id: &str) -> ApiResult<PathBuf> {
        validate_id_segment(workflow_id, "workflow id")?;
        Ok(self
            .repo_root
            .join(LIGHTFLOW_DIR)
            .join(WORKFLOW_DIR)
            .join(format!("{workflow_id}.json")))
    }
}

/// API-level error.
#[derive(Debug)]
pub enum ApiError {
    InvalidRequest(String),
    NotFound(String),
    Io(io::Error),
}

impl ApiError {
    /// HTTP-style status code for adapters.
    #[must_use]
    pub const fn status_code(&self) -> u16 {
        match self {
            Self::InvalidRequest(_) => 400,
            Self::NotFound(_) => 404,
            Self::Io(_) => 500,
        }
    }
}

impl Display for ApiError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest(message) => write!(f, "invalid request: {message}"),
            Self::NotFound(message) => write!(f, "not found: {message}"),
            Self::Io(error) => Display::fmt(error, f),
        }
    }
}

impl std::error::Error for ApiError {}

impl From<io::Error> for ApiError {
    fn from(error: io::Error) -> Self {
        if error.kind() == io::ErrorKind::NotFound {
            Self::NotFound(error.to_string())
        } else {
            Self::Io(error)
        }
    }
}

/// Service result.
pub type ApiResult<T> = Result<T, ApiError>;

fn read_specs<T: DeserializeOwned>(root: &Path, dir: &str) -> ApiResult<Vec<T>> {
    let mut specs = Vec::new();
    match fs::read_dir(root.join(LIGHTFLOW_DIR).join(dir)) {
        Ok(entries) => {
            for entry in entries {
                let path = entry.map_err(ApiError::from)?.path();
                if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                    continue;
                }
                specs.push(read_json(&path)?);
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(ApiError::from(error)),
    }
    Ok(specs)
}

fn validate_component(component: &ComponentSpec) -> ApiResult<()> {
    let mut issues = Vec::new();
    push_id_issue(&mut issues, &component.id, "component id");
    if component.name.trim().is_empty() {
        issues.push(format!("component {} must have a name", component.id));
    }
    push_duplicate_port_issues(&mut issues, "input", &component.id, &component.inputs);
    push_duplicate_port_issues(&mut issues, "output", &component.id, &component.outputs);
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ApiError::InvalidRequest(issues.join("; ")))
    }
}

fn validate_workflow_spec(
    workflow: &WorkflowSpec,
    components: &BTreeMap<String, ComponentSpec>,
    workflows: &BTreeMap<String, WorkflowSpec>,
) -> WorkflowValidation {
    let mut issues = Vec::new();
    push_id_issue(&mut issues, &workflow.id, "workflow id");
    if workflow.name.trim().is_empty() {
        issues.push(format!("workflow {} must have a name", workflow.id));
    }
    push_duplicate_port_issues(
        &mut issues,
        "workflow input",
        &workflow.id,
        &workflow.inputs,
    );
    push_duplicate_port_issues(
        &mut issues,
        "workflow output",
        &workflow.id,
        &workflow.outputs,
    );

    let mut nodes = BTreeMap::new();
    for node in &workflow.nodes {
        push_id_issue(&mut issues, &node.id, "node id");
        if nodes.insert(node.id.as_str(), node).is_some() {
            issues.push(format!("duplicate node id {}", node.id));
        }
        match &node.uses {
            WorkflowNodeTarget::Component { component_id } => {
                if !components.contains_key(component_id) {
                    issues.push(format!(
                        "node {} references missing component {}",
                        node.id, component_id
                    ));
                }
            }
            WorkflowNodeTarget::Workflow { workflow_id } => {
                if workflow_id == &workflow.id {
                    issues.push(format!(
                        "workflow {} cannot directly nest itself",
                        workflow.id
                    ));
                } else if !workflows.contains_key(workflow_id) {
                    issues.push(format!(
                        "node {} references missing workflow {}",
                        node.id, workflow_id
                    ));
                }
            }
        }
    }

    let mut graph = DiGraph::<&str, ()>::new();
    let mut graph_nodes = BTreeMap::<&str, NodeIndex>::new();
    for node in &workflow.nodes {
        graph_nodes
            .entry(node.id.as_str())
            .or_insert_with(|| graph.add_node(node.id.as_str()));
    }

    for edge in &workflow.edges {
        let Some(from_node) = nodes.get(edge.from.node.as_str()) else {
            issues.push(format!(
                "edge references missing source node {}",
                edge.from.node
            ));
            continue;
        };
        if !node_outputs(from_node, components, workflows)
            .iter()
            .any(|port| port.name == edge.from.port)
        {
            issues.push(format!(
                "edge source {}.{} is not an output port",
                edge.from.node, edge.from.port
            ));
        }
        let Some(to_node) = nodes.get(edge.to.node.as_str()) else {
            issues.push(format!(
                "edge references missing target node {}",
                edge.to.node
            ));
            continue;
        };
        if !node_inputs(to_node, components, workflows)
            .iter()
            .any(|port| port.name == edge.to.port)
        {
            issues.push(format!(
                "edge target {}.{} is not an input port",
                edge.to.node, edge.to.port
            ));
        }
        if let (Some(from), Some(to)) = (
            graph_nodes.get(edge.from.node.as_str()),
            graph_nodes.get(edge.to.node.as_str()),
        ) {
            graph.add_edge(*from, *to, ());
        }
    }

    let topological_order = match toposort(&graph, None) {
        Ok(order) => order
            .into_iter()
            .filter_map(|node| graph.node_weight(node).copied())
            .map(ToOwned::to_owned)
            .collect(),
        Err(cycle) => {
            let node = graph
                .node_weight(cycle.node_id())
                .copied()
                .unwrap_or("unknown");
            issues.push(format!(
                "workflow {} contains a cycle involving node {node}",
                workflow.id
            ));
            Vec::new()
        }
    };

    WorkflowValidation {
        valid: issues.is_empty(),
        issues,
        topological_order,
    }
}

fn node_inputs(
    node: &crate::workflow::WorkflowNode,
    components: &BTreeMap<String, ComponentSpec>,
    workflows: &BTreeMap<String, WorkflowSpec>,
) -> Vec<PortSpec> {
    match &node.uses {
        WorkflowNodeTarget::Component { component_id } => components
            .get(component_id)
            .map(|component| component.inputs.clone())
            .unwrap_or_default(),
        WorkflowNodeTarget::Workflow { workflow_id } => workflows
            .get(workflow_id)
            .map(|workflow| workflow.inputs.clone())
            .unwrap_or_default(),
    }
}

fn node_outputs(
    node: &crate::workflow::WorkflowNode,
    components: &BTreeMap<String, ComponentSpec>,
    workflows: &BTreeMap<String, WorkflowSpec>,
) -> Vec<PortSpec> {
    match &node.uses {
        WorkflowNodeTarget::Component { component_id } => components
            .get(component_id)
            .map(|component| component.outputs.clone())
            .unwrap_or_default(),
        WorkflowNodeTarget::Workflow { workflow_id } => workflows
            .get(workflow_id)
            .map(|workflow| workflow.outputs.clone())
            .unwrap_or_default(),
    }
}

fn push_id_issue(issues: &mut Vec<String>, value: &str, label: &str) {
    if let Err(error) = validate_id_segment(value, label) {
        issues.push(error.to_string());
    }
}

fn push_duplicate_port_issues(
    issues: &mut Vec<String>,
    direction: &str,
    owner_id: &str,
    ports: &[PortSpec],
) {
    let mut names = BTreeSet::new();
    for port in ports {
        if port.name.trim().is_empty() {
            issues.push(format!("{owner_id} has an empty {direction} port name"));
        }
        if port.ty.trim().is_empty() {
            issues.push(format!("{owner_id} port {} has an empty type", port.name));
        }
        if !names.insert(port.name.as_str()) {
            issues.push(format!(
                "{owner_id} has duplicate {direction} port {}",
                port.name
            ));
        }
    }
}

fn validate_id_segment(value: &str, label: &str) -> ApiResult<()> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
    {
        return Err(ApiError::InvalidRequest(format!(
            "invalid {label} path segment: {value}"
        )));
    }
    Ok(())
}

fn read_json<T: DeserializeOwned>(path: &Path) -> ApiResult<T> {
    let file = fs::File::open(path).map_err(ApiError::from)?;
    serde_json::from_reader(file)
        .map_err(|error| ApiError::InvalidRequest(format!("invalid JSON in {:?}: {error}", path)))
}

fn write_json_atomic(path: &Path, value: &impl Serialize) -> ApiResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| ApiError::InvalidRequest("json path has no parent".to_owned()))?;
    fs::create_dir_all(parent).map_err(ApiError::from)?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ApiError::InvalidRequest("json path has no file name".to_owned()))?;
    let temp_path = parent.join(format!("{file_name}.tmp"));
    let mut file = fs::File::create(&temp_path).map_err(ApiError::from)?;
    serde_json::to_writer_pretty(&mut file, value)
        .map_err(|error| ApiError::InvalidRequest(format!("failed to encode JSON: {error}")))?;
    file.write_all(b"\n").map_err(ApiError::from)?;
    file.sync_all().map_err(ApiError::from)?;
    drop(file);
    fs::rename(temp_path, path).map_err(ApiError::from)
}

fn builtin_components() -> Vec<ComponentSpec> {
    vec![
        ComponentSpec {
            id: "component.input".to_owned(),
            name: "Workflow Input".to_owned(),
            description: Some("Passes external workflow input into the graph.".to_owned()),
            inputs: Vec::new(),
            outputs: vec![PortSpec {
                name: "value".to_owned(),
                ty: "json".to_owned(),
            }],
            config_schema: serde_json::Value::Null,
        },
        ComponentSpec {
            id: "component.output".to_owned(),
            name: "Workflow Output".to_owned(),
            description: Some("Collects graph output.".to_owned()),
            inputs: vec![PortSpec {
                name: "value".to_owned(),
                ty: "json".to_owned(),
            }],
            outputs: Vec::new(),
            config_schema: serde_json::Value::Null,
        },
    ]
}

fn builtin_workflows() -> Vec<WorkflowSpec> {
    Vec::new()
}
