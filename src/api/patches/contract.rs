use crate::api::util::{node_inputs, node_outputs};
use crate::workflow::{PortSpec, WorkflowNode, WorkflowSpec};
use std::collections::{BTreeMap, BTreeSet};

pub(super) fn validate_candidate_contract(
    node_id: &str,
    role: &str,
    candidate_id: &str,
    node: &WorkflowNode,
    workflows: &BTreeMap<String, WorkflowSpec>,
    issues: &mut Vec<String>,
) {
    let Some(candidate) = workflows.get(candidate_id) else {
        return;
    };
    let original_inputs = node_inputs(node, workflows);
    push_missing_ports(
        node_id,
        role,
        candidate_id,
        "input",
        &original_inputs,
        &candidate.inputs,
        issues,
    );
    push_unsatisfied_extra_required_inputs(
        node_id,
        role,
        candidate_id,
        &original_inputs,
        &candidate.inputs,
        issues,
    );
    push_missing_ports(
        node_id,
        role,
        candidate_id,
        "output",
        &node_outputs(node, workflows),
        &candidate.outputs,
        issues,
    );
}

fn push_missing_ports(
    node_id: &str,
    role: &str,
    candidate_id: &str,
    direction: &str,
    required: &[PortSpec],
    available: &[PortSpec],
    issues: &mut Vec<String>,
) {
    for port in required {
        match available.iter().find(|candidate| candidate.name == port.name) {
            Some(candidate) if candidate.ty == port.ty => {}
            Some(candidate) => issues.push(format!(
                "patch node {node_id} {role} workflow {candidate_id} {direction} port {} has type {}, expected {}",
                port.name, candidate.ty, port.ty
            )),
            None => issues.push(format!(
                "patch node {node_id} {role} workflow {candidate_id} is missing {direction} port {}",
                port.name
            )),
        }
    }
}

fn push_unsatisfied_extra_required_inputs(
    node_id: &str,
    role: &str,
    candidate_id: &str,
    original_inputs: &[PortSpec],
    candidate_inputs: &[PortSpec],
    issues: &mut Vec<String>,
) {
    let original_names = original_inputs
        .iter()
        .map(|port| port.name.as_str())
        .collect::<BTreeSet<_>>();
    for port in candidate_inputs {
        if original_names.contains(port.name.as_str()) {
            continue;
        }
        if port.required == Some(true) && port.default.is_none() {
            issues.push(format!(
                "patch node {node_id} {role} workflow {candidate_id} has unsatisfied required input port {}",
                port.name
            ));
        }
    }
}
