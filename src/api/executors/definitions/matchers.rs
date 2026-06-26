use crate::api::flux;
use crate::api::plan::{
    CONTROL_IF_CAPABILITY, CONTROL_MERGE_CAPABILITY, CONTROL_SPLIT_CAPABILITY,
    CONTROL_SWITCH_CAPABILITY, IMAGE_CROP_CAPABILITY, IMAGE_EDIT_CAPABILITY,
    IMAGE_GENERATE_CAPABILITY, IMAGE_INPAINT_CAPABILITY, IMAGE_INVERT_CAPABILITY,
    IMAGE_LOAD_CAPABILITY, IMAGE_RESIZE_CAPABILITY, IMAGE_SAVE_CAPABILITY,
    IMAGE_UPSCALE_CAPABILITY, INVERT_ENGINE, JSON_EXTRACT_CAPABILITY, LLM_CLASSIFY_CAPABILITY,
    LLM_GENERATE_CAPABILITY, LLM_MOCK_ENGINE, LLM_STRUCTURED_OUTPUT_CAPABILITY,
    MASK_COMPOSE_CAPABILITY, MODEL_LOCK_CHECK_CAPABILITY, MODEL_SELECT_CAPABILITY,
    PREVIEW_EDIT_ENGINE, PREVIEW_ENGINE, PREVIEW_INPAINT_ENGINE, TEXT_CONCAT_CAPABILITY,
    TEXT_REGEX_CAPABILITY, TEXT_TEMPLATE_CAPABILITY,
};
use crate::workflow::WorkflowSpec;

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

pub(super) fn matches_flux_text_to_image(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, IMAGE_GENERATE_CAPABILITY)
        && flux::workflow_declares_flux_assets(workflow)
}

pub(super) fn matches_flux_image_edit(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, IMAGE_EDIT_CAPABILITY)
        && flux::workflow_declares_flux_assets(workflow)
}

pub(super) fn matches_flux_inpaint(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, IMAGE_INPAINT_CAPABILITY)
        && flux::workflow_declares_flux_assets(workflow)
}

pub(super) fn matches_image_invert(workflow: &WorkflowSpec) -> bool {
    workflow.runtimes.iter().any(|runtime| {
        runtime.capability == IMAGE_INVERT_CAPABILITY
            && runtime.engine.as_deref() == Some(INVERT_ENGINE)
    })
}

pub(super) fn matches_image_load(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, IMAGE_LOAD_CAPABILITY)
}

pub(super) fn matches_image_save(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, IMAGE_SAVE_CAPABILITY)
}

pub(super) fn matches_image_resize(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, IMAGE_RESIZE_CAPABILITY)
}

pub(super) fn matches_image_crop(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, IMAGE_CROP_CAPABILITY)
}

pub(super) fn matches_image_upscale(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, IMAGE_UPSCALE_CAPABILITY)
}

pub(super) fn matches_mask_compose(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, MASK_COMPOSE_CAPABILITY)
}

pub(super) fn matches_builtin_llm_generate(workflow: &WorkflowSpec) -> bool {
    workflow.runtimes.iter().any(|runtime| {
        runtime.capability == LLM_GENERATE_CAPABILITY
            && runtime.engine.as_deref() == Some(LLM_MOCK_ENGINE)
    })
}

pub(super) fn matches_rig_llm(workflow: &WorkflowSpec) -> bool {
    workflow.runtimes.iter().any(|runtime| {
        runtime.capability == LLM_GENERATE_CAPABILITY
            && runtime.engine.as_deref() != Some(LLM_MOCK_ENGINE)
    })
}

pub(super) fn matches_text_concat(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, TEXT_CONCAT_CAPABILITY)
}

pub(super) fn matches_text_template(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, TEXT_TEMPLATE_CAPABILITY)
}

pub(super) fn matches_text_regex(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, TEXT_REGEX_CAPABILITY)
}

pub(super) fn matches_json_extract(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, JSON_EXTRACT_CAPABILITY)
}

pub(super) fn matches_control_if(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, CONTROL_IF_CAPABILITY)
}

pub(super) fn matches_control_switch(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, CONTROL_SWITCH_CAPABILITY)
}

pub(super) fn matches_control_merge(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, CONTROL_MERGE_CAPABILITY)
}

pub(super) fn matches_control_split(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, CONTROL_SPLIT_CAPABILITY)
}

pub(super) fn matches_model_select(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, MODEL_SELECT_CAPABILITY)
}

pub(super) fn matches_model_lock_check(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, MODEL_LOCK_CHECK_CAPABILITY)
}

pub(super) fn matches_llm_classify(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, LLM_CLASSIFY_CAPABILITY)
}

pub(super) fn matches_llm_structured_output(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, LLM_STRUCTURED_OUTPUT_CAPABILITY)
}

pub(super) fn matches_never(_workflow: &WorkflowSpec) -> bool {
    false
}

pub(super) fn workflow_has_capability(workflow: &WorkflowSpec, capability: &str) -> bool {
    workflow
        .runtimes
        .iter()
        .any(|runtime| runtime.capability == capability)
}
