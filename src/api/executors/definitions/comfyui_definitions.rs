use super::super::{ExecutorAvailability, ExecutorDefinition};
use super::matchers::matches_comfyui;
use crate::api::plan::{
    COMFYUI_API_ENGINE, COMFYUI_WORKFLOW_CAPABILITY, DataPolicy, ExecutionRecipe,
    IMAGE_EDIT_CAPABILITY, IMAGE_GENERATE_CAPABILITY, IMAGE_INPAINT_CAPABILITY,
};

pub(super) static COMFYUI_EXECUTORS: [ExecutorDefinition; 1] = [ExecutorDefinition {
    id: COMFYUI_API_ENGINE,
    kind: "external",
    capabilities: &[
        COMFYUI_WORKFLOW_CAPABILITY,
        IMAGE_GENERATE_CAPABILITY,
        IMAGE_EDIT_CAPABILITY,
        IMAGE_INPAINT_CAPABILITY,
    ],
    features: &[],
    env: None,
    command_env: None,
    visible: true,
    availability: ExecutorAvailability::EndpointCheckedAtRun,
    recipe: ExecutionRecipe::ComfyUiWorkflow,
    data_policy: DataPolicy::ArtifactHandles,
    atoms: &[
        ("lightflow.atom.comfyui.upload", "lightflow.artifact.upload"),
        ("lightflow.atom.comfyui.submit", COMFYUI_WORKFLOW_CAPABILITY),
        ("lightflow.atom.comfyui.poll", "lightflow.remote.poll"),
        (
            "lightflow.atom.comfyui.download",
            "lightflow.artifact.download",
        ),
    ],
    plans_models: false,
    matcher: matches_comfyui,
}];
