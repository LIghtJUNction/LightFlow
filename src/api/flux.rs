use super::{ApiError, ApiResult};
use crate::api::model_manager::ModelManager;
use crate::api::plan::{
    IMAGE_EDIT_CAPABILITY, IMAGE_GENERATE_CAPABILITY, IMAGE_INPAINT_CAPABILITY,
};
use crate::workflow::{WorkflowArtifact, WorkflowSpec};
use std::fs;
use std::path::{Path, PathBuf};

mod io_contract;
use io_contract::{
    flux_artifact, input_count, input_f32, input_i32, input_i64, input_string, output_paths,
    required_input_path,
};
use std::process::Command;

const FLUX_RUNNER_ENV: &str = "LIGHTFLOW_FLUX_RUNNER";
const FLUX_BACKEND_ENV: &str = "LIGHTFLOW_FLUX_BACKEND";

#[derive(Debug, Clone, PartialEq)]
pub(super) struct FluxExecution {
    pub(super) outputs: serde_json::Map<String, serde_json::Value>,
    pub(super) artifacts: Vec<WorkflowArtifact>,
}

#[derive(Debug, Clone)]
struct FluxModelHandles {
    diffusion_model: PathBuf,
    llm: PathBuf,
    vae: PathBuf,
}

struct FluxRunRequest<'a> {
    task: FluxTask,
    prompt: &'a str,
    negative: &'a str,
    width: i32,
    height: i32,
    seed: i64,
    steps: i32,
    guidance: f32,
    cfg_scale: f32,
    strength: f32,
    image_path: Option<&'a Path>,
    mask_path: Option<&'a Path>,
    output_path: &'a Path,
    models: &'a FluxModelHandles,
}

#[allow(dead_code)]
struct FluxBatchRunRequest<'a> {
    task: FluxTask,
    prompt: &'a str,
    negative: &'a str,
    width: i32,
    height: i32,
    seed: i64,
    steps: i32,
    guidance: f32,
    cfg_scale: f32,
    strength: f32,
    output_paths: &'a [PathBuf],
    models: &'a FluxModelHandles,
}

#[derive(Debug, Clone, Copy)]
enum FluxTask {
    TextToImage,
    ImageEdit,
    Inpaint,
}

#[derive(Debug, Clone, Copy)]
enum FluxBackend {
    Native,
    ExternalRunner,
}

impl FluxBackend {
    fn engine(self) -> &'static str {
        match self {
            Self::Native => "diffusion-rs.native.v1",
            Self::ExternalRunner => "flux2-klein.gguf.runner.v1",
        }
    }
}

impl FluxTask {
    fn as_str(self) -> &'static str {
        match self {
            Self::TextToImage => "text-to-image",
            Self::ImageEdit => "image-edit",
            Self::Inpaint => "inpaint",
        }
    }

    fn default_strength(self) -> f32 {
        match self {
            Self::TextToImage => 0.0,
            Self::ImageEdit => 0.75,
            Self::Inpaint => 0.85,
        }
    }

    fn capability(self) -> &'static str {
        match self {
            Self::TextToImage => IMAGE_GENERATE_CAPABILITY,
            Self::ImageEdit => IMAGE_EDIT_CAPABILITY,
            Self::Inpaint => IMAGE_INPAINT_CAPABILITY,
        }
    }
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

fn selected_flux_backend() -> ApiResult<FluxBackend> {
    match std::env::var(FLUX_BACKEND_ENV).as_deref() {
        Ok("external") | Ok("runner") => Ok(FluxBackend::ExternalRunner),
        Ok("native") => Ok(FluxBackend::Native),
        Ok(other) => Err(ApiError::InvalidRequest(format!(
            "{FLUX_BACKEND_ENV} must be native, external, or unset; got {other}"
        ))),
        Err(_) => {
            if native_flux_runtime_enabled() {
                Ok(FluxBackend::Native)
            } else {
                Ok(FluxBackend::ExternalRunner)
            }
        }
    }
}

fn run_selected_flux_backend(request: &FluxRunRequest<'_>, backend: FluxBackend) -> ApiResult<()> {
    match backend {
        FluxBackend::Native => run_flux_native(request),
        FluxBackend::ExternalRunner => run_flux_runner(request),
    }
}

fn native_flux_runtime_enabled() -> bool {
    cfg!(feature = "flux-native")
}

