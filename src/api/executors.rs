use super::plan::{
    CONTROL_IF_CAPABILITY, CONTROL_MERGE_CAPABILITY, CONTROL_SPLIT_CAPABILITY,
    CONTROL_SWITCH_CAPABILITY, DataPolicy, ExecutionRecipe, IMAGE_CROP_CAPABILITY,
    IMAGE_EDIT_CAPABILITY, IMAGE_GENERATE_CAPABILITY, IMAGE_INPAINT_CAPABILITY,
    IMAGE_INVERT_CAPABILITY, IMAGE_LOAD_CAPABILITY, IMAGE_RESIZE_CAPABILITY, IMAGE_SAVE_CAPABILITY,
    IMAGE_UPSCALE_CAPABILITY, INVERT_ENGINE, JSON_EXTRACT_CAPABILITY, LLM_CLASSIFY_CAPABILITY,
    LLM_GENERATE_CAPABILITY, LLM_MOCK_ENGINE, LLM_STRUCTURED_OUTPUT_CAPABILITY,
    MASK_COMPOSE_CAPABILITY, MODEL_LOCK_CHECK_CAPABILITY, MODEL_SELECT_CAPABILITY,
    PREVIEW_EDIT_ENGINE, PREVIEW_ENGINE, PREVIEW_INPAINT_ENGINE, TEXT_CONCAT_CAPABILITY,
    TEXT_REGEX_CAPABILITY, TEXT_TEMPLATE_CAPABILITY,
};
use crate::workflow::WorkflowSpec;
use serde::Serialize;
use std::env;

#[derive(Debug, Clone, Serialize)]
pub struct ExecutorInfo {
    pub id: &'static str,
    pub kind: &'static str,
    pub capabilities: Vec<&'static str>,
    pub available: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub features: Vec<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub env: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub command: Option<String>,
}

pub(super) struct ExecutorDefinition {
    pub(super) id: &'static str,
    kind: &'static str,
    capabilities: &'static [&'static str],
    features: &'static [&'static str],
    env: Option<&'static str>,
    command_env: Option<&'static str>,
    visible: bool,
    availability: ExecutorAvailability,
    pub(super) recipe: ExecutionRecipe,
    pub(super) data_policy: DataPolicy,
    pub(super) atoms: &'static [(&'static str, &'static str)],
    pub(super) plans_models: bool,
    matcher: fn(&WorkflowSpec) -> bool,
}

impl ExecutorDefinition {
    fn info(&self) -> ExecutorInfo {
        ExecutorInfo {
            id: self.id,
            kind: self.kind,
            capabilities: self.capabilities.to_vec(),
            available: self.availability.available(),
            features: self.features.to_vec(),
            env: self.env,
            command: self.command_env.and_then(|name| env::var(name).ok()),
        }
    }
}

#[derive(Debug, Clone, Copy)]
enum ExecutorAvailability {
    Always,
    Unavailable,
    EnvPresent(&'static str),
    Feature(bool),
}

impl ExecutorAvailability {
    fn available(self) -> bool {
        match self {
            Self::Always => true,
            Self::Unavailable => false,
            Self::EnvPresent(name) => env::var(name).is_ok(),
            Self::Feature(enabled) => enabled,
        }
    }
}

pub fn executor_registry() -> Vec<ExecutorInfo> {
    executor_definitions()
        .iter()
        .filter(|executor| executor.visible)
        .map(ExecutorDefinition::info)
        .collect()
}

pub(super) fn select_leaf_executor(workflow: &WorkflowSpec) -> Option<&'static ExecutorDefinition> {
    executor_definitions()
        .iter()
        .find(|executor| (executor.matcher)(workflow))
}

fn executor_definitions() -> &'static [ExecutorDefinition] {
    &EXECUTORS
}

