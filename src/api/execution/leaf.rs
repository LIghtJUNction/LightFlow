use super::artifacts::image_artifact;
use super::media;
use super::types::LeafExecution;
use super::{image, text};
use crate::api::model_manager::ModelManager;
use crate::api::plan::{ExecutionPlan, ExecutionPlanNode, ExecutionRecipe};
use crate::api::{ApiError, ApiResult};
use crate::api::{comfyui, executors, flux, llm_rig};
use crate::workflow::{ExecutionRuntime, WorkflowSpec};
use std::path::Path;

pub(super) fn execute_leaf_workflow(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
    model_manager: &mut ModelManager,
) -> ApiResult<LeafExecution> {
    let plan = crate::api::plan::build_leaf_execution_plan(workflow)?;
    execute_leaf_plan(root, workflow, inputs, model_manager, &plan)
}

pub(super) fn execute_leaf_plan(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
    model_manager: &mut ModelManager,
    plan: &ExecutionPlan,
) -> ApiResult<LeafExecution> {
    let runtime = execution_runtime(workflow, &plan.node);

    let mut replay_fingerprint = None;
    let mut leaf = match plan.node.recipe {
        ExecutionRecipe::ComfyUiWorkflow => {
            let result = comfyui::execute(root, workflow, inputs)?;
            replay_fingerprint = Some(result.replay_fingerprint);
            Ok(LeafExecution {
                outputs: result.outputs,
                runtime: None,
                artifacts: result.artifacts,
            })
        }
        ExecutionRecipe::PreviewTextToImage => {
            execute_preview_text_to_image(root, workflow, inputs)
        }
        ExecutionRecipe::FluxTextToImage => {
            let result = flux::execute_flux_text_to_image(root, workflow, inputs, model_manager)?;
            Ok(LeafExecution {
                outputs: result.outputs,
                runtime: None,
                artifacts: result.artifacts,
            })
        }
        ExecutionRecipe::FluxImageEdit => {
            let result = flux::execute_flux_image_edit(root, workflow, inputs, model_manager)?;
            Ok(LeafExecution {
                outputs: result.outputs,
                runtime: None,
                artifacts: result.artifacts,
            })
        }
        ExecutionRecipe::FluxInpaint => {
            let result = flux::execute_flux_inpaint(root, workflow, inputs, model_manager)?;
            Ok(LeafExecution {
                outputs: result.outputs,
                runtime: None,
                artifacts: result.artifacts,
            })
        }
        ExecutionRecipe::ImageInvert => image::execute_image_invert(root, workflow, inputs),
        ExecutionRecipe::ImageLoad => image::execute_image_load(workflow, inputs),
        ExecutionRecipe::ImageSave => image::execute_image_save(root, workflow, inputs),
        ExecutionRecipe::ImageResize => image::execute_image_resize(root, workflow, inputs),
        ExecutionRecipe::ImageCrop => image::execute_image_crop(root, workflow, inputs),
        ExecutionRecipe::PreviewImageEdit => {
            image::execute_preview_image_edit(root, workflow, inputs)
        }
        ExecutionRecipe::PreviewInpaint => image::execute_preview_inpaint(root, workflow, inputs),
        ExecutionRecipe::ImageUpscale => image::execute_image_upscale(root, workflow, inputs),
        ExecutionRecipe::MaskCompose => image::execute_mask_compose(root, workflow, inputs),
        ExecutionRecipe::RigLlmGenerate => {
            let outputs = llm_rig::execute_rig_llm(workflow, inputs)?;
            Ok(LeafExecution {
                outputs,
                runtime: None,
                artifacts: Vec::new(),
            })
        }
        ExecutionRecipe::BuiltinLlmGenerate => text::execute_builtin_llm_generate(workflow, inputs),
        ExecutionRecipe::TextConcat => text::execute_text_concat(workflow, inputs),
        ExecutionRecipe::TextTemplate => text::execute_text_template(workflow, inputs),
        ExecutionRecipe::TextRegex => text::execute_text_regex(workflow, inputs),
        ExecutionRecipe::JsonExtract => text::execute_json_extract(workflow, inputs),
        ExecutionRecipe::ControlIf => text::execute_control_if(workflow, inputs),
        ExecutionRecipe::ControlSwitch => text::execute_control_switch(workflow, inputs),
        ExecutionRecipe::ControlMerge => text::execute_control_merge(workflow, inputs),
        ExecutionRecipe::ControlSplit => text::execute_control_split(workflow, inputs),
        ExecutionRecipe::ModelSelect => text::execute_model_select(workflow, inputs),
        ExecutionRecipe::ModelLockCheck => text::execute_model_lock_check(root, workflow, inputs),
        ExecutionRecipe::LlmClassify => text::execute_llm_classify(workflow, inputs),
        ExecutionRecipe::LlmStructuredOutput => {
            text::execute_llm_structured_output(workflow, inputs)
        }
        ExecutionRecipe::Passthrough => Ok(LeafExecution {
            outputs: media::execute_passthrough_ports(&workflow.outputs, inputs),
            runtime: None,
            artifacts: Vec::new(),
        }),
    }?;

    let mut runtime = runtime;
    runtime.replay_fingerprint = replay_fingerprint;
    leaf.runtime = Some(runtime);
    Ok(leaf)
}

fn execution_runtime(workflow: &WorkflowSpec, node: &ExecutionPlanNode) -> ExecutionRuntime {
    ExecutionRuntime {
        executor_id: node.executor_id.clone(),
        executor_kind: node.executor_kind.clone(),
        capabilities: node.capabilities.clone(),
        data_policy: executors::data_policy_name(node.data_policy).to_owned(),
        declared: workflow.runtimes.clone(),
        replay_fingerprint: None,
    }
}

fn execute_preview_text_to_image(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let prompt = media::input_string(inputs, "prompt")
        .or_else(|| media::input_string(inputs, "text"))
        .or_else(|| media::input_string(inputs, "positive"))
        .unwrap_or_default();
    let width = media::input_u32(inputs, "width")
        .unwrap_or(512)
        .clamp(64, 2048);
    let height = media::input_u32(inputs, "height")
        .unwrap_or(512)
        .clamp(64, 2048);
    let seed = media::input_u64(inputs, "seed").unwrap_or_else(|| media::stable_seed(&prompt));
    let path = media::output_path(root, workflow, inputs, seed);

    super::png::write_preview_png(&path, width, height, seed, &prompt).map_err(|error| {
        ApiError::InvalidRequest(format!("failed to write preview image: {error}"))
    })?;

    let artifact = image_artifact(workflow, &path, &prompt, width, height, seed, inputs);
    let artifact_value = serde_json::to_value(&artifact)
        .map_err(|error| ApiError::InvalidRequest(error.to_string()))?;
    let mut outputs = serde_json::Map::new();

    for output in &workflow.outputs {
        let value = match output.name.as_str() {
            "image" | "artifact" => artifact_value.clone(),
            "image_path" | "output_path" => serde_json::Value::String(artifact.path.clone()),
            "prompt" => serde_json::Value::String(prompt.clone()),
            "width" => width.into(),
            "height" => height.into(),
            "seed" => seed.into(),
            other => inputs
                .get(other)
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        };
        outputs.insert(output.name.clone(), value);
    }

    Ok(LeafExecution {
        outputs,
        runtime: None,
        artifacts: vec![artifact],
    })
}
