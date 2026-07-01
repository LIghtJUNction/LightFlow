use crate::api::plan::{
    IMAGE_EDIT_CAPABILITY, IMAGE_GENERATE_CAPABILITY, IMAGE_INPAINT_CAPABILITY,
};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub(super) struct FluxModelHandles {
    pub(super) diffusion_model: PathBuf,
    pub(super) llm: PathBuf,
    pub(super) vae: PathBuf,
}

pub(super) struct FluxRunRequest<'a> {
    pub(super) task: FluxTask,
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
    pub(super) models: &'a FluxModelHandles,
}

#[allow(dead_code)]
pub(super) struct FluxBatchRunRequest<'a> {
    pub(super) task: FluxTask,
    pub(super) prompt: &'a str,
    pub(super) negative: &'a str,
    pub(super) width: i32,
    pub(super) height: i32,
    pub(super) seed: i64,
    pub(super) steps: i32,
    pub(super) guidance: f32,
    pub(super) cfg_scale: f32,
    pub(super) strength: f32,
    pub(super) output_paths: &'a [PathBuf],
    pub(super) models: &'a FluxModelHandles,
}

#[derive(Debug, Clone, Copy)]
pub(super) enum FluxTask {
    TextToImage,
    ImageEdit,
    Inpaint,
}

impl FluxTask {
    pub(super) fn as_str(self) -> &'static str {
        match self {
            Self::TextToImage => "text-to-image",
            Self::ImageEdit => "image-edit",
            Self::Inpaint => "inpaint",
        }
    }

    pub(super) fn default_strength(self) -> f32 {
        match self {
            Self::TextToImage => 0.0,
            Self::ImageEdit => 0.75,
            Self::Inpaint => 0.85,
        }
    }

    pub(super) fn capability(self) -> &'static str {
        match self {
            Self::TextToImage => IMAGE_GENERATE_CAPABILITY,
            Self::ImageEdit => IMAGE_EDIT_CAPABILITY,
            Self::Inpaint => IMAGE_INPAINT_CAPABILITY,
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub(super) enum FluxBackend {
    Native,
    ExternalRunner,
}

impl FluxBackend {
    pub(super) fn engine(self) -> &'static str {
        match self {
            Self::Native => "diffusion-rs.native.v1",
            Self::ExternalRunner => "flux2-klein.gguf.runner.v1",
        }
    }
}