static EXECUTORS: [ExecutorDefinition; 34] = [
    ExecutorDefinition {
        id: "passthrough",
        kind: "builtin",
        capabilities: &["lightflow.data.copy"],
        features: &[],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Always,
        recipe: ExecutionRecipe::Passthrough,
        data_policy: DataPolicy::JsonValues,
        atoms: &[("lightflow.atom.passthrough", "lightflow.data.copy")],
        plans_models: false,
        matcher: matches_passthrough,
    },
    ExecutorDefinition {
        id: PREVIEW_ENGINE,
        kind: "builtin",
        capabilities: &[IMAGE_GENERATE_CAPABILITY],
        features: &[],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Always,
        recipe: ExecutionRecipe::PreviewTextToImage,
        data_policy: DataPolicy::ArtifactHandles,
        atoms: &[
            ("lightflow.atom.prompt", "lightflow.text.prompt"),
            ("lightflow.atom.preview_pixels", IMAGE_GENERATE_CAPABILITY),
            ("lightflow.atom.save_image", "lightflow.artifact.image"),
        ],
        plans_models: true,
        matcher: matches_preview_text_to_image,
    },
    ExecutorDefinition {
        id: PREVIEW_EDIT_ENGINE,
        kind: "builtin",
        capabilities: &[IMAGE_EDIT_CAPABILITY],
        features: &[],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Always,
        recipe: ExecutionRecipe::PreviewImageEdit,
        data_policy: DataPolicy::ArtifactHandles,
        atoms: &[
            ("lightflow.atom.load_image", "lightflow.artifact.image"),
            ("lightflow.atom.preview_edit_pixels", IMAGE_EDIT_CAPABILITY),
            ("lightflow.atom.save_image", "lightflow.artifact.image"),
        ],
        plans_models: true,
        matcher: matches_preview_image_edit,
    },
    ExecutorDefinition {
        id: PREVIEW_INPAINT_ENGINE,
        kind: "builtin",
        capabilities: &[IMAGE_INPAINT_CAPABILITY],
        features: &[],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Always,
        recipe: ExecutionRecipe::PreviewInpaint,
        data_policy: DataPolicy::ArtifactHandles,
        atoms: &[
            ("lightflow.atom.load_image", "lightflow.artifact.image"),
            ("lightflow.atom.load_mask", "lightflow.artifact.mask"),
            (
                "lightflow.atom.preview_inpaint_pixels",
                IMAGE_INPAINT_CAPABILITY,
            ),
            ("lightflow.atom.save_image", "lightflow.artifact.image"),
        ],
        plans_models: true,
        matcher: matches_preview_inpaint,
    },
    ExecutorDefinition {
        id: "flux.runner.logical.v1",
        kind: "logical",
        capabilities: &[
            IMAGE_GENERATE_CAPABILITY,
            IMAGE_EDIT_CAPABILITY,
            IMAGE_INPAINT_CAPABILITY,
        ],
        features: &["flux"],
        env: None,
        command_env: None,
        visible: false,
        availability: ExecutorAvailability::Feature(cfg!(feature = "flux")),
        recipe: ExecutionRecipe::FluxTextToImage,
        data_policy: DataPolicy::DeviceResidentPreferred,
        atoms: &[
            ("lightflow.atom.load_flux_model", "lightflow.model.load"),
            ("lightflow.atom.load_text_encoder", "lightflow.model.load"),
            ("lightflow.atom.load_vae", "lightflow.model.load"),
            ("lightflow.atom.encode_prompt", "lightflow.text.encode"),
            ("lightflow.atom.sample_latents", IMAGE_GENERATE_CAPABILITY),
            ("lightflow.atom.decode_vae", "lightflow.image.decode"),
            ("lightflow.atom.save_image", "lightflow.artifact.image"),
        ],
        plans_models: true,
        matcher: matches_flux_text_to_image,
    },
    ExecutorDefinition {
        id: "flux.runner.logical.v1",
        kind: "logical",
        capabilities: &[IMAGE_EDIT_CAPABILITY],
        features: &["flux"],
        env: None,
        command_env: None,
        visible: false,
        availability: ExecutorAvailability::Feature(cfg!(feature = "flux")),
        recipe: ExecutionRecipe::FluxImageEdit,
        data_policy: DataPolicy::DeviceResidentPreferred,
        atoms: &[
            ("lightflow.atom.load_image", "lightflow.artifact.image"),
            ("lightflow.atom.load_flux_model", "lightflow.model.load"),
            ("lightflow.atom.load_text_encoder", "lightflow.model.load"),
            ("lightflow.atom.load_vae", "lightflow.model.load"),
            ("lightflow.atom.encode_prompt", "lightflow.text.encode"),
            ("lightflow.atom.sample_latents", IMAGE_EDIT_CAPABILITY),
            ("lightflow.atom.decode_vae", "lightflow.image.decode"),
            ("lightflow.atom.save_image", "lightflow.artifact.image"),
        ],
        plans_models: true,
        matcher: matches_flux_image_edit,
    },
    ExecutorDefinition {
        id: "flux.runner.logical.v1",
        kind: "logical",
        capabilities: &[IMAGE_INPAINT_CAPABILITY],
        features: &["flux"],
        env: None,
        command_env: None,
        visible: false,
        availability: ExecutorAvailability::Feature(cfg!(feature = "flux")),
        recipe: ExecutionRecipe::FluxInpaint,
        data_policy: DataPolicy::DeviceResidentPreferred,
        atoms: &[
            ("lightflow.atom.load_image", "lightflow.artifact.image"),
            ("lightflow.atom.load_mask", "lightflow.artifact.mask"),
            ("lightflow.atom.load_flux_model", "lightflow.model.load"),
            ("lightflow.atom.load_text_encoder", "lightflow.model.load"),
            ("lightflow.atom.load_vae", "lightflow.model.load"),
            ("lightflow.atom.encode_prompt", "lightflow.text.encode"),
            ("lightflow.atom.sample_latents", IMAGE_INPAINT_CAPABILITY),
            ("lightflow.atom.decode_vae", "lightflow.image.decode"),
            ("lightflow.atom.save_image", "lightflow.artifact.image"),
        ],
        plans_models: true,
        matcher: matches_flux_inpaint,
    },
    ExecutorDefinition {
        id: INVERT_ENGINE,
        kind: "builtin",
        capabilities: &[IMAGE_INVERT_CAPABILITY],
        features: &[],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Always,
        recipe: ExecutionRecipe::ImageInvert,
        data_policy: DataPolicy::ArtifactHandles,
        atoms: &[
            ("lightflow.atom.load_image", "lightflow.artifact.image"),
            ("lightflow.atom.invert_pixels", IMAGE_INVERT_CAPABILITY),
            ("lightflow.atom.save_image", "lightflow.artifact.image"),
        ],
        plans_models: false,
        matcher: matches_image_invert,
    },
    ExecutorDefinition {
        id: "builtin.image.load.v1",
        kind: "builtin",
        capabilities: &[IMAGE_LOAD_CAPABILITY],
        features: &[],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Always,
        recipe: ExecutionRecipe::ImageLoad,
        data_policy: DataPolicy::ArtifactHandles,
        atoms: &[("lightflow.atom.load_image", IMAGE_LOAD_CAPABILITY)],
        plans_models: false,
        matcher: matches_image_load,
    },
    ExecutorDefinition {
        id: "builtin.image.save.v1",
        kind: "builtin",
        capabilities: &[IMAGE_SAVE_CAPABILITY],
        features: &[],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Always,
        recipe: ExecutionRecipe::ImageSave,
        data_policy: DataPolicy::ArtifactHandles,
        atoms: &[("lightflow.atom.save_image", IMAGE_SAVE_CAPABILITY)],
        plans_models: false,
        matcher: matches_image_save,
    },
    ExecutorDefinition {
        id: "builtin.image.resize.v1",
        kind: "builtin",
        capabilities: &[IMAGE_RESIZE_CAPABILITY],
        features: &[],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Always,
        recipe: ExecutionRecipe::ImageResize,
        data_policy: DataPolicy::ArtifactHandles,
        atoms: &[("lightflow.atom.resize_pixels", IMAGE_RESIZE_CAPABILITY)],
        plans_models: false,
        matcher: matches_image_resize,
    },
    ExecutorDefinition {
        id: "builtin.image.crop.v1",
        kind: "builtin",
        capabilities: &[IMAGE_CROP_CAPABILITY],
        features: &[],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Always,
        recipe: ExecutionRecipe::ImageCrop,
        data_policy: DataPolicy::ArtifactHandles,
        atoms: &[("lightflow.atom.crop_pixels", IMAGE_CROP_CAPABILITY)],
        plans_models: false,
        matcher: matches_image_crop,
    },
    ExecutorDefinition {
        id: "builtin.image.upscale.v1",
        kind: "builtin",
        capabilities: &[IMAGE_UPSCALE_CAPABILITY],
        features: &[],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Always,
        recipe: ExecutionRecipe::ImageUpscale,
        data_policy: DataPolicy::ArtifactHandles,
        atoms: &[("lightflow.atom.upscale_pixels", IMAGE_UPSCALE_CAPABILITY)],
        plans_models: false,
        matcher: matches_image_upscale,
    },
    ExecutorDefinition {
        id: "builtin.mask.compose.v1",
        kind: "builtin",
        capabilities: &[MASK_COMPOSE_CAPABILITY],
        features: &[],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Always,
        recipe: ExecutionRecipe::MaskCompose,
        data_policy: DataPolicy::ArtifactHandles,
        atoms: &[("lightflow.atom.compose_masks", MASK_COMPOSE_CAPABILITY)],
        plans_models: false,
        matcher: matches_mask_compose,
    },
    ExecutorDefinition {
        id: LLM_MOCK_ENGINE,
        kind: "builtin",
        capabilities: &[LLM_GENERATE_CAPABILITY],
        features: &[],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Always,
        recipe: ExecutionRecipe::BuiltinLlmGenerate,
        data_policy: DataPolicy::JsonValues,
        atoms: &[("lightflow.atom.mock_llm_generate", LLM_GENERATE_CAPABILITY)],
        plans_models: false,
        matcher: matches_builtin_llm_generate,
    },
    ExecutorDefinition {
        id: "rig-core",
        kind: "native",
        capabilities: &[LLM_GENERATE_CAPABILITY],
        features: &["rig"],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Feature(cfg!(feature = "rig")),
        recipe: ExecutionRecipe::RigLlmGenerate,
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
        matcher: matches_rig_llm,
    },
    ExecutorDefinition {
        id: "builtin.text.concat.v1",
        kind: "builtin",
        capabilities: &[TEXT_CONCAT_CAPABILITY],
        features: &[],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Always,
        recipe: ExecutionRecipe::TextConcat,
        data_policy: DataPolicy::JsonValues,
        atoms: &[("lightflow.atom.concat_text", TEXT_CONCAT_CAPABILITY)],
        plans_models: false,
        matcher: matches_text_concat,
    },
    ExecutorDefinition {
        id: "builtin.text.template.v1",
        kind: "builtin",
        capabilities: &[TEXT_TEMPLATE_CAPABILITY],
        features: &[],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Always,
        recipe: ExecutionRecipe::TextTemplate,
        data_policy: DataPolicy::JsonValues,
        atoms: &[("lightflow.atom.render_template", TEXT_TEMPLATE_CAPABILITY)],
        plans_models: false,
        matcher: matches_text_template,
    },
    ExecutorDefinition {
        id: "builtin.text.regex.v1",
        kind: "builtin",
        capabilities: &[TEXT_REGEX_CAPABILITY],
        features: &[],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Always,
        recipe: ExecutionRecipe::TextRegex,
        data_policy: DataPolicy::JsonValues,
        atoms: &[("lightflow.atom.regex_text", TEXT_REGEX_CAPABILITY)],
        plans_models: false,
        matcher: matches_text_regex,
    },
    ExecutorDefinition {
        id: "builtin.json.extract.v1",
        kind: "builtin",
        capabilities: &[JSON_EXTRACT_CAPABILITY],
        features: &[],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Always,
        recipe: ExecutionRecipe::JsonExtract,
        data_policy: DataPolicy::JsonValues,
        atoms: &[("lightflow.atom.extract_json_path", JSON_EXTRACT_CAPABILITY)],
        plans_models: false,
        matcher: matches_json_extract,
    },
    ExecutorDefinition {
        id: "builtin.control.if.v1",
        kind: "builtin",
        capabilities: &[CONTROL_IF_CAPABILITY],
        features: &[],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Always,
        recipe: ExecutionRecipe::ControlIf,
        data_policy: DataPolicy::JsonValues,
        atoms: &[("lightflow.atom.if_value", CONTROL_IF_CAPABILITY)],
        plans_models: false,
        matcher: matches_control_if,
    },
    ExecutorDefinition {
        id: "builtin.control.switch.v1",
        kind: "builtin",
        capabilities: &[CONTROL_SWITCH_CAPABILITY],
        features: &[],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Always,
        recipe: ExecutionRecipe::ControlSwitch,
        data_policy: DataPolicy::JsonValues,
        atoms: &[("lightflow.atom.switch_value", CONTROL_SWITCH_CAPABILITY)],
        plans_models: false,
        matcher: matches_control_switch,
    },
    ExecutorDefinition {
        id: "builtin.control.merge.v1",
        kind: "builtin",
        capabilities: &[CONTROL_MERGE_CAPABILITY],
        features: &[],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Always,
        recipe: ExecutionRecipe::ControlMerge,
        data_policy: DataPolicy::JsonValues,
        atoms: &[("lightflow.atom.merge_values", CONTROL_MERGE_CAPABILITY)],
        plans_models: false,
        matcher: matches_control_merge,
    },
    ExecutorDefinition {
        id: "builtin.control.split.v1",
        kind: "builtin",
        capabilities: &[CONTROL_SPLIT_CAPABILITY],
        features: &[],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Always,
        recipe: ExecutionRecipe::ControlSplit,
        data_policy: DataPolicy::JsonValues,
        atoms: &[("lightflow.atom.split_value", CONTROL_SPLIT_CAPABILITY)],
        plans_models: false,
        matcher: matches_control_split,
    },
    ExecutorDefinition {
        id: "builtin.model.select.v1",
        kind: "builtin",
        capabilities: &[MODEL_SELECT_CAPABILITY],
        features: &[],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Always,
        recipe: ExecutionRecipe::ModelSelect,
        data_policy: DataPolicy::JsonValues,
        atoms: &[(
            "lightflow.atom.select_model_variant",
            MODEL_SELECT_CAPABILITY,
        )],
        plans_models: false,
        matcher: matches_model_select,
    },
    ExecutorDefinition {
        id: "builtin.model.lock.check.v1",
        kind: "builtin",
        capabilities: &[MODEL_LOCK_CHECK_CAPABILITY],
        features: &[],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Always,
        recipe: ExecutionRecipe::ModelLockCheck,
        data_policy: DataPolicy::JsonValues,
        atoms: &[(
            "lightflow.atom.check_model_lock",
            MODEL_LOCK_CHECK_CAPABILITY,
        )],
        plans_models: false,
        matcher: matches_model_lock_check,
    },
    ExecutorDefinition {
        id: "builtin.llm.classify.v1",
        kind: "builtin",
        capabilities: &[LLM_CLASSIFY_CAPABILITY],
        features: &[],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Always,
        recipe: ExecutionRecipe::LlmClassify,
        data_policy: DataPolicy::JsonValues,
        atoms: &[("lightflow.atom.classify_text", LLM_CLASSIFY_CAPABILITY)],
        plans_models: false,
        matcher: matches_llm_classify,
    },
    ExecutorDefinition {
        id: "builtin.llm.structured_output.v1",
        kind: "builtin",
        capabilities: &[LLM_STRUCTURED_OUTPUT_CAPABILITY],
        features: &[],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Always,
        recipe: ExecutionRecipe::LlmStructuredOutput,
        data_policy: DataPolicy::JsonValues,
        atoms: &[(
            "lightflow.atom.structured_output",
            LLM_STRUCTURED_OUTPUT_CAPABILITY,
        )],
        plans_models: false,
        matcher: matches_llm_structured_output,
    },
    ExecutorDefinition {
        id: "flux2-klein.gguf.runner.v1",
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
        availability: ExecutorAvailability::EnvPresent("LIGHTFLOW_FLUX_RUNNER"),
        recipe: ExecutionRecipe::FluxTextToImage,
        data_policy: DataPolicy::DeviceResidentPreferred,
        atoms: &[],
        plans_models: true,
        matcher: matches_never,
    },
    ExecutorDefinition {
        id: "diffusion-rs.native.v1",
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
        availability: ExecutorAvailability::Feature(cfg!(feature = "flux-native")),
        recipe: ExecutionRecipe::FluxTextToImage,
        data_policy: DataPolicy::DeviceResidentPreferred,
        atoms: &[],
        plans_models: true,
        matcher: matches_never,
    },
    ExecutorDefinition {
        id: "lightflow.command.executor.v1",
        kind: "reserved",
        capabilities: &["lightflow.command"],
        features: &[],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Unavailable,
        recipe: ExecutionRecipe::Passthrough,
        data_policy: DataPolicy::JsonValues,
        atoms: &[],
        plans_models: false,
        matcher: matches_never,
    },
    ExecutorDefinition {
        id: "lightflow.python.node.executor.v1",
        kind: "reserved",
        capabilities: &["lightflow.python.node"],
        features: &[],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Unavailable,
        recipe: ExecutionRecipe::Passthrough,
        data_policy: DataPolicy::JsonValues,
        atoms: &[],
        plans_models: false,
        matcher: matches_never,
    },
    ExecutorDefinition {
        id: "lightflow.onnx.executor.v1",
        kind: "reserved",
        capabilities: &["lightflow.onnx"],
        features: &[],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Unavailable,
        recipe: ExecutionRecipe::Passthrough,
        data_policy: DataPolicy::JsonValues,
        atoms: &[],
        plans_models: false,
        matcher: matches_never,
    },
    ExecutorDefinition {
        id: "lightflow.candle.executor.v1",
        kind: "reserved",
        capabilities: &["lightflow.candle"],
        features: &["gguf"],
        env: None,
        command_env: None,
        visible: true,
        availability: ExecutorAvailability::Unavailable,
        recipe: ExecutionRecipe::Passthrough,
        data_policy: DataPolicy::JsonValues,
        atoms: &[],
        plans_models: true,
        matcher: matches_never,
    },
];

