use super::leaf::execute_leaf_workflow;
use super::media::{collect_node_inputs, collect_workflow_outputs};
use super::types::{ChildExecution, ChildWorkflowRun, LeafExecution};
use crate::api::model_manager::ModelManager;
use crate::api::port_values;
use crate::api::source::WorkflowOrigin;
use crate::api::validation;
use crate::api::{ApiError, ApiResult};
use crate::workflow::{
    NodeExecution, NodeExecutionStatus, WorkflowCondition, WorkflowExecution,
    WorkflowExecutionOptions, WorkflowNode, WorkflowNodeKind, WorkflowNodePatch, WorkflowSpec,
};
use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;
use std::time::Instant;

pub(super) fn execute_workflow_spec(
    root: &Path,
    workflow: &WorkflowSpec,
    workflows: &BTreeMap<String, WorkflowSpec>,
    origins: &BTreeMap<String, WorkflowOrigin>,
    options: WorkflowExecutionOptions,
) -> ApiResult<WorkflowExecution> {
    let mut model_manager = ModelManager::new(root);
    execute_workflow_spec_with_models(
        root,
        workflow,
        workflows,
        origins,
        options,
        &mut model_manager,
    )
}

fn execute_workflow_spec_with_models(
    root: &Path,
    workflow: &WorkflowSpec,
    workflows: &BTreeMap<String, WorkflowSpec>,
    origins: &BTreeMap<String, WorkflowOrigin>,
    options: WorkflowExecutionOptions,
    model_manager: &mut ModelManager,
) -> ApiResult<WorkflowExecution> {
    let validation = validation::validate_workflow_spec(workflow, workflows);
    if !validation.valid {
        return Err(ApiError::InvalidRequest(validation.issues.join("; ")));
    }

    if workflow.nodes.is_empty() {
        let leaf = execute_leaf_workflow(
            root,
            workflow,
            origins.get(&workflow.id),
            &options.inputs,
            model_manager,
        )?;
        validate_values(&workflow.outputs, &leaf.outputs, "output", workflow)?;
        return Ok(WorkflowExecution {
            workflow_id: workflow.id.clone(),
            version: workflow.version.clone(),
            inputs: options.inputs,
            outputs: leaf.outputs,
            runtime: leaf.runtime,
            artifacts: leaf.artifacts,
            nodes: Vec::new(),
        });
    }

    let disabled_nodes = options.disabled_nodes.into_iter().collect::<BTreeSet<_>>();
    let enabled_nodes = options.enabled_nodes.into_iter().collect::<BTreeSet<_>>();
    let patch = options.patch.unwrap_or_default();
    let nodes_by_id = workflow
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<BTreeMap<_, _>>();

    let mut node_outputs = BTreeMap::<String, serde_json::Map<String, serde_json::Value>>::new();
    let mut artifacts = Vec::new();
    let mut executions = Vec::new();

    for node_id in validation.topological_order {
        let Some(node) = nodes_by_id.get(node_id.as_str()) else {
            continue;
        };

        let node_started_at = Instant::now();
        let node_inputs =
            collect_node_inputs(node, workflow, workflows, &options.inputs, &node_outputs);
        let node_patch = patch.nodes.get(&node.id);
        let is_enabled =
            enabled_nodes.contains(&node.id) || node_patch.is_some_and(|patch| patch.enable);
        let is_disabled = (node.disabled
            || disabled_nodes.contains(&node.id)
            || node_patch.is_some_and(|patch| patch.disable))
            && !is_enabled;

        if is_disabled
            && node_patch
                .and_then(|patch| patch.fallback_workflow_id.as_ref())
                .is_none()
        {
            executions.push(NodeExecution {
                node_id: node.id.clone(),
                workflow_id: node.workflow_id.clone(),
                selected_workflow_id: None,
                runtime: None,
                status: NodeExecutionStatus::Skipped,
                duration_ms: elapsed_ms(node_started_at),
                attempts: 0,
                inputs: node_inputs,
                outputs: serde_json::Map::new(),
                artifacts: Vec::new(),
                nodes: Vec::new(),
            });
            continue;
        }

        let selected_workflow_id =
            patched_node_workflow_id(node, &node_inputs, node_patch, is_disabled)?;
        let child = workflows
            .get(selected_workflow_id.as_str())
            .ok_or_else(|| {
                ApiError::InvalidRequest(format!(
                    "node {} references missing workflow {}",
                    node.id, selected_workflow_id
                ))
            })?;
        let child_inputs = declared_child_inputs(child, &node_inputs);
        let child_run = execute_child_workflow_with_patch(
            root,
            child,
            workflows,
            origins,
            &child_inputs,
            model_manager,
            node_patch,
        )?;

        node_outputs.insert(node.id.clone(), child_run.leaf.outputs.clone());
        artifacts.extend(child_run.leaf.artifacts.clone());

        executions.push(NodeExecution {
            node_id: node.id.clone(),
            workflow_id: node.workflow_id.clone(),
            selected_workflow_id: if node.kind == WorkflowNodeKind::If
                || selected_workflow_id != node.workflow_id
            {
                Some(selected_workflow_id)
            } else {
                None
            },
            runtime: child_run.leaf.runtime,
            status: NodeExecutionStatus::Completed,
            duration_ms: elapsed_ms(node_started_at),
            attempts: child_run.attempts,
            inputs: node_inputs,
            outputs: child_run.leaf.outputs,
            artifacts: child_run.leaf.artifacts,
            nodes: child_run.nodes,
        });
    }

    let outputs = collect_workflow_outputs(workflow, &options.inputs, &node_outputs)?;
    validate_values(&workflow.outputs, &outputs, "output", workflow)?;
    Ok(WorkflowExecution {
        workflow_id: workflow.id.clone(),
        version: workflow.version.clone(),
        inputs: options.inputs,
        outputs,
        runtime: None,
        artifacts,
        nodes: executions,
    })
}

