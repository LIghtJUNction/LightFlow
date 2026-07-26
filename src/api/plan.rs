use super::{ApiError, ApiResult};
use crate::api::executors::select_leaf_executor;
use crate::workflow::{WorkflowNode, WorkflowNodeKind, WorkflowSpec};
use std::collections::BTreeMap;

mod types;
pub(super) use types::{
    COMFYUI_API_ENGINE, COMFYUI_WORKFLOW_CAPABILITY, COMMAND_ENGINE, COMMAND_RUN_CAPABILITY,
    DataPolicy, ExecutionAtom, ExecutionPlan, ExecutionPlanNode, ExecutionRecipe,
    FLUX_EXTERNAL_ENGINE, FLUX_NATIVE_ENGINE, IMAGE_EDIT_CAPABILITY, IMAGE_GENERATE_CAPABILITY,
    IMAGE_INPAINT_CAPABILITY, LLM_GENERATE_CAPABILITY, PREVIEW_EDIT_ENGINE, PREVIEW_ENGINE,
    PREVIEW_INPAINT_ENGINE, PlannedModel, RUNNER_CAPABILITY, RUNNER_ENGINE,
};
pub use types::{
    WorkflowPlan, WorkflowPlanAtom, WorkflowPlanNode, WorkflowPlannedModel, WorkflowRuntimePlan,
};

pub(super) fn build_workflow_plan(
    workflow: &WorkflowSpec,
    workflows: &BTreeMap<String, WorkflowSpec>,
) -> ApiResult<WorkflowPlan> {
    let validation = super::validation::validate_workflow_spec(workflow, workflows);
    if !validation.valid {
        return Err(ApiError::InvalidRequest(validation.issues.join("; ")));
    }

    if workflow.nodes.is_empty() {
        let plan = build_leaf_execution_plan(workflow)?;
        return Ok(WorkflowPlan {
            workflow_id: workflow.id.clone(),
            version: workflow.version.clone(),
            kind: workflow_kind(workflow).to_owned(),
            runtime: Some(runtime_plan(workflow, &plan.node)),
            nodes: Vec::new(),
        });
    }

    let nodes_by_id = workflow
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let mut nodes = Vec::new();
    for node_id in validation.topological_order {
        let Some(node) = nodes_by_id.get(node_id.as_str()) else {
            continue;
        };
        nodes.push(plan_graph_node(node, workflows)?);
    }

    Ok(WorkflowPlan {
        workflow_id: workflow.id.clone(),
        version: workflow.version.clone(),
        kind: workflow_kind(workflow).to_owned(),
        runtime: None,
        nodes,
    })
}

fn plan_graph_node(
    node: &WorkflowNode,
    workflows: &BTreeMap<String, WorkflowSpec>,
) -> ApiResult<WorkflowPlanNode> {
    let candidate_workflow_ids = candidate_workflow_ids(node);
    let selected_workflow_id = match node.kind {
        WorkflowNodeKind::Workflow => Some(node.workflow_id.clone()),
        WorkflowNodeKind::If => None,
    };
    let child = selected_workflow_id
        .as_ref()
        .and_then(|workflow_id| workflows.get(workflow_id));
    let runtime = match child {
        Some(child) if !node.disabled && child.nodes.is_empty() => {
            let plan = build_leaf_execution_plan(child)?;
            Some(runtime_plan(child, &plan.node))
        }
        _ => None,
    };

    Ok(WorkflowPlanNode {
        node_id: node.id.clone(),
        kind: node_kind(node.kind).to_owned(),
        workflow_id: node.workflow_id.clone(),
        candidate_workflow_ids,
        selected_workflow_id,
        disabled: node.disabled,
        child_kind: child.map(workflow_kind).map(ToOwned::to_owned),
        runtime,
    })
}

fn candidate_workflow_ids(node: &WorkflowNode) -> Vec<String> {
    match node.kind {
        WorkflowNodeKind::Workflow => vec![node.workflow_id.clone()],
        WorkflowNodeKind::If => {
            let mut candidates = Vec::new();
            if let Some(workflow_id) = &node.then_workflow_id {
                candidates.push(workflow_id.clone());
            }
            if let Some(workflow_id) = &node.else_workflow_id
                && !candidates.contains(workflow_id)
            {
                candidates.push(workflow_id.clone());
            }
            candidates
        }
    }
}