fn matches_passthrough(workflow: &WorkflowSpec) -> bool {
    workflow.runtimes.is_empty()
}

fn matches_preview_text_to_image(workflow: &WorkflowSpec) -> bool {
    workflow.runtimes.iter().any(|runtime| {
        runtime.capability == IMAGE_GENERATE_CAPABILITY
            && runtime.engine.as_deref() == Some(PREVIEW_ENGINE)
    })
}

fn matches_preview_image_edit(workflow: &WorkflowSpec) -> bool {
    workflow.runtimes.iter().any(|runtime| {
        runtime.capability == IMAGE_EDIT_CAPABILITY
            && runtime.engine.as_deref() == Some(PREVIEW_EDIT_ENGINE)
    })
}

fn matches_preview_inpaint(workflow: &WorkflowSpec) -> bool {
    workflow.runtimes.iter().any(|runtime| {
        runtime.capability == IMAGE_INPAINT_CAPABILITY
            && runtime.engine.as_deref() == Some(PREVIEW_INPAINT_ENGINE)
    })
}

fn matches_flux_text_to_image(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, IMAGE_GENERATE_CAPABILITY)
        && super::flux::workflow_declares_flux_assets(workflow)
}

fn matches_flux_image_edit(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, IMAGE_EDIT_CAPABILITY)
        && super::flux::workflow_declares_flux_assets(workflow)
}

