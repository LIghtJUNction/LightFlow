mod comfyui_definitions;
mod command_definitions;
mod control_model_reserved_definitions;
mod core_preview_definitions;
mod flux_definitions;
mod legacy_std_definitions;
mod matchers;
mod runner_definitions;
mod text_llm_definitions;

use super::ExecutorDefinition;
use comfyui_definitions::COMFYUI_EXECUTORS;
use command_definitions::COMMAND_EXECUTORS;
use control_model_reserved_definitions::CONTROL_MODEL_RESERVED_EXECUTORS;
use core_preview_definitions::CORE_PREVIEW_EXECUTORS;
use flux_definitions::FLUX_EXECUTORS;
use legacy_std_definitions::LEGACY_STD_EXECUTORS;
use runner_definitions::RUNNER_EXECUTORS;
use text_llm_definitions::TEXT_LLM_EXECUTORS;

pub(super) fn executor_definitions() -> Vec<&'static ExecutorDefinition> {
    COMFYUI_EXECUTORS
        .iter()
        .chain(RUNNER_EXECUTORS.iter())
        .chain(COMMAND_EXECUTORS.iter())
        .chain(CORE_PREVIEW_EXECUTORS.iter())
        .chain(FLUX_EXECUTORS.iter())
        .chain(LEGACY_STD_EXECUTORS.iter())
        .chain(TEXT_LLM_EXECUTORS.iter())
        .chain(CONTROL_MODEL_RESERVED_EXECUTORS.iter())
        .collect()
}
