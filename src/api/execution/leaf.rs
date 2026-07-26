use super::artifacts::image_artifact;
use super::image;
use super::media;
use super::types::LeafExecution;
use crate::api::model_manager::ModelManager;
use crate::api::plan::{ExecutionPlan, ExecutionPlanNode, ExecutionRecipe};
use crate::api::source::WorkflowOrigin;
use crate::api::{ApiError, ApiResult};
use crate::api::{comfyui, command_runner, executors, runner_process};
use crate::workflow::{ExecutionRuntime, WorkflowSpec};
use std::path::Path;

pub(super) fn execute_leaf_workflow(
    root: &Path,
    workflow: &WorkflowSpec,
    origin: Option<&WorkflowOrigin>,
    inputs: &serde_json::Map<String, serde_json::Value>,
    _model_manager: &mut ModelManager,
) -> ApiResult<LeafExecution> {
    let plan = crate::api::plan::build_leaf_execution_plan(workflow)?;
    execute_leaf_plan(root, workflow, origin, inputs, _model_manager, &plan)
}

pub(super) fn execute_leaf_plan(
    root: &Path,
    workflow: &WorkflowSpec,
    origin: Option<&WorkflowOrigin>,
    inputs: &serde_json::Map<String, serde_json::Value>,
    _model_manager: &mut ModelManager,
    plan: &ExecutionPlan,
) -> ApiResult<LeafExecution> {
    let runtime = execution_runtime(workflow, &plan.node);

    let mut replay_fingerprint = None;
    let mut leaf = match plan.node.recipe {
        ExecutionRecipe::Unavailable => Err(ApiError::InvalidRequest(format!(
            "workflow {} selected reserved engine {}, which is not runnable in this build",
            workflow.id, plan.node.executor_id
        ))),
        ExecutionRecipe::Runner => {
            let origin = origin.ok_or_else(|| {
                ApiError::InvalidRequest(format!(
                    "workflow {} selected runner without a discovered runner origin",
                    workflow.id
                ))
            })?;
            let result = runner_process::execute(root, workflow, inputs, origin)?;
            replay_fingerprint = result.replay_fingerprint;
            Ok(LeafExecution {
                outputs: result.outputs,
                runtime: None,
                artifacts: result.artifacts,
            })
        }
        ExecutionRecipe::ExternalCommand => {
            let result = command_runner::execute(root, workflow, inputs)?;
            replay_fingerprint = result.replay_fingerprint;
            Ok(LeafExecution {
                outputs: result.outputs,
                runtime: None,
                artifacts: result.artifacts,
            })
        }
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
        ExecutionRecipe::PreviewImageEdit => {
            image::execute_preview_image_edit(root, workflow, inputs)
        }
        ExecutionRecipe::PreviewInpaint => image::execute_preview_inpaint(root, workflow, inputs),
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::workflow_with_identity;

    #[test]
    fn reserved_legacy_engine_fails_clearly_at_execution() {
        let root = tempfile::tempdir().expect("tempdir");
        let workflow = workflow_with_identity("lightflow.legacy_concat", "0.1.0")
            .builtin_runtime("legacy", "lightflow.text.concat", "builtin.text.concat.v1")
            .build();
        let mut models = ModelManager::new(root.path());

        let error = execute_leaf_workflow(
            root.path(),
            &workflow,
            None,
            &serde_json::Map::new(),
            &mut models,
        )
        .expect_err("reserved engine cannot execute");

        assert!(error.message().contains("reserved engine"));
        assert!(error.message().contains("builtin.text.concat.v1"));
    }
}
