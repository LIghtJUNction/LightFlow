use super::{ApiError, ApiResult};
use crate::api::model_manager::ModelManager;
use crate::workflow::{WorkflowArtifact, WorkflowSpec};
use std::fs;
use std::path::Path;

mod backend;
mod io_contract;
mod types;
use backend::{run_flux_native_batch, run_selected_flux_backend, selected_flux_backend};
use io_contract::{
    flux_artifact, input_count, input_f32, input_i32, input_i64, input_string, output_paths,
    required_input_path,
};
use types::{FluxBackend, FluxBatchRunRequest, FluxModelHandles, FluxRunRequest, FluxTask};

#[derive(Debug, Clone, PartialEq)]
pub(super) struct FluxExecution {
    pub(super) outputs: serde_json::Map<String, serde_json::Value>,
    pub(super) artifacts: Vec<WorkflowArtifact>,
}

pub(super) fn workflow_declares_flux_assets(workflow: &WorkflowSpec) -> bool {
    has_model_requirement(workflow, "flux_model")
        && has_model_requirement(workflow, "llm_model")
        && has_model_requirement(workflow, "vae_model")
}

fn has_model_requirement(workflow: &WorkflowSpec, id: &str) -> bool {
    workflow.models.iter().any(|model| model.id == id)
}

pub(super) fn execute_flux_text_to_image(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
    model_manager: &mut ModelManager,
) -> ApiResult<FluxExecution> {
    let models = load_flux_models(&workflow.id, model_manager)?;
    execute_flux_with_models(root, workflow, inputs, models, FluxTask::TextToImage)
}

pub(super) fn execute_flux_image_edit(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
    model_manager: &mut ModelManager,
) -> ApiResult<FluxExecution> {
    let models = load_flux_models(&workflow.id, model_manager)?;
    execute_flux_with_models(root, workflow, inputs, models, FluxTask::ImageEdit)
}

pub(super) fn execute_flux_inpaint(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
    model_manager: &mut ModelManager,
) -> ApiResult<FluxExecution> {
    let models = load_flux_models(&workflow.id, model_manager)?;
    execute_flux_with_models(root, workflow, inputs, models, FluxTask::Inpaint)
}

fn execute_flux_with_models(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
    models: FluxModelHandles,
    task: FluxTask,
) -> ApiResult<FluxExecution> {
    let prompt = input_string(inputs, "prompt")
        .or_else(|| input_string(inputs, "text"))
        .ok_or_else(|| {
            ApiError::InvalidRequest("prompt is required for FLUX generation".to_owned())
        })?;
    let negative = input_string(inputs, "negative").unwrap_or_default();
    let width = input_i32(inputs, "width").unwrap_or(512).clamp(64, 2048);
    let height = input_i32(inputs, "height").unwrap_or(512).clamp(64, 2048);
    let steps = input_i32(inputs, "steps").unwrap_or(4).clamp(1, 64);
    let seed = input_i64(inputs, "seed").unwrap_or(42);
    let count = input_count(inputs);
    let guidance = input_f32(inputs, "guidance").unwrap_or(3.5);
    let cfg_scale = input_f32(inputs, "cfg_scale").unwrap_or(1.0);
    let strength = input_f32(inputs, "strength").unwrap_or(task.default_strength());
    let image_path = match task {
        FluxTask::TextToImage => None,
        FluxTask::ImageEdit | FluxTask::Inpaint => Some(required_input_path(inputs, "image_path")?),
    };
    let mask_path = match task {
        FluxTask::Inpaint => Some(required_input_path(inputs, "mask_path")?),
        FluxTask::TextToImage | FluxTask::ImageEdit => None,
    };
    let output_paths = output_paths(root, workflow, inputs, seed as u64, count);
    let mut artifacts = Vec::with_capacity(output_paths.len());
    let backend = selected_flux_backend()?;

    for output_path in &output_paths {
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent).map_err(ApiError::from)?;
        }
    }

    if matches!(backend, FluxBackend::Native)
        && matches!(task, FluxTask::TextToImage)
        && image_path.is_none()
        && mask_path.is_none()
    {
        let request = FluxBatchRunRequest {
            task,
            prompt: &prompt,
            negative: &negative,
            width,
            height,
            seed,
            steps,
            guidance,
            cfg_scale,
            strength,
            output_paths: &output_paths,
            models: &models,
        };
        run_flux_native_batch(&request)?;
    } else {
        for (offset, output_path) in output_paths.iter().enumerate() {
            let request = FluxRunRequest {
                task,
                prompt: &prompt,
                negative: &negative,
                width,
                height,
                seed: seed.saturating_add(offset as i64),
                steps,
                guidance,
                cfg_scale,
                strength,
                image_path: image_path.as_deref(),
                mask_path: mask_path.as_deref(),
                output_path,
                models: &models,
            };
            run_selected_flux_backend(&request, backend)?;
        }
    }

    for (offset, output_path) in output_paths.iter().enumerate() {
        let request = FluxRunRequest {
            task,
            prompt: &prompt,
            negative: &negative,
            width,
            height,
            seed: seed.saturating_add(offset as i64),
            steps,
            guidance,
            cfg_scale,
            strength,
            image_path: image_path.as_deref(),
            mask_path: mask_path.as_deref(),
            output_path,
            models: &models,
        };
        artifacts.push(flux_artifact(
            workflow,
            &request,
            backend,
            offset + 1,
            count,
        ));
    }

    let mut outputs = serde_json::Map::new();
    let first_artifact = artifacts.first().cloned().ok_or_else(|| {
        ApiError::InvalidRequest("FLUX generation produced no artifacts".to_owned())
    })?;
    outputs.insert(
        "image".to_owned(),
        serde_json::to_value(&first_artifact).unwrap(),
    );
    outputs.insert("image_path".to_owned(), first_artifact.path.clone().into());
    outputs.insert(
        "images".to_owned(),
        serde_json::to_value(&artifacts).unwrap(),
    );
    outputs.insert(
        "image_paths".to_owned(),
        serde_json::Value::Array(
            artifacts
                .iter()
                .map(|artifact| artifact.path.clone().into())
                .collect(),
        ),
    );

    Ok(FluxExecution { outputs, artifacts })
}

fn load_flux_models(
    workflow_id: &str,
    model_manager: &mut ModelManager,
) -> ApiResult<FluxModelHandles> {
    Ok(FluxModelHandles {
        diffusion_model: model_manager.locked_path_with_format(
            workflow_id,
            "flux_model",
            "gguf",
        )?,
        llm: model_manager.locked_path_with_format(workflow_id, "llm_model", "gguf")?,
        vae: model_manager.locked_path_with_format(workflow_id, "vae_model", "safetensors")?,
    })
}