fn runtime_plan(workflow: &WorkflowSpec, node: &ExecutionPlanNode) -> WorkflowRuntimePlan {
    WorkflowRuntimePlan {
        plan_node_id: node.id.clone(),
        executor_id: node.executor_id.clone(),
        executor_kind: node.executor_kind.clone(),
        executor_status: node.executor_status.clone(),
        executor_status_reason: node.executor_status_reason.clone(),
        executor_available: node.executor_available,
        capabilities: node.capabilities.clone(),
        data_policy: crate::api::executors::data_policy_name(node.data_policy).to_owned(),
        plans_models: node.plans_models,
        recipe: recipe_name(node.recipe).to_owned(),
        atoms: node
            .atoms
            .iter()
            .map(|atom| WorkflowPlanAtom {
                id: atom.id.clone(),
                capability: atom.capability.clone(),
            })
            .collect(),
        models: node
            .models
            .iter()
            .map(|model| WorkflowPlannedModel {
                requirement_id: model.requirement_id.clone(),
                capability: model.capability.clone(),
                preferred_format: model.preferred_format.clone(),
            })
            .collect(),
        declared: workflow.runtimes.clone(),
    }
}

pub(super) fn build_leaf_execution_plan(workflow: &WorkflowSpec) -> ApiResult<ExecutionPlan> {
    validate_explicit_runtime_engines(workflow)?;
    let explicit_runner = workflow
        .runtimes
        .iter()
        .any(|runtime| runtime.engine.as_deref() == Some(RUNNER_ENGINE))
        .then(|| super::executors::executor_by_id(RUNNER_ENGINE))
        .flatten();
    let explicit_unavailable = workflow.runtimes.iter().find_map(|runtime| {
        runtime
            .engine
            .as_deref()
            .and_then(super::executors::executor_by_id)
            .filter(|executor| executor.recipe == ExecutionRecipe::Unavailable)
    });
    let Some(executor) = explicit_runner
        .or(explicit_unavailable)
        .or_else(|| select_leaf_executor(workflow))
    else {
        let Some(runtime) = workflow.runtimes.first() else {
            unreachable!("passthrough executor matches workflows with no runtimes");
        };
        return Err(ApiError::InvalidRequest(format!(
            "workflow {} declares runtime capability {}, but this LightFlow build has no executor for it",
            workflow.id, runtime.capability
        )));
    };
    let selected = explicit_executor_for(workflow, executor).unwrap_or(executor);
    let (recipe_executor, public_executor) = (selected, selected);
    let info = public_executor.info();

    let node = ExecutionPlanNode {
        id: format!("{}::plan", workflow.id),
        executor_id: public_executor.id.to_owned(),
        executor_kind: public_executor.kind.to_owned(),
        executor_status: info.status.to_owned(),
        executor_status_reason: info.status_reason,
        executor_available: info.available,
        capabilities: public_executor
            .capabilities
            .iter()
            .map(|capability| (*capability).to_owned())
            .collect(),
        plans_models: recipe_executor.plans_models,
        recipe: recipe_executor.recipe,
        atoms: atoms(recipe_executor.atoms),
        models: if recipe_executor.plans_models {
            planned_models(workflow)
        } else {
            Vec::new()
        },
        data_policy: recipe_executor.data_policy,
    };

    Ok(ExecutionPlan {
        workflow_id: workflow.id.clone(),
        node,
    })
}

fn validate_explicit_runtime_engines(workflow: &WorkflowSpec) -> ApiResult<()> {
    for runtime in &workflow.runtimes {
        let Some(engine) = runtime.engine.as_deref() else {
            continue;
        };
        let executor = super::executors::executor_by_id(engine).ok_or_else(|| {
            ApiError::InvalidRequest(format!(
                "workflow {} runtime {} declares unknown engine {engine}",
                workflow.id, runtime.id
            ))
        })?;
        if engine != RUNNER_ENGINE && !executor.capabilities.contains(&runtime.capability.as_str())
        {
            return Err(ApiError::InvalidRequest(format!(
                "workflow {} runtime {} engine {engine} does not support capability {}",
                workflow.id, runtime.id, runtime.capability
            )));
        }
    }
    Ok(())
}

fn explicit_executor_for(
    workflow: &WorkflowSpec,
    matched: &'static super::executors::ExecutorDefinition,
) -> Option<&'static super::executors::ExecutorDefinition> {
    workflow.runtimes.iter().find_map(|runtime| {
        let engine = runtime.engine.as_deref()?;
        (engine == RUNNER_ENGINE || matched.capabilities.contains(&runtime.capability.as_str()))
            .then(|| super::executors::executor_by_id(engine))?
    })
}