fn matches_flux_inpaint(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, IMAGE_INPAINT_CAPABILITY)
        && super::flux::workflow_declares_flux_assets(workflow)
}

fn matches_image_invert(workflow: &WorkflowSpec) -> bool {
    workflow.runtimes.iter().any(|runtime| {
        runtime.capability == IMAGE_INVERT_CAPABILITY
            && runtime.engine.as_deref() == Some(INVERT_ENGINE)
    })
}

fn matches_image_load(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, IMAGE_LOAD_CAPABILITY)
}

fn matches_image_save(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, IMAGE_SAVE_CAPABILITY)
}

fn matches_image_resize(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, IMAGE_RESIZE_CAPABILITY)
}

fn matches_image_crop(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, IMAGE_CROP_CAPABILITY)
}

fn matches_image_upscale(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, IMAGE_UPSCALE_CAPABILITY)
}

fn matches_mask_compose(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, MASK_COMPOSE_CAPABILITY)
}

fn matches_builtin_llm_generate(workflow: &WorkflowSpec) -> bool {
    workflow.runtimes.iter().any(|runtime| {
        runtime.capability == LLM_GENERATE_CAPABILITY
            && runtime.engine.as_deref() == Some(LLM_MOCK_ENGINE)
    })
}

fn matches_rig_llm(workflow: &WorkflowSpec) -> bool {
    workflow.runtimes.iter().any(|runtime| {
        runtime.capability == LLM_GENERATE_CAPABILITY
            && runtime.engine.as_deref() != Some(LLM_MOCK_ENGINE)
    })
}