fn execute_child_workflow(
    root: &Path,
    workflow: &WorkflowSpec,
    workflows: &BTreeMap<String, WorkflowSpec>,
    origins: &BTreeMap<String, WorkflowOrigin>,
    inputs: &serde_json::Map<String, serde_json::Value>,
    model_manager: &mut ModelManager,
) -> ApiResult<ChildExecution> {
    validate_values(&workflow.inputs, inputs, "input", workflow)?;
    if workflow.nodes.is_empty() {
        let leaf = execute_leaf_workflow(
            root,
            workflow,
            origins.get(&workflow.id),
            inputs,
            model_manager,
        )?;
        validate_values(&workflow.outputs, &leaf.outputs, "output", workflow)?;
        return Ok(ChildExecution {
            leaf,
            nodes: Vec::new(),
        });
    }

    let execution = execute_workflow_spec_with_models(
        root,
        workflow,
        workflows,
        origins,
        WorkflowExecutionOptions {
            inputs: inputs.clone(),
            disabled_nodes: Vec::new(),
            enabled_nodes: Vec::new(),
            patch: None,
        },
        model_manager,
    )?;
    Ok(ChildExecution {
        leaf: LeafExecution {
            outputs: execution.outputs,
            runtime: execution.runtime,
            artifacts: execution.artifacts,
        },
        nodes: execution.nodes,
    })
}

/// Keeps only the inputs a selected child workflow declares. If-node inputs
/// include the condition port and the union of both branch ports, so the
/// selected branch must not receive ports it never declared.
fn declared_child_inputs(
    child: &WorkflowSpec,
    node_inputs: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    let declared = child
        .inputs
        .iter()
        .map(|port| port.name.as_str())
        .collect::<BTreeSet<_>>();
    node_inputs
        .iter()
        .filter(|(name, _)| declared.contains(name.as_str()))
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect()
}

fn validate_values(
    ports: &[crate::workflow::PortSpec],
    values: &serde_json::Map<String, serde_json::Value>,
    subject: &str,
    workflow: &WorkflowSpec,
) -> ApiResult<()> {
    let issues = port_values::validate_port_map(ports, values, subject);
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ApiError::InvalidRequest(format!(
            "workflow {} has invalid {subject}s: {}",
            workflow.id,
            issues.join("; ")
        )))
    }
}

fn execute_child_workflow_with_patch(
    root: &Path,
    workflow: &WorkflowSpec,
    workflows: &BTreeMap<String, WorkflowSpec>,
    origins: &BTreeMap<String, WorkflowOrigin>,
    inputs: &serde_json::Map<String, serde_json::Value>,
    model_manager: &mut ModelManager,
    patch: Option<&WorkflowNodePatch>,
) -> ApiResult<ChildWorkflowRun> {
    let attempts = patch.and_then(|patch| patch.retry).unwrap_or(1).max(1);
    let timeout_ms = patch.and_then(|patch| patch.timeout_ms).map(u128::from);
    let mut last_error = None;

    for attempt in 1..=attempts {
        let started_at = Instant::now();
        match execute_child_workflow(root, workflow, workflows, origins, inputs, model_manager) {
            Ok(output) => {
                if let Some(timeout_ms) = timeout_ms
                    && started_at.elapsed().as_millis() > timeout_ms
                {
                    last_error = Some(ApiError::InvalidRequest(format!(
                        "node execution timed out after {timeout_ms}ms"
                    )));
                    continue;
                }
                return Ok(ChildWorkflowRun {
                    leaf: output.leaf,
                    attempts: attempt,
                    nodes: output.nodes,
                });
            }
            Err(error) => last_error = Some(error),
        }
    }

    Err(last_error.expect("node attempts are always at least one"))
}

fn patched_node_workflow_id(
    node: &WorkflowNode,
    inputs: &serde_json::Map<String, serde_json::Value>,
    patch: Option<&WorkflowNodePatch>,
    is_disabled: bool,
) -> ApiResult<String> {
    if let Some(fallback) = patch.and_then(|patch| patch.fallback_workflow_id.as_ref())
        && is_disabled
    {
        return Ok(fallback.clone());
    }
    if let Some(replacement) = patch.and_then(|patch| patch.replace_with.as_ref()) {
        return Ok(replacement.clone());
    }
    selected_node_workflow_id(node, inputs)
}

fn selected_node_workflow_id(
    node: &WorkflowNode,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<String> {
    match node.kind {
        WorkflowNodeKind::Workflow => Ok(node.workflow_id.clone()),
        WorkflowNodeKind::If => {
            let condition = node.condition.as_ref().ok_or_else(|| {
                ApiError::InvalidRequest(format!("if node {} has no condition", node.id))
            })?;
            let condition_matches = evaluate_condition(condition, inputs);
            if condition_matches {
                node.then_workflow_id.clone()
            } else {
                node.else_workflow_id.clone()
            }
            .ok_or_else(|| {
                ApiError::InvalidRequest(format!("if node {} has an incomplete branch", node.id))
            })
        }
    }
}

fn evaluate_condition(
    condition: &WorkflowCondition,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    match condition {
        WorkflowCondition::InputEquals { input, value } => inputs.get(input) == Some(value),
    }
}

fn elapsed_ms(started_at: Instant) -> u64 {
    started_at
        .elapsed()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}
