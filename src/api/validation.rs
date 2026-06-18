use super::deps::is_supported_version_requirement;
use super::util::{
    node_inputs, node_outputs, push_duplicate_port_issues, push_id_issue, referenced_workflow_ids,
};
use super::{ApiError, ApiResult};
use crate::workflow::{WorkflowCondition, WorkflowNodeKind, WorkflowSpec, WorkflowValidation};
use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};
use semver::Version;
use std::collections::BTreeMap;

pub(super) fn validate_workflow_shape(workflow: &WorkflowSpec) -> ApiResult<()> {
    let mut issues = Vec::new();
    push_id_issue(&mut issues, &workflow.id, "workflow id");
    if workflow.version.trim().is_empty() {
        issues.push(format!("workflow {} must have a version", workflow.id));
    } else if Version::parse(&workflow.version).is_err() {
        issues.push(format!(
            "workflow {} version {} must be semantic version",
            workflow.id, workflow.version
        ));
    }
    if workflow.name.trim().is_empty() {
        issues.push(format!("workflow {} must have a name", workflow.id));
    }
    if let Some(category) = &workflow.category {
        push_id_issue(&mut issues, category, "workflow category");
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
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ApiError::InvalidRequest(issues.join("; ")))
    }
}

pub(super) fn validate_workflow_spec(
    workflow: &WorkflowSpec,
    workflows: &BTreeMap<String, WorkflowSpec>,
) -> WorkflowValidation {
    let mut issues = match validate_workflow_shape(workflow) {
        Ok(()) => Vec::new(),
        Err(ApiError::InvalidRequest(message)) => vec![message],
        Err(error) => vec![error.to_string()],
    };

    for dependency in &workflow.dependencies {
        if let Some(version) = &dependency.version
            && !is_supported_version_requirement(version)
        {
            issues.push(format!(
                "workflow {} declares unsupported version requirement {} for {}",
                workflow.id, version, dependency.workflow_id
            ));
        }
        if !workflows.contains_key(&dependency.workflow_id) {
            issues.push(format!(
                "workflow {} declares missing dependency {}",
                workflow.id, dependency.workflow_id
            ));
        }
    }

    let mut nodes = BTreeMap::new();
    for node in &workflow.nodes {
        push_id_issue(&mut issues, &node.id, "node id");
        if nodes.insert(node.id.as_str(), node).is_some() {
            issues.push(format!("duplicate node id {}", node.id));
        }
        match node.kind {
            WorkflowNodeKind::Workflow => {
                if node.workflow_id.trim().is_empty() {
                    issues.push(format!("node {} must reference a workflow", node.id));
                }
            }
            WorkflowNodeKind::If => {
                if !matches!(node.condition, Some(WorkflowCondition::InputEquals { .. })) {
                    issues.push(format!("if node {} must declare a condition", node.id));
                }
                if node
                    .then_workflow_id
                    .as_deref()
                    .unwrap_or("")
                    .trim()
                    .is_empty()
                {
                    issues.push(format!("if node {} must declare then workflow", node.id));
                }
                if node
                    .else_workflow_id
                    .as_deref()
                    .unwrap_or("")
                    .trim()
                    .is_empty()
                {
                    issues.push(format!("if node {} must declare else workflow", node.id));
                }
            }
        }
        for referenced in referenced_workflow_ids(node) {
            if referenced == workflow.id {
                issues.push(format!(
                    "workflow {} cannot directly nest itself",
                    workflow.id
                ));
            } else if !workflows.contains_key(referenced) {
                issues.push(format!(
                    "node {} references missing workflow {}",
                    node.id, referenced
                ));
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
        if !node_outputs(from_node, workflows)
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
        if !node_inputs(to_node, workflows)
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