fn matches_text_concat(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, TEXT_CONCAT_CAPABILITY)
}

fn matches_text_template(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, TEXT_TEMPLATE_CAPABILITY)
}

fn matches_text_regex(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, TEXT_REGEX_CAPABILITY)
}

fn matches_json_extract(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, JSON_EXTRACT_CAPABILITY)
}

fn matches_control_if(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, CONTROL_IF_CAPABILITY)
}

fn matches_control_switch(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, CONTROL_SWITCH_CAPABILITY)
}

fn matches_control_merge(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, CONTROL_MERGE_CAPABILITY)
}

fn matches_control_split(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, CONTROL_SPLIT_CAPABILITY)
}

fn matches_model_select(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, MODEL_SELECT_CAPABILITY)
}

fn matches_model_lock_check(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, MODEL_LOCK_CHECK_CAPABILITY)
}

fn matches_llm_classify(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, LLM_CLASSIFY_CAPABILITY)
}

fn matches_llm_structured_output(workflow: &WorkflowSpec) -> bool {
    workflow_has_capability(workflow, LLM_STRUCTURED_OUTPUT_CAPABILITY)
}

fn matches_never(_workflow: &WorkflowSpec) -> bool {
    false
}

fn workflow_has_capability(workflow: &WorkflowSpec, capability: &str) -> bool {
    workflow
        .runtimes
        .iter()
        .any(|runtime| runtime.capability == capability)
}
