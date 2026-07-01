use crate::workflow::RuntimeRequirement;
use serde::Serialize;

pub(in crate::api) const IMAGE_GENERATE_CAPABILITY: &str = "lightflow.image.generate";
pub(in crate::api) const IMAGE_EDIT_CAPABILITY: &str = "lightflow.image.edit";
pub(in crate::api) const IMAGE_INPAINT_CAPABILITY: &str = "lightflow.image.inpaint";
pub(in crate::api) const IMAGE_INVERT_CAPABILITY: &str = "lightflow.image.invert";
pub(in crate::api) const IMAGE_LOAD_CAPABILITY: &str = "lightflow.image.load";
pub(in crate::api) const IMAGE_SAVE_CAPABILITY: &str = "lightflow.image.save";
pub(in crate::api) const IMAGE_RESIZE_CAPABILITY: &str = "lightflow.image.resize";
pub(in crate::api) const IMAGE_CROP_CAPABILITY: &str = "lightflow.image.crop";
pub(in crate::api) const LLM_GENERATE_CAPABILITY: &str = "lightflow.llm.generate";
pub(in crate::api) const TEXT_CONCAT_CAPABILITY: &str = "lightflow.text.concat";
pub(in crate::api) const TEXT_TEMPLATE_CAPABILITY: &str = "lightflow.text.template";
pub(in crate::api) const TEXT_REGEX_CAPABILITY: &str = "lightflow.text.regex";
pub(in crate::api) const JSON_EXTRACT_CAPABILITY: &str = "lightflow.json.extract";
pub(in crate::api) const CONTROL_IF_CAPABILITY: &str = "lightflow.control.if";
pub(in crate::api) const CONTROL_SWITCH_CAPABILITY: &str = "lightflow.control.switch";
pub(in crate::api) const CONTROL_MERGE_CAPABILITY: &str = "lightflow.control.merge";
pub(in crate::api) const CONTROL_SPLIT_CAPABILITY: &str = "lightflow.control.split";
pub(in crate::api) const MODEL_SELECT_CAPABILITY: &str = "lightflow.model.select";
pub(in crate::api) const MODEL_LOCK_CHECK_CAPABILITY: &str = "lightflow.model.lock.check";
pub(in crate::api) const IMAGE_UPSCALE_CAPABILITY: &str = "lightflow.image.upscale";
pub(in crate::api) const MASK_COMPOSE_CAPABILITY: &str = "lightflow.mask.compose";
pub(in crate::api) const LLM_CLASSIFY_CAPABILITY: &str = "lightflow.llm.classify";
pub(in crate::api) const LLM_STRUCTURED_OUTPUT_CAPABILITY: &str = "lightflow.llm.structured_output";
pub(in crate::api) const PREVIEW_ENGINE: &str = "builtin.preview.v1";
pub(in crate::api) const PREVIEW_EDIT_ENGINE: &str = "builtin.preview.edit.v1";
pub(in crate::api) const PREVIEW_INPAINT_ENGINE: &str = "builtin.preview.inpaint.v1";
pub(in crate::api) const INVERT_ENGINE: &str = "builtin.image.invert.v1";
pub(in crate::api) const LLM_MOCK_ENGINE: &str = "builtin.llm.mock.v1";

#[derive(Debug, Clone, Eq, PartialEq)]
pub(in crate::api) struct ExecutionPlan {
    pub(in crate::api) workflow_id: String,
    pub(in crate::api) node: ExecutionPlanNode,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(in crate::api) struct ExecutionPlanNode {
    pub(in crate::api) id: String,
    pub(in crate::api) executor_id: String,
    pub(in crate::api) executor_kind: String,
    pub(in crate::api) executor_status: String,
    pub(in crate::api) executor_status_reason: String,
    pub(in crate::api) executor_available: bool,
    pub(in crate::api) capabilities: Vec<String>,
    pub(in crate::api) plans_models: bool,
    pub(in crate::api) recipe: ExecutionRecipe,
    pub(in crate::api) atoms: Vec<ExecutionAtom>,
    pub(in crate::api) models: Vec<PlannedModel>,
    pub(in crate::api) data_policy: DataPolicy,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(in crate::api) enum ExecutionRecipe {
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
pub(in crate::api) struct ExecutionAtom {
    pub(in crate::api) id: String,
    pub(in crate::api) capability: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(in crate::api) struct PlannedModel {
    pub(in crate::api) requirement_id: String,
    pub(in crate::api) capability: String,
    pub(in crate::api) preferred_format: Option<String>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(in crate::api) enum DataPolicy {
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
