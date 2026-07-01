use super::types::{FluxBackend, FluxBatchRunRequest, FluxRunRequest};
use crate::api::{ApiError, ApiResult};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

const FLUX_RUNNER_ENV: &str = "LIGHTFLOW_FLUX_RUNNER";
const FLUX_BACKEND_ENV: &str = "LIGHTFLOW_FLUX_BACKEND";

pub(super) fn selected_flux_backend() -> ApiResult<FluxBackend> {
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

pub(super) fn run_selected_flux_backend(
    request: &FluxRunRequest<'_>,
    backend: FluxBackend,
) -> ApiResult<()> {
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
    let request = super::super::flux_native::NativeFluxRequest {
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
    super::super::flux_native::generate(request)
}

#[cfg(feature = "flux-native")]
pub(super) fn run_flux_native_batch(request: &FluxBatchRunRequest<'_>) -> ApiResult<()> {
    let request = super::super::flux_native::NativeFluxBatchRequest {
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
    super::super::flux_native::generate_batch(request)
}

#[cfg(not(feature = "flux-native"))]
fn run_flux_native(_request: &FluxRunRequest<'_>) -> ApiResult<()> {
    Err(ApiError::InvalidRequest(
        "native FLUX backend requested, but this LightFlow binary was not built with --features flux-native".to_owned(),
    ))
}

#[cfg(not(feature = "flux-native"))]
pub(super) fn run_flux_native_batch(_request: &FluxBatchRunRequest<'_>) -> ApiResult<()> {
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
