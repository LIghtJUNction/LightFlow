use super::{ApiError, ApiResult};
use crate::api::executors::select_leaf_executor;
use crate::workflow::WorkflowSpec;

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

    let node = ExecutionPlanNode {
        id: format!("{}::plan", workflow.id),
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
