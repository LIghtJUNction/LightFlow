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
    if request.task == "text-to-image"
        && request.image_path.is_none()
        && request.mask_path.is_none()
    {
        return generate_text_to_image_with_cached_session(request);
    }

    generate_with_diffusion_rs(request)
}

fn generate_with_diffusion_rs(request: NativeFluxRequest<'_>) -> ApiResult<()> {
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

fn generate_text_to_image_with_cached_session(request: NativeFluxRequest<'_>) -> ApiResult<()> {
    use diffusion_rs_sys::{
        generate_image, lora_apply_mode_t, sd_cache_params_init, sd_ctx_params_init,
        sd_get_default_sample_method, sd_get_default_scheduler, sd_guidance_params_t,
        sd_hires_params_init, sd_image_t, sd_img_gen_params_init, sd_pm_params_t,
        sd_sample_params_init, sd_set_progress_callback, sd_slg_params_t, sd_tiling_params_t,
        sd_vae_format_t,
    };
    use libc::free;
    use std::ffi::{CString, c_void};
    use std::fs::File;
    use std::io::BufWriter;
    use std::ptr::null_mut;
    use std::slice;
    use std::sync::{Mutex, OnceLock};

    static SESSION: OnceLock<Mutex<Option<NativeFluxSession>>> = OnceLock::new();

    let mut session = SESSION
        .get_or_init(|| Mutex::new(None))
        .lock()
        .map_err(|_| {
            ApiError::InvalidRequest("native FLUX session lock was poisoned".to_owned())
        })?;
    let key = NativeFluxSessionKey::from_request(&request);
    let reload = session
        .as_ref()
        .map(|session| session.key != key)
        .unwrap_or(true);
    if reload {
        *session = Some(NativeFluxSession::load(key.clone())?);
    }
    let session = session
        .as_mut()
        .ok_or_else(|| ApiError::InvalidRequest("native FLUX session was not loaded".to_owned()))?;

    let prompt = cstring("prompt", request.prompt)?;
    let negative = cstring("negative prompt", request.negative)?;
    let mut layers: Vec<i32> = Vec::new();

    unsafe {
        sd_set_progress_callback(None, null_mut());

        let sample_method = sd_get_default_sample_method(session.ctx);
        let scheduler = sd_get_default_scheduler(session.ctx, sample_method);
        let mut sample_params = std::mem::zeroed();
        sd_sample_params_init(&mut sample_params);
        sample_params.guidance = sd_guidance_params_t {
            txt_cfg: request.cfg_scale,
            img_cfg: request.cfg_scale,
            distilled_guidance: request.guidance,
            slg: sd_slg_params_t {
                layers: layers.as_mut_ptr(),
                layer_count: layers.len(),
                layer_start: 0.01,
                layer_end: 0.2,
                scale: 0.0,
            },
        };
        sample_params.sample_method = sample_method;
        sample_params.scheduler = scheduler;
        sample_params.sample_steps = request.steps;

        let mut cache = std::mem::zeroed();
        sd_cache_params_init(&mut cache);

        let mut hires = std::mem::zeroed();
        sd_hires_params_init(&mut hires);

        let mut params = std::mem::zeroed();
        sd_img_gen_params_init(&mut params);
        params.prompt = prompt.as_ptr();
        params.negative_prompt = negative.as_ptr();
        params.width = request.width;
        params.height = request.height;
        params.seed = request.seed;
        params.batch_count = 1;
        params.sample_params = sample_params;
        params.strength = request.strength;
        params.init_image = sd_image_t {
            width: 0,
            height: 0,
            channel: 3,
            data: null_mut(),
        };
        params.mask_image = sd_image_t {
            width: request.width as u32,
            height: request.height as u32,
            channel: 1,
            data: null_mut(),
        };
        params.control_image = sd_image_t {
            width: 0,
            height: 0,
            channel: 3,
            data: null_mut(),
        };
        params.vae_tiling_params = sd_tiling_params_t {
            enabled: true,
            temporal_tiling: false,
            tile_size_x: 32,
            tile_size_y: 32,
            target_overlap: 0.5,
            rel_size_x: 0.0,
            rel_size_y: 0.0,
            extra_tiling_args: null_mut(),
        };
        params.cache = cache;
        params.hires = hires;
        params.pm_params = sd_pm_params_t {
            id_images: null_mut(),
            id_images_count: 0,
            id_embed_path: null_mut(),
            style_strength: 20.0,
        };

        let images = generate_image(session.ctx, &params);
        if images.is_null() {
            return Err(ApiError::InvalidRequest(
                "native FLUX text-to-image generation returned no image".to_owned(),
            ));
        }

        let result = slice::from_raw_parts(images, 1)
            .first()
            .ok_or_else(|| {
                ApiError::InvalidRequest(
                    "native FLUX text-to-image generation returned an empty image list".to_owned(),
                )
            })
            .and_then(|image| write_native_png(*image, request.output_path));
        free(images.cast::<c_void>());
        result?;
    }

    let bytes = std::fs::read(request.output_path).map_err(ApiError::from)?;
    if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        return Err(ApiError::InvalidRequest(format!(
            "native FLUX generation completed but did not write a PNG: {}",
            request.output_path.display()
        )));
    }

    #[derive(Debug, Clone, PartialEq, Eq)]
    struct NativeFluxSessionKey {
        diffusion_model: std::path::PathBuf,
        llm_model: std::path::PathBuf,
        vae_model: std::path::PathBuf,
    }

    impl NativeFluxSessionKey {
        fn from_request(request: &NativeFluxRequest<'_>) -> Self {
            Self {
                diffusion_model: request.diffusion_model.to_path_buf(),
                llm_model: request.llm_model.to_path_buf(),
                vae_model: request.vae_model.to_path_buf(),
            }
        }
    }

    struct NativeFluxSession {
        key: NativeFluxSessionKey,
        ctx: *mut diffusion_rs_sys::sd_ctx_t,
        _diffusion_model: CString,
        _llm_model: CString,
        _vae_model: CString,
    }

    unsafe impl Send for NativeFluxSession {}

    impl NativeFluxSession {
        fn load(key: NativeFluxSessionKey) -> ApiResult<Self> {
            let diffusion_model = cstring_path("FLUX model", &key.diffusion_model)?;
            let llm_model = cstring_path("LLM model", &key.llm_model)?;
            let vae_model = cstring_path("VAE model", &key.vae_model)?;

            unsafe {
                let mut params = std::mem::zeroed();
                sd_ctx_params_init(&mut params);
                params.diffusion_model_path = diffusion_model.as_ptr();
                params.llm_path = llm_model.as_ptr();
                params.vae_path = vae_model.as_ptr();
                params.vae_format = sd_vae_format_t::SD_VAE_FORMAT_FLUX2;
                params.enable_mmap = true;
                params.flash_attn = true;
                params.lora_apply_mode = lora_apply_mode_t::LORA_APPLY_AUTO;

                let ctx = diffusion_rs_sys::new_sd_ctx(&params);
                if ctx.is_null() {
                    return Err(ApiError::InvalidRequest(format!(
                        "failed to load native FLUX session for {}",
                        key.diffusion_model.display()
                    )));
                }

                if !diffusion_rs_sys::sd_ctx_supports_image_generation(ctx) {
                    diffusion_rs_sys::free_sd_ctx(ctx);
                    return Err(ApiError::InvalidRequest(format!(
                        "native FLUX session does not support image generation: {}",
                        key.diffusion_model.display()
                    )));
                }

                Ok(Self {
                    key,
                    ctx,
                    _diffusion_model: diffusion_model,
                    _llm_model: llm_model,
                    _vae_model: vae_model,
                })
            }
        }
    }

    impl Drop for NativeFluxSession {
        fn drop(&mut self) {
            unsafe {
                diffusion_rs_sys::free_sd_ctx(self.ctx);
            }
        }
    }

    fn cstring(label: &str, value: &str) -> ApiResult<CString> {
        CString::new(value)
            .map_err(|_| ApiError::InvalidRequest(format!("{label} contains an interior NUL byte")))
    }

    fn cstring_path(label: &str, path: &Path) -> ApiResult<CString> {
        cstring(label, &path.display().to_string())
    }

    fn write_native_png(image: diffusion_rs_sys::sd_image_t, path: &Path) -> ApiResult<()> {
        if image.data.is_null() {
            return Err(ApiError::InvalidRequest(
                "native FLUX text-to-image generation returned null image data".to_owned(),
            ));
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(ApiError::from)?;
        }

        let len = image
            .width
            .checked_mul(image.height)
            .and_then(|pixels| pixels.checked_mul(image.channel))
            .ok_or_else(|| {
                ApiError::InvalidRequest("native FLUX image dimensions overflowed".to_owned())
            })? as usize;
        let data = unsafe { slice::from_raw_parts(image.data, len) };
        let file = File::create(path).map_err(ApiError::from)?;
        let writer = BufWriter::new(file);
        let mut encoder = png::Encoder::new(writer, image.width, image.height);
        encoder.set_depth(png::BitDepth::Eight);
        encoder.set_color(match image.channel {
            1 => png::ColorType::Grayscale,
            3 => png::ColorType::Rgb,
            4 => png::ColorType::Rgba,
            channel => {
                return Err(ApiError::InvalidRequest(format!(
                    "native FLUX returned unsupported PNG channel count: {channel}"
                )));
            }
        });
        let mut png = encoder.write_header().map_err(|error| {
            ApiError::InvalidRequest(format!(
                "failed to write native FLUX PNG header for {}: {error}",
                path.display()
            ))
        })?;
        png.write_image_data(data).map_err(|error| {
            ApiError::InvalidRequest(format!(
                "failed to write native FLUX PNG data for {}: {error}",
                path.display()
            ))
        })
    }

    Ok(())
}
