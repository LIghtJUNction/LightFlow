use crate::api::plan::{
    COMFYUI_API_ENGINE, COMFYUI_WORKFLOW_CAPABILITY, COMMAND_ENGINE, COMMAND_RUN_CAPABILITY,
    IMAGE_EDIT_CAPABILITY, IMAGE_GENERATE_CAPABILITY, IMAGE_INPAINT_CAPABILITY,
    PREVIEW_EDIT_ENGINE, PREVIEW_ENGINE, PREVIEW_INPAINT_ENGINE, RUNNER_ENGINE,
};
use crate::workflow::WorkflowSpec;

pub(super) fn matches_comfyui(workflow: &WorkflowSpec) -> bool {
    workflow.runtimes.iter().any(|runtime| {
        runtime.capability == COMFYUI_WORKFLOW_CAPABILITY
            || ([
                IMAGE_GENERATE_CAPABILITY,
                IMAGE_EDIT_CAPABILITY,
                IMAGE_INPAINT_CAPABILITY,
            ]
            .contains(&runtime.capability.as_str())
                && runtime.engine.as_deref() == Some(COMFYUI_API_ENGINE))
    })
}

pub(super) fn matches_command(workflow: &WorkflowSpec) -> bool {
    workflow.runtimes.iter().any(|runtime| {
        runtime.capability == COMMAND_RUN_CAPABILITY
            && runtime.engine.as_deref() == Some(COMMAND_ENGINE)
    })
}

pub(super) fn matches_runner(workflow: &WorkflowSpec) -> bool {
    workflow
        .runtimes
        .iter()
        .any(|runtime| runtime.engine.as_deref() == Some(RUNNER_ENGINE))
}

pub(super) fn matches_passthrough(workflow: &WorkflowSpec) -> bool {
    workflow.runtimes.is_empty()
}

pub(super) fn matches_preview_text_to_image(workflow: &WorkflowSpec) -> bool {
    workflow.runtimes.iter().any(|runtime| {
        runtime.capability == IMAGE_GENERATE_CAPABILITY
            && runtime.engine.as_deref() == Some(PREVIEW_ENGINE)
    })
}

pub(super) fn matches_preview_image_edit(workflow: &WorkflowSpec) -> bool {
    workflow.runtimes.iter().any(|runtime| {
        runtime.capability == IMAGE_EDIT_CAPABILITY
            && runtime.engine.as_deref() == Some(PREVIEW_EDIT_ENGINE)
    })
}

pub(super) fn matches_preview_inpaint(workflow: &WorkflowSpec) -> bool {
    workflow.runtimes.iter().any(|runtime| {
        runtime.capability == IMAGE_INPAINT_CAPABILITY
            && runtime.engine.as_deref() == Some(PREVIEW_INPAINT_ENGINE)
    })
}

pub(super) fn matches_never(_workflow: &WorkflowSpec) -> bool {
    false
}
