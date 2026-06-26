use super::{ApiError, ApiResult};
use crate::api::executors::select_leaf_executor;
use crate::workflow::{RuntimeRequirement, WorkflowNode, WorkflowNodeKind, WorkflowSpec};
use serde::Serialize;
use std::collections::BTreeMap;

pub(super) const IMAGE_GENERATE_CAPABILITY: &str = "lightflow.image.generate";
pub(super) const IMAGE_EDIT_CAPABILITY: &str = "lightflow.image.edit";
pub(super) const IMAGE_INPAINT_CAPABILITY: &str = "lightflow.image.inpaint";
pub(super) const IMAGE_INVERT_CAPABILITY: &str = "lightflow.image.invert";
pub(super) const IMAGE_LOAD_CAPABILITY: &str = "lightflow.image.load";
pub(super) const IMAGE_SAVE_CAPABILITY: &str = "lightflow.image.save";
pub(super) const IMAGE_RESIZE_CAPABILITY: &str = "lightflow.image.resize";
pub(super) const IMAGE_CROP_CAPABILITY: &str = "lightflow.image.crop";
pub(super) const LLM_GENERATE_CAPABILITY: &str = "lightflow.llm.generate";
pub(super) const TEXT_CONCAT_CAPABILITY: &str = "lightflow.text.concat";
pub(super) const TEXT_TEMPLATE_CAPABILITY: &str = "lightflow.text.template";
pub(super) const TEXT_REGEX_CAPABILITY: &str = "lightflow.text.regex";
pub(super) const JSON_EXTRACT_CAPABILITY: &str = "lightflow.json.extract";
pub(super) const CONTROL_IF_CAPABILITY: &str = "lightflow.control.if";
pub(super) const CONTROL_SWITCH_CAPABILITY: &str = "lightflow.control.switch";
pub(super) const CONTROL_MERGE_CAPABILITY: &str = "lightflow.control.merge";
pub(super) const CONTROL_SPLIT_CAPABILITY: &str = "lightflow.control.split";
pub(super) const MODEL_SELECT_CAPABILITY: &str = "lightflow.model.select";
pub(super) const MODEL_LOCK_CHECK_CAPABILITY: &str = "lightflow.model.lock.check";
pub(super) const IMAGE_UPSCALE_CAPABILITY: &str = "lightflow.image.upscale";
pub(super) const MASK_COMPOSE_CAPABILITY: &str = "lightflow.mask.compose";
pub(super) const LLM_CLASSIFY_CAPABILITY: &str = "lightflow.llm.classify";
pub(super) const LLM_STRUCTURED_OUTPUT_CAPABILITY: &str = "lightflow.llm.structured_output";
pub(super) const PREVIEW_ENGINE: &str = "builtin.preview.v1";
pub(super) const PREVIEW_EDIT_ENGINE: &str = "builtin.preview.edit.v1";
pub(super) const PREVIEW_INPAINT_ENGINE: &str = "builtin.preview.inpaint.v1";
pub(super) const INVERT_ENGINE: &str = "builtin.image.invert.v1";
pub(super) const LLM_MOCK_ENGINE: &str = "builtin.llm.mock.v1";

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct ExecutionPlan {
    pub(super) workflow_id: String,
    pub(super) node: ExecutionPlanNode,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct ExecutionPlanNode {
    pub(super) id: String,
    pub(super) executor_id: String,
    pub(super) executor_kind: String,
    pub(super) executor_status: String,
    pub(super) executor_status_reason: String,
    pub(super) executor_available: bool,
    pub(super) capabilities: Vec<String>,
    pub(super) plans_models: bool,
    pub(super) recipe: ExecutionRecipe,
    pub(super) atoms: Vec<ExecutionAtom>,
    pub(super) models: Vec<PlannedModel>,
    pub(super) data_policy: DataPolicy,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) enum ExecutionRecipe {
    Passthrough,
    PreviewTextToImage,
    FluxTextToImage,
    FluxImageEdit,
    FluxInpaint,
    ImageInvert,
    ImageLoad,
    ImageSave,
    ImageResize,
    ImageCrop,
    PreviewImageEdit,
    PreviewInpaint,
    RigLlmGenerate,
    TextConcat,
    TextTemplate,
    TextRegex,
    JsonExtract,
    ControlIf,
    ControlSwitch,
    ControlMerge,
    ControlSplit,
    ModelSelect,
    ModelLockCheck,
    ImageUpscale,
    MaskCompose,
    BuiltinLlmGenerate,
    LlmClassify,
    LlmStructuredOutput,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct ExecutionAtom {
    pub(super) id: String,
    pub(super) capability: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct PlannedModel {
    pub(super) requirement_id: String,
    pub(super) capability: String,
    pub(super) preferred_format: Option<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) enum DataPolicy {
    JsonValues,
    ArtifactHandles,
    DeviceResidentPreferred,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct WorkflowPlan {
    pub workflow_id: String,
    pub version: String,
    pub kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime: Option<WorkflowRuntimePlan>,
    pub nodes: Vec<WorkflowPlanNode>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct WorkflowPlanNode {
    pub node_id: String,
    pub kind: String,
    pub workflow_id: String,
    pub candidate_workflow_ids: Vec<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected_workflow_id: Option<String>,
    pub disabled: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub child_kind: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub runtime: Option<WorkflowRuntimePlan>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct WorkflowRuntimePlan {
    pub plan_node_id: String,
    pub executor_id: String,
    pub executor_kind: String,
    pub executor_status: String,
    pub executor_status_reason: String,
    pub executor_available: bool,
    pub capabilities: Vec<String>,
    pub data_policy: String,
    pub plans_models: bool,
    pub recipe: String,
    pub atoms: Vec<WorkflowPlanAtom>,
    pub models: Vec<WorkflowPlannedModel>,
    pub declared: Vec<RuntimeRequirement>,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct WorkflowPlanAtom {
    pub id: String,
    pub capability: String,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct WorkflowPlannedModel {
    pub requirement_id: String,
    pub capability: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preferred_format: Option<String>,
}

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
        Some(child) if child.nodes.is_empty() => {
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
    let Some(executor) = select_leaf_executor(workflow) else {
        let Some(runtime) = workflow.runtimes.first() else {
            unreachable!("passthrough executor matches workflows with no runtimes");
        };
        return Err(ApiError::InvalidRequest(format!(
            "workflow {} declares runtime capability {}, but this LightFlow build has no executor for it",
            workflow.id, runtime.capability
        )));
    };
    let info = executor.info();

    let node = ExecutionPlanNode {
        id: format!("{}::plan", workflow.id),
        executor_id: executor.id.to_owned(),
        executor_kind: executor.kind.to_owned(),
        executor_status: info.status.to_owned(),
        executor_status_reason: info.status_reason,
        executor_available: info.available,
        capabilities: executor
            .capabilities
            .iter()
            .map(|capability| (*capability).to_owned())
            .collect(),
        plans_models: executor.plans_models,
        recipe: executor.recipe,
        atoms: atoms(executor.atoms),
        models: if executor.plans_models {
            planned_models(workflow)
        } else {
            Vec::new()
        },
        data_policy: executor.data_policy,
    };

    Ok(ExecutionPlan {
        workflow_id: workflow.id.clone(),
        node,
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
        ExecutionRecipe::PreviewTextToImage => "preview_text_to_image",
        ExecutionRecipe::FluxTextToImage => "flux_text_to_image",
        ExecutionRecipe::FluxImageEdit => "flux_image_edit",
        ExecutionRecipe::FluxInpaint => "flux_inpaint",
        ExecutionRecipe::ImageInvert => "image_invert",
        ExecutionRecipe::ImageLoad => "image_load",
        ExecutionRecipe::ImageSave => "image_save",
        ExecutionRecipe::ImageResize => "image_resize",
        ExecutionRecipe::ImageCrop => "image_crop",
        ExecutionRecipe::PreviewImageEdit => "preview_image_edit",
        ExecutionRecipe::PreviewInpaint => "preview_inpaint",
        ExecutionRecipe::RigLlmGenerate => "rig_llm_generate",
        ExecutionRecipe::TextConcat => "text_concat",
        ExecutionRecipe::TextTemplate => "text_template",
        ExecutionRecipe::TextRegex => "text_regex",
        ExecutionRecipe::JsonExtract => "json_extract",
        ExecutionRecipe::ControlIf => "control_if",
        ExecutionRecipe::ControlSwitch => "control_switch",
        ExecutionRecipe::ControlMerge => "control_merge",
        ExecutionRecipe::ControlSplit => "control_split",
        ExecutionRecipe::ModelSelect => "model_select",
        ExecutionRecipe::ModelLockCheck => "model_lock_check",
        ExecutionRecipe::ImageUpscale => "image_upscale",
        ExecutionRecipe::MaskCompose => "mask_compose",
        ExecutionRecipe::BuiltinLlmGenerate => "builtin_llm_generate",
        ExecutionRecipe::LlmClassify => "llm_classify",
        ExecutionRecipe::LlmStructuredOutput => "llm_structured_output",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::preload::*;

    #[test]
    fn flux_workflow_builds_single_device_resident_plan_node() {
        let workflow = workflow("lightflow.flux.text_to_image")
            .runtime("image_runtime", IMAGE_GENERATE_CAPABILITY)
            .hf_model(
                "flux_model",
                "flux2-klein-q3",
                "image-generation",
                "gguf",
                "owner/repo",
                "model.gguf",
            )
            .hf_model(
                "llm_model",
                "qwen-q4",
                "text-encoder",
                "gguf",
                "owner/llm",
                "llm.gguf",
            )
            .hf_model(
                "vae_model",
                "flux-ae",
                "vae",
                "safetensors",
                "owner/vae",
                "ae.safetensors",
            )
            .build();

        let plan = build_leaf_execution_plan(&workflow).expect("plan builds");

        assert_eq!(plan.workflow_id, workflow.id);
        assert_eq!(plan.node.recipe, ExecutionRecipe::FluxTextToImage);
        assert_eq!(plan.node.data_policy, DataPolicy::DeviceResidentPreferred);
        assert_eq!(plan.node.models.len(), 3);
        assert_eq!(plan.node.atoms[0].id, "lightflow.atom.load_flux_model");
    }
}