#[cfg(feature = "flux-native")]
fn run_flux_native(request: &FluxRunRequest<'_>) -> ApiResult<()> {
    let request = super::flux_native::NativeFluxRequest {
        task: request.task.as_str(),
        prompt: request.prompt,
        negative: request.negative,
        width: request.width,
        height: request.height,
        seed: request.seed,
        steps: request.steps,
        guidance: request.guidance,
        cfg_scale: request.cfg_scale,
        strength: request.strength,
        image_path: request.image_path,
        mask_path: request.mask_path,
        output_path: request.output_path,
        diffusion_model: &request.models.diffusion_model,
        llm_model: &request.models.llm,
        vae_model: &request.models.vae,
    };
    super::flux_native::generate(request)
}

#[cfg(feature = "flux-native")]
fn run_flux_native_batch(request: &FluxBatchRunRequest<'_>) -> ApiResult<()> {
    let request = super::flux_native::NativeFluxBatchRequest {
        task: request.task.as_str(),
        prompt: request.prompt,
        negative: request.negative,
        width: request.width,
        height: request.height,
        seed: request.seed,
        steps: request.steps,
        guidance: request.guidance,
        cfg_scale: request.cfg_scale,
        strength: request.strength,
        output_paths: request.output_paths,
        diffusion_model: &request.models.diffusion_model,
        llm_model: &request.models.llm,
        vae_model: &request.models.vae,
    };
    super::flux_native::generate_batch(request)
}

#[cfg(not(feature = "flux-native"))]
fn run_flux_native(_request: &FluxRunRequest<'_>) -> ApiResult<()> {
    Err(ApiError::InvalidRequest(
        "native FLUX backend requested, but this LightFlow binary was not built with --features flux-native".to_owned(),
    ))
}

#[cfg(not(feature = "flux-native"))]
fn run_flux_native_batch(_request: &FluxBatchRunRequest<'_>) -> ApiResult<()> {
    Err(ApiError::InvalidRequest(
        "native FLUX backend requested, but this LightFlow binary was not built with --features flux-native".to_owned(),
    ))
}

fn run_flux_runner(request: &FluxRunRequest<'_>) -> ApiResult<()> {
    let runner = std::env::var_os(FLUX_RUNNER_ENV).ok_or_else(|| {
        ApiError::InvalidRequest(format!(
            "workflow requires a FLUX runner; set {FLUX_RUNNER_ENV} to an executable that accepts LightFlow FLUX runner arguments"
        ))
    })?;
    let runner = PathBuf::from(runner);
    if !runner.is_file() {
        return Err(ApiError::InvalidRequest(format!(
            "{FLUX_RUNNER_ENV} does not point to a file: {}",
            runner.display()
        )));
    }

    let mut command = Command::new(&runner);
    command
        .arg("--task")
        .arg(request.task.as_str())
        .arg("--prompt")
        .arg(request.prompt)
        .arg("--negative")
        .arg(request.negative)
        .arg("--width")
        .arg(request.width.to_string())
        .arg("--height")
        .arg(request.height.to_string())
        .arg("--seed")
        .arg(request.seed.to_string())
        .arg("--steps")
        .arg(request.steps.to_string())
        .arg("--guidance")
        .arg(request.guidance.to_string())
        .arg("--cfg-scale")
        .arg(request.cfg_scale.to_string())
        .arg("--strength")
        .arg(request.strength.to_string())
        .arg("--output")
        .arg(request.output_path)
        .arg("--flux-model")
        .arg(&request.models.diffusion_model)
        .arg("--llm-model")
        .arg(&request.models.llm)
        .arg("--vae-model")
        .arg(&request.models.vae);
    if let Some(image_path) = request.image_path {
        command.arg("--image").arg(image_path);
    }
    if let Some(mask_path) = request.mask_path {
        command.arg("--mask").arg(mask_path);
    }
    let output = command.output().map_err(ApiError::from)?;
    if !output.status.success() {
        return Err(ApiError::InvalidRequest(format!(
            "FLUX runner {} failed with status {}\nstdout:\n{}\nstderr:\n{}",
            runner.display(),
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )));
    }

    let bytes = fs::read(request.output_path).map_err(ApiError::from)?;
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(ApiError::InvalidRequest(format!(
            "FLUX generation completed but did not write a PNG: {}",
            request.output_path.display()
        )));
    }

    Ok(())
}
