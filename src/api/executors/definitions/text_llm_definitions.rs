use super::super::{ExecutorAvailability, ExecutorDefinition};
use super::matchers::matches_never;
use crate::api::plan::{
    DataPolicy, ExecutionRecipe, FLUX_EXTERNAL_ENGINE, FLUX_NATIVE_ENGINE, IMAGE_EDIT_CAPABILITY,
    IMAGE_GENERATE_CAPABILITY, IMAGE_INPAINT_CAPABILITY, LLM_GENERATE_CAPABILITY,
};

pub(super) static TEXT_LLM_EXECUTORS: [ExecutorDefinition; 3] = [
    ExecutorDefinition {
        id: "rig-core",
        kind: "native",
        capabilities: &[LLM_GENERATE_CAPABILITY],
        features: &["rig"],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Unavailable,
        recipe: ExecutionRecipe::Unavailable,
        data_policy: DataPolicy::JsonValues,
        atoms: &[
            (
                "lightflow.atom.select_llm_provider",
                "lightflow.llm.provider",
            ),
            ("lightflow.atom.build_rig_agent", LLM_GENERATE_CAPABILITY),
            ("lightflow.atom.prompt_llm", "lightflow.text.generate"),
        ],
        plans_models: true,
        matcher: matches_never,
    },
    ExecutorDefinition {
        id: FLUX_EXTERNAL_ENGINE,
        kind: "external",
        capabilities: &[
            IMAGE_GENERATE_CAPABILITY,
            IMAGE_EDIT_CAPABILITY,
            IMAGE_INPAINT_CAPABILITY,
        ],
        features: &["flux"],
        env: Some("LIGHTFLOW_FLUX_RUNNER"),
        command_env: Some("LIGHTFLOW_FLUX_RUNNER"),
        visible: true,
        availability: ExecutorAvailability::Unavailable,
        recipe: ExecutionRecipe::Unavailable,
        data_policy: DataPolicy::DeviceResidentPreferred,
        atoms: &[],
        plans_models: true,
        matcher: matches_never,
    },
    ExecutorDefinition {
        id: FLUX_NATIVE_ENGINE,
        kind: "native",
        capabilities: &[
            IMAGE_GENERATE_CAPABILITY,
            IMAGE_EDIT_CAPABILITY,
            IMAGE_INPAINT_CAPABILITY,
        ],
        features: &["flux-native"],
        env: Some("LIGHTFLOW_FLUX_BACKEND"),
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Unavailable,
        recipe: ExecutionRecipe::Unavailable,
        data_policy: DataPolicy::DeviceResidentPreferred,
        atoms: &[],
        plans_models: true,
        matcher: matches_never,
    },
];