fn atoms(items: &[(&str, &str)]) -> Vec<ExecutionAtom> {
    items
        .iter()
        .map(|(id, capability)| ExecutionAtom {
            id: (*id).to_owned(),
            capability: (*capability).to_owned(),
        })
        .collect()
}

fn planned_models(workflow: &WorkflowSpec) -> Vec<PlannedModel> {
    workflow
        .models
        .iter()
        .map(|model| PlannedModel {
            requirement_id: model.id.clone(),
            capability: model.capability.clone(),
            preferred_format: model.variants.first().map(|variant| variant.format.clone()),
        })
        .collect()
}

fn workflow_kind(workflow: &WorkflowSpec) -> &'static str {
    if workflow.nodes.is_empty() {
        "leaf"
    } else {
        "composite"
    }
}

fn node_kind(kind: WorkflowNodeKind) -> &'static str {
    match kind {
        WorkflowNodeKind::Workflow => "workflow",
        WorkflowNodeKind::If => "if",
    }
}

fn recipe_name(recipe: ExecutionRecipe) -> &'static str {
    match recipe {
        ExecutionRecipe::Passthrough => "passthrough",
        ExecutionRecipe::Unavailable => "unavailable",
        ExecutionRecipe::Runner => "runner",
        ExecutionRecipe::ExternalCommand => "external_command",
        ExecutionRecipe::ComfyUiWorkflow => "comfyui_workflow",
        ExecutionRecipe::PreviewTextToImage => "preview_text_to_image",
        ExecutionRecipe::PreviewImageEdit => "preview_image_edit",
        ExecutionRecipe::PreviewInpaint => "preview_inpaint",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::workflow_with_identity;

    #[test]
    fn explicit_comfyui_engine_selects_generic_image_capabilities() {
        for capability in [
            IMAGE_GENERATE_CAPABILITY,
            IMAGE_EDIT_CAPABILITY,
            IMAGE_INPAINT_CAPABILITY,
        ] {
            let workflow = workflow_with_identity(format!("lightflow.comfy.{capability}"), "0.1.0")
                .builtin_runtime("comfyui", capability, COMFYUI_API_ENGINE)
                .build();

            let plan = build_leaf_execution_plan(&workflow).expect("ComfyUI plan builds");

            assert_eq!(plan.node.executor_id, COMFYUI_API_ENGINE);
            assert_eq!(plan.node.recipe, ExecutionRecipe::ComfyUiWorkflow);
            assert_eq!(plan.node.data_policy, DataPolicy::ArtifactHandles);
            assert!(plan.node.models.is_empty());
        }
    }

    #[test]
    fn explicit_command_engine_selects_external_command_recipe() {
        let workflow = workflow_with_identity("lightflow.command_fixture", "0.1.0")
            .builtin_runtime("command", COMMAND_RUN_CAPABILITY, COMMAND_ENGINE)
            .build();

        let plan = build_leaf_execution_plan(&workflow).expect("command plan builds");

        assert_eq!(plan.node.executor_id, COMMAND_ENGINE);
        assert_eq!(plan.node.recipe, ExecutionRecipe::ExternalCommand);
        assert_eq!(plan.node.data_policy, DataPolicy::ArtifactHandles);
        assert!(!plan.node.plans_models);
    }

    #[test]
    fn legacy_builtin_engine_is_known_but_unavailable() {
        let workflow = workflow_with_identity("lightflow.legacy_concat", "0.1.0")
            .builtin_runtime("legacy", "lightflow.text.concat", "builtin.text.concat.v1")
            .build();

        let plan = build_leaf_execution_plan(&workflow).expect("legacy engine remains known");

        assert_eq!(plan.node.executor_id, "builtin.text.concat.v1");
        assert_eq!(plan.node.recipe, ExecutionRecipe::Unavailable);
        assert!(!plan.node.executor_available);
    }

    #[test]
    fn reserved_legacy_matchers_do_not_steal_runner_specs() {
        let workflow = workflow_with_identity("lightflow.runner_concat", "0.1.0")
            .builtin_runtime("runner", "lightflow.text.concat", RUNNER_ENGINE)
            .build();

        let plan = build_leaf_execution_plan(&workflow).expect("runner plan");

        assert_eq!(plan.node.executor_id, RUNNER_ENGINE);
        assert_eq!(plan.node.recipe, ExecutionRecipe::Runner);
    }
}
