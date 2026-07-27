use crate::workflow::RuntimeRequirement;
use serde::Serialize;

pub(in crate::api) const IMAGE_GENERATE_CAPABILITY: &str = "lightflow.image.generate";
pub(in crate::api) const IMAGE_EDIT_CAPABILITY: &str = "lightflow.image.edit";
pub(in crate::api) const IMAGE_INPAINT_CAPABILITY: &str = "lightflow.image.inpaint";
pub(in crate::api) const LLM_GENERATE_CAPABILITY: &str = "lightflow.llm.generate";
pub(in crate::api) const COMFYUI_WORKFLOW_CAPABILITY: &str = "lightflow.comfyui.workflow";
pub(in crate::api) const COMMAND_RUN_CAPABILITY: &str = "lightflow.command.run";
pub(in crate::api) const RUNNER_CAPABILITY: &str = "lightflow.runner";
pub(in crate::api) const PREVIEW_ENGINE: &str = "builtin.preview.v1";
pub(in crate::api) const PREVIEW_EDIT_ENGINE: &str = "builtin.preview.edit.v1";
pub(in crate::api) const PREVIEW_INPAINT_ENGINE: &str = "builtin.preview.inpaint.v1";
pub(in crate::api) const FLUX_NATIVE_ENGINE: &str = "diffusion-rs.native.v1";
pub(in crate::api) const FLUX_EXTERNAL_ENGINE: &str = "flux2-klein.gguf.runner.v1";
pub(in crate::api) const COMFYUI_API_ENGINE: &str = "comfyui.api.v1";
pub(in crate::api) const COMMAND_ENGINE: &str = "process.command.v1";
pub(in crate::api) const RUNNER_ENGINE: &str = "runner.v1";

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
    Unavailable,
    Runner,
    ExternalCommand,
    ComfyUiWorkflow,
    PreviewTextToImage,
    PreviewImageEdit,
    PreviewInpaint,
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
