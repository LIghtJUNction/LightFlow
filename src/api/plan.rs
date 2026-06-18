use super::{ApiError, ApiResult};
use crate::workflow::WorkflowSpec;

pub(super) const IMAGE_GENERATE_CAPABILITY: &str = "lightflow.image.generate";
pub(super) const IMAGE_EDIT_CAPABILITY: &str = "lightflow.image.edit";
pub(super) const IMAGE_INPAINT_CAPABILITY: &str = "lightflow.image.inpaint";
pub(super) const IMAGE_INVERT_CAPABILITY: &str = "lightflow.image.invert";
pub(super) const LLM_GENERATE_CAPABILITY: &str = "lightflow.llm.generate";
pub(super) const PREVIEW_ENGINE: &str = "builtin.preview.v1";
pub(super) const INVERT_ENGINE: &str = "builtin.image.invert.v1";

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
    RigLlmGenerate,
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
    let node = if workflow.runtimes.iter().any(|runtime| {
        runtime.capability == IMAGE_GENERATE_CAPABILITY
            && runtime.engine.as_deref() == Some(PREVIEW_ENGINE)
    }) {
        ExecutionPlanNode {
            id: format!("{}::plan", workflow.id),
            recipe: ExecutionRecipe::PreviewTextToImage,
            atoms: atoms(&[
                ("lightflow.atom.prompt", "lightflow.text.prompt"),
                ("lightflow.atom.preview_pixels", IMAGE_GENERATE_CAPABILITY),
                ("lightflow.atom.save_image", "lightflow.artifact.image"),
            ]),
            models: planned_models(workflow),
            data_policy: DataPolicy::ArtifactHandles,
        }
    } else if workflow
        .runtimes
        .iter()
        .any(|runtime| runtime.capability == IMAGE_GENERATE_CAPABILITY)
        && super::flux::workflow_declares_flux_assets(workflow)
    {
        ExecutionPlanNode {
            id: format!("{}::plan", workflow.id),
            recipe: ExecutionRecipe::FluxTextToImage,
            atoms: atoms(&[
                ("lightflow.atom.load_flux_model", "lightflow.model.load"),
                ("lightflow.atom.load_text_encoder", "lightflow.model.load"),
                ("lightflow.atom.load_vae", "lightflow.model.load"),
                ("lightflow.atom.encode_prompt", "lightflow.text.encode"),
                ("lightflow.atom.sample_latents", IMAGE_GENERATE_CAPABILITY),
                ("lightflow.atom.decode_vae", "lightflow.image.decode"),
                ("lightflow.atom.save_image", "lightflow.artifact.image"),
            ]),
            models: planned_models(workflow),
            data_policy: DataPolicy::DeviceResidentPreferred,
        }
    } else if workflow
        .runtimes
        .iter()
        .any(|runtime| runtime.capability == IMAGE_EDIT_CAPABILITY)
        && super::flux::workflow_declares_flux_assets(workflow)
    {
        ExecutionPlanNode {
            id: format!("{}::plan", workflow.id),
            recipe: ExecutionRecipe::FluxImageEdit,
            atoms: atoms(&[
                ("lightflow.atom.load_image", "lightflow.artifact.image"),
                ("lightflow.atom.load_flux_model", "lightflow.model.load"),
                ("lightflow.atom.load_text_encoder", "lightflow.model.load"),
                ("lightflow.atom.load_vae", "lightflow.model.load"),
                ("lightflow.atom.encode_prompt", "lightflow.text.encode"),
                ("lightflow.atom.sample_latents", IMAGE_EDIT_CAPABILITY),
                ("lightflow.atom.decode_vae", "lightflow.image.decode"),
                ("lightflow.atom.save_image", "lightflow.artifact.image"),
            ]),
            models: planned_models(workflow),
            data_policy: DataPolicy::DeviceResidentPreferred,
        }
    } else if workflow
        .runtimes
        .iter()
        .any(|runtime| runtime.capability == IMAGE_INPAINT_CAPABILITY)
        && super::flux::workflow_declares_flux_assets(workflow)
    {
        ExecutionPlanNode {
            id: format!("{}::plan", workflow.id),
            recipe: ExecutionRecipe::FluxInpaint,
            atoms: atoms(&[
                ("lightflow.atom.load_image", "lightflow.artifact.image"),
                ("lightflow.atom.load_mask", "lightflow.artifact.mask"),
                ("lightflow.atom.load_flux_model", "lightflow.model.load"),
                ("lightflow.atom.load_text_encoder", "lightflow.model.load"),
                ("lightflow.atom.load_vae", "lightflow.model.load"),
                ("lightflow.atom.encode_prompt", "lightflow.text.encode"),
                ("lightflow.atom.sample_latents", IMAGE_INPAINT_CAPABILITY),
                ("lightflow.atom.decode_vae", "lightflow.image.decode"),
                ("lightflow.atom.save_image", "lightflow.artifact.image"),
            ]),
            models: planned_models(workflow),
            data_policy: DataPolicy::DeviceResidentPreferred,
        }
    } else if workflow.runtimes.iter().any(|runtime| {
        runtime.capability == IMAGE_INVERT_CAPABILITY
            && runtime.engine.as_deref() == Some(INVERT_ENGINE)
    }) {
        ExecutionPlanNode {
            id: format!("{}::plan", workflow.id),
            recipe: ExecutionRecipe::ImageInvert,
            atoms: atoms(&[
                ("lightflow.atom.load_image", "lightflow.artifact.image"),
                ("lightflow.atom.invert_pixels", IMAGE_INVERT_CAPABILITY),
                ("lightflow.atom.save_image", "lightflow.artifact.image"),
            ]),
            models: Vec::new(),
            data_policy: DataPolicy::ArtifactHandles,
        }
    } else if workflow
        .runtimes
        .iter()
        .any(|runtime| runtime.capability == LLM_GENERATE_CAPABILITY)
    {
        ExecutionPlanNode {
            id: format!("{}::plan", workflow.id),
            recipe: ExecutionRecipe::RigLlmGenerate,
            atoms: atoms(&[
                (
                    "lightflow.atom.select_llm_provider",
                    "lightflow.llm.provider",
                ),
                ("lightflow.atom.build_rig_agent", LLM_GENERATE_CAPABILITY),
                ("lightflow.atom.prompt_llm", "lightflow.text.generate"),
            ]),
            models: planned_models(workflow),
            data_policy: DataPolicy::JsonValues,
        }
    } else if let Some(runtime) = workflow.runtimes.first() {
        return Err(ApiError::InvalidRequest(format!(
            "workflow {} declares runtime capability {}, but this LightFlow build has no executor for it",
            workflow.id, runtime.capability
        )));
    } else {
        ExecutionPlanNode {
            id: format!("{}::plan", workflow.id),
            recipe: ExecutionRecipe::Passthrough,
            atoms: atoms(&[("lightflow.atom.passthrough", "lightflow.data.copy")]),
            models: Vec::new(),
            data_policy: DataPolicy::JsonValues,
        }
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
