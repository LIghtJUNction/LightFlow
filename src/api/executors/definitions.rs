mod control_model_reserved_definitions;
mod core_preview_definitions;
mod flux_definitions;
mod image_definitions;
mod matchers;
mod text_llm_definitions;

use super::ExecutorDefinition;
use control_model_reserved_definitions::CONTROL_MODEL_RESERVED_EXECUTORS;
use core_preview_definitions::CORE_PREVIEW_EXECUTORS;
use flux_definitions::FLUX_EXECUTORS;
use image_definitions::IMAGE_EXECUTORS;
use text_llm_definitions::TEXT_LLM_EXECUTORS;

pub(super) fn executor_definitions() -> Vec<&'static ExecutorDefinition> {
    CORE_PREVIEW_EXECUTORS
        .iter()
        .chain(FLUX_EXECUTORS.iter())
        .chain(IMAGE_EXECUTORS.iter())
        .chain(TEXT_LLM_EXECUTORS.iter())
        .chain(CONTROL_MODEL_RESERVED_EXECUTORS.iter())
        .collect()
}
