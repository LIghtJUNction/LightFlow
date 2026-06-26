use super::{ApiResult, ExecutorInfo};
use crate::workflow::{
    ModelRequirement, PortSpec, RuntimeRequirement, WorkflowDependencyRequirement, WorkflowSpec,
};
use serde::Serialize;
use std::collections::{BTreeMap, BTreeSet};

mod model_locks;
pub use model_locks::{
    ModelCatalog, ModelListOptions, ModelLockFingerprint, ModelLockState, ModelLockStatus,
    ModelStatusFilter, NodeModelBinding, NodeModelCard, PortDirection,
};
pub(super) use model_locks::{model_catalog, model_lock_fingerprints};

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NodeCatalog {
    pub nodes: Vec<NodeCard>,
    pub categories: Vec<NodeCategory>,
    pub runtimes: Vec<NodeRuntimeSummary>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct NodeCategory {
    pub category: String,
    pub nodes: usize,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct NodeRuntimeSummary {
    pub capability: String,
    pub nodes: usize,
    pub available_executors: usize,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NodeCard {
    pub id: String,
    pub version: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub category: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub kind: NodeKind,
    pub inputs: Vec<PortSpec>,
    pub outputs: Vec<PortSpec>,
    pub dependencies: Vec<WorkflowDependencyRequirement>,
    pub models: Vec<ModelRequirement>,
    pub runtimes: Vec<NodeRuntimeStatus>,
    pub graph: NodeGraphSummary,
    pub validation: NodeValidationSummary,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    Leaf,
    Composite,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct NodeGraphSummary {
    pub nodes: usize,
    pub edges: usize,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct NodeValidationSummary {
    pub valid: bool,
    pub issues: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NodeRuntimeStatus {
    pub id: String,
    pub capability: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    pub available: bool,
    pub executors: Vec<NodeExecutorStatus>,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct NodeExecutorStatus {
    pub id: String,
    pub kind: String,
    pub status: String,
    pub status_reason: String,
    pub available: bool,
    pub data_policy: String,
    pub plans_models: bool,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

pub(super) fn node_catalog(
    workflows: &BTreeMap<String, WorkflowSpec>,
    executors: &[ExecutorInfo],
    validate: impl Fn(&WorkflowSpec) -> NodeValidationSummary,
) -> NodeCatalog {
    let nodes = workflows
        .values()
        .map(|workflow| node_card(workflow, executors, validate(workflow)))
        .collect::<Vec<_>>();
    let mut categories = BTreeMap::<String, usize>::new();
    let mut runtimes = BTreeMap::<String, RuntimeAccumulator>::new();
    for node in &nodes {
        let category = node
            .category
            .clone()
            .unwrap_or_else(|| "uncategorized".to_owned());
        *categories.entry(category).or_default() += 1;
        for runtime in &node.runtimes {
            let entry = runtimes.entry(runtime.capability.clone()).or_default();
            entry.nodes += 1;
            for executor in runtime
                .executors
                .iter()
                .filter(|executor| executor.available)
                .map(|executor| executor.id.clone())
            {
                entry.available_executors.insert(executor);
            }
        }
    }

    NodeCatalog {
        nodes,
        categories: categories
            .into_iter()
            .map(|(category, nodes)| NodeCategory { category, nodes })
            .collect(),
        runtimes: runtimes
            .into_iter()
            .map(|(capability, runtime)| NodeRuntimeSummary {
                capability,
                nodes: runtime.nodes,
                available_executors: runtime.available_executors.len(),
            })
            .collect(),
    }
}

pub(super) fn get_node_card(
    workflows: &BTreeMap<String, WorkflowSpec>,
    executors: &[ExecutorInfo],
    workflow_id: &str,
    validate: impl Fn(&WorkflowSpec) -> NodeValidationSummary,
) -> ApiResult<NodeCard> {
    let workflow = workflows
        .get(workflow_id)
        .ok_or_else(|| super::ApiError::NotFound(format!("node {workflow_id}")))?;
    Ok(node_card(workflow, executors, validate(workflow)))
}

fn node_card(
    workflow: &WorkflowSpec,
    executors: &[ExecutorInfo],
    validation: NodeValidationSummary,
) -> NodeCard {
    NodeCard {
        id: workflow.id.clone(),
        version: workflow.version.clone(),
        name: workflow.name.clone(),
        category: workflow.category.clone(),
        description: workflow.description.clone(),
        kind: if workflow.nodes.is_empty() {
            NodeKind::Leaf
        } else {
            NodeKind::Composite
        },
        inputs: workflow.inputs.clone(),
        outputs: workflow.outputs.clone(),
        dependencies: workflow.dependencies.clone(),
        models: workflow.models.clone(),
        runtimes: workflow
            .runtimes
            .iter()
            .map(|runtime| runtime_status(runtime, executors))
            .collect(),
        graph: NodeGraphSummary {
            nodes: workflow.nodes.len(),
            edges: workflow.edges.len(),
        },
        validation,
    }
}

fn runtime_status(runtime: &RuntimeRequirement, executors: &[ExecutorInfo]) -> NodeRuntimeStatus {
    let matches = executors
        .iter()
        .filter(|executor| {
            executor
                .capabilities
                .iter()
                .any(|capability| capability == &runtime.capability)
        })
        .map(|executor| NodeExecutorStatus {
            id: executor.id.to_owned(),
            kind: executor.kind.to_owned(),
            status: executor.status.to_owned(),
            status_reason: executor.status_reason.clone(),
            available: executor.available,
            data_policy: executor.data_policy.to_owned(),
            plans_models: executor.plans_models,
            features: executor
                .features
                .iter()
                .map(|feature| (*feature).to_owned())
                .collect(),
            env: executor.env.map(str::to_owned),
            command: executor.command.clone(),
        })
        .collect::<Vec<_>>();
    NodeRuntimeStatus {
        id: runtime.id.clone(),
        capability: runtime.capability.clone(),
        engine: runtime.engine.clone(),
        available: matches.iter().any(|executor| executor.available),
        executors: matches,
    }
}

#[derive(Default)]
struct RuntimeAccumulator {
    nodes: usize,
    available_executors: BTreeSet<String>,
}
