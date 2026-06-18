use super::{ApiError, ApiResult};
use std::path::Path;

pub(super) struct NativeFluxRequest<'a> {
    pub(super) task: &'a str,
    pub(super) prompt: &'a str,
    pub(super) negative: &'a str,
    pub(super) width: i32,
    pub(super) height: i32,
    pub(super) seed: i64,
    pub(super) steps: i32,
    pub(super) guidance: f32,
    pub(super) cfg_scale: f32,
    pub(super) strength: f32,
    pub(super) image_path: Option<&'a Path>,
    pub(super) mask_path: Option<&'a Path>,
    pub(super) output_path: &'a Path,
    pub(super) diffusion_model: &'a Path,
    pub(super) llm_model: &'a Path,
    pub(super) vae_model: &'a Path,
}

pub(super) fn generate(request: NativeFluxRequest<'_>) -> ApiResult<()> {
    use diffusion_rs::api::{ConfigBuilder, ModelConfigBuilder, VaeFormat, gen_img};

    let mut model = ModelConfigBuilder::default();
    model
        .diffusion_model(request.diffusion_model.to_path_buf())
        .llm(request.llm_model.to_path_buf())
        .vae(request.vae_model.to_path_buf())
        .vae_format(VaeFormat::SD_VAE_FORMAT_FLUX2)
        .flash_attention(true)
        .vae_tiling(true)
        .enable_mmap(true);

    let mut config = ConfigBuilder::default();
    config
        .prompt(request.prompt.to_owned())
        .negative_prompt(request.negative.to_owned())
        .width(request.width)
        .height(request.height)
        .seed(request.seed)
        .steps(request.steps)
        .guidance(request.guidance)
        .cfg_scale(request.cfg_scale)
        .strength(request.strength)
        .output(request.output_path.to_path_buf());

    if let Some(image_path) = request.image_path {
        config.init_img(image_path.to_path_buf());
    }
    if let Some(mask_path) = request.mask_path {
        config.mask_img(mask_path.to_path_buf());
    }

    let mut model = model.build().map_err(|error| {
        ApiError::InvalidRequest(format!(
            "failed to build native FLUX model config for {}: {error}",
            request.task
        ))
    })?;
    let config = config.build().map_err(|error| {
        ApiError::InvalidRequest(format!(
            "failed to build native FLUX generation config for {}: {error}",
            request.task
        ))
    })?;

    gen_img(&config, &mut model).map_err(|error| {
        ApiError::InvalidRequest(format!(
            "native FLUX generation failed for {}: {error}",
            request.task
        ))
    })
}
