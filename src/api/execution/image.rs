use super::artifacts::{
    PreviewTransformArtifact, build_preview_transform_artifact, image_file_artifact,
    image_transform_artifact, mask_artifact,
};
use super::media::{
    image_path_outputs, image_transform_output_path, input_image_path, input_mask_path,
    input_string, input_u32, input_u64, mask_output_path, preview_image_outputs,
};
use super::png::{
    compose_masks, crop_png_image, preview_edit_image, read_png_image, resize_png_image,
    write_inverted_png, write_png_image,
};
use super::types::LeafExecution;
use crate::api::plan::{
    IMAGE_CROP_CAPABILITY, IMAGE_EDIT_CAPABILITY, IMAGE_INPAINT_CAPABILITY,
    IMAGE_INVERT_CAPABILITY, IMAGE_LOAD_CAPABILITY, IMAGE_RESIZE_CAPABILITY, IMAGE_SAVE_CAPABILITY,
    IMAGE_UPSCALE_CAPABILITY, PREVIEW_EDIT_ENGINE, PREVIEW_INPAINT_ENGINE,
};
use crate::api::{ApiError, ApiResult};
use crate::workflow::WorkflowSpec;
use std::path::{Path, PathBuf};

pub(super) fn execute_image_invert(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let input_path = input_image_path(inputs)?;
    let output_path = image_transform_output_path(root, workflow, inputs, &input_path, "invert");
    write_inverted_png(&input_path, &output_path)?;

    let artifact = image_transform_artifact(
        &input_path,
        &output_path,
        crate::api::plan::INVERT_ENGINE,
        IMAGE_INVERT_CAPABILITY,
        Some((0, 0)),
    );
    image_path_outputs(workflow, inputs, &output_path, artifact)
}

pub(super) fn execute_image_load(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let image_path = input_image_path(inputs)?;
    let image = read_png_image(&image_path)?;

    let artifact = image_file_artifact(
        &image_path,
        &image_path,
        "builtin.image.load.v1",
        IMAGE_LOAD_CAPABILITY,
        Some((image.width, image.height)),
    );

    image_path_outputs(workflow, inputs, &image_path, artifact)
}

pub(super) fn execute_image_save(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let image_path = input_image_path(inputs)?;
    let image = read_png_image(&image_path)?;
    let output_path = image_transform_output_path(root, workflow, inputs, &image_path, "saved");

    if let Some(parent) = output_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::copy(&image_path, &output_path)?;

    let artifact = image_file_artifact(
        &image_path,
        &output_path,
        "builtin.image.save.v1",
        IMAGE_SAVE_CAPABILITY,
        Some((image.width, image.height)),
    );

    image_path_outputs(workflow, inputs, &output_path, artifact)
}

pub(super) fn execute_image_resize(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let image_path = input_image_path(inputs)?;
    let image = read_png_image(&image_path)?;
    let width = input_u32(inputs, "width").unwrap_or(image.width).max(1);
    let height = input_u32(inputs, "height").unwrap_or(image.height).max(1);
    let resized = resize_png_image(&image, width, height);
    let output_path = image_transform_output_path(root, workflow, inputs, &image_path, "resized");
    write_png_image(&output_path, &resized)?;

    let artifact = image_file_artifact(
        &image_path,
        &output_path,
        "builtin.image.resize.v1",
        IMAGE_RESIZE_CAPABILITY,
        Some((width, height)),
    );

    image_path_outputs(workflow, inputs, &output_path, artifact)
}

pub(super) fn execute_image_crop(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let image_path = input_image_path(inputs)?;
    let image = read_png_image(&image_path)?;
    let x = input_u32(inputs, "x").unwrap_or(0);
    let y = input_u32(inputs, "y").unwrap_or(0);
    let width = input_u32(inputs, "width").unwrap_or(image.width.saturating_sub(x));
    let height = input_u32(inputs, "height").unwrap_or(image.height.saturating_sub(y));
    let cropped = crop_png_image(&image, x, y, width, height)?;
    let output_path = image_transform_output_path(root, workflow, inputs, &image_path, "cropped");
    write_png_image(&output_path, &cropped)?;

    let artifact = image_file_artifact(
        &image_path,
        &output_path,
        "builtin.image.crop.v1",
        IMAGE_CROP_CAPABILITY,
        Some((cropped.width, cropped.height)),
    );

    image_path_outputs(workflow, inputs, &output_path, artifact)
}

pub(super) fn execute_preview_image_edit(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let image_path = input_image_path(inputs)?;
    let prompt = input_string(inputs, "prompt")
        .or_else(|| input_string(inputs, "text"))
        .unwrap_or_default();
    let seed = input_u64(inputs, "seed").unwrap_or_else(|| super::media::stable_seed(&prompt));
    let image = read_png_image(&image_path)?;
    let edited = preview_edit_image(&image, seed, &prompt, None);
    let output_path = image_transform_output_path(root, workflow, inputs, &image_path, "edited");
    write_png_image(&output_path, &edited)?;

    let artifact = build_preview_transform_artifact(&PreviewTransformArtifact {
        workflow,
        input_path: &image_path,
        mask_path: None,
        output_path: &output_path,
        prompt: &prompt,
        seed,
        engine: PREVIEW_EDIT_ENGINE,
        capability: IMAGE_EDIT_CAPABILITY,
        dimensions: Some((edited.width, edited.height)),
        inputs,
    });

    preview_image_outputs(workflow, inputs, &output_path, artifact, &prompt, seed)
}

pub(super) fn execute_preview_inpaint(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let image_path = input_image_path(inputs)?;
    let mask_path = input_mask_path(inputs)?;
    let prompt = input_string(inputs, "prompt")
        .or_else(|| input_string(inputs, "text"))
        .unwrap_or_default();
    let seed = input_u64(inputs, "seed").unwrap_or_else(|| super::media::stable_seed(&prompt));

    let image = read_png_image(&image_path)?;
    let mask = read_png_image(&mask_path)?;
    let mask = if mask.width == image.width && mask.height == image.height {
        mask
    } else {
        resize_png_image(&mask, image.width, image.height)
    };

    let inpainted = preview_edit_image(&image, seed, &prompt, Some(&mask));
    let output_path = image_transform_output_path(root, workflow, inputs, &image_path, "inpainted");
    write_png_image(&output_path, &inpainted)?;

    let artifact = build_preview_transform_artifact(&PreviewTransformArtifact {
        workflow,
        input_path: &image_path,
        mask_path: Some(&mask_path),
        output_path: &output_path,
        prompt: &prompt,
        seed,
        engine: PREVIEW_INPAINT_ENGINE,
        capability: IMAGE_INPAINT_CAPABILITY,
        dimensions: Some((inpainted.width, inpainted.height)),
        inputs,
    });

    preview_image_outputs(workflow, inputs, &output_path, artifact, &prompt, seed)
}

pub(super) fn execute_image_upscale(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let image_path = input_image_path(inputs)?;
    let image = read_png_image(&image_path)?;
    let scale = input_u32(inputs, "scale").unwrap_or(2).clamp(1, 16);
    let width = image.width.saturating_mul(scale).max(1);
    let height = image.height.saturating_mul(scale).max(1);
    let upscaled = resize_png_image(&image, width, height);
    let output_path = image_transform_output_path(root, workflow, inputs, &image_path, "upscaled");
    write_png_image(&output_path, &upscaled)?;

    let artifact = image_file_artifact(
        &image_path,
        &output_path,
        "builtin.image.upscale.v1",
        IMAGE_UPSCALE_CAPABILITY,
        Some((width, height)),
    );

    image_path_outputs(workflow, inputs, &output_path, artifact)
}

pub(super) fn execute_mask_compose(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let mask_a_path = input_string(inputs, "mask_a_path")
        .or_else(|| input_string(inputs, "a_path"))
        .or_else(|| input_string(inputs, "mask_path"))
        .map(PathBuf::from)
        .ok_or_else(|| ApiError::InvalidRequest("mask_a_path is required".to_owned()))?;

    let mask_b_path = input_string(inputs, "mask_b_path")
        .or_else(|| input_string(inputs, "b_path"))
        .map(PathBuf::from)
        .ok_or_else(|| ApiError::InvalidRequest("mask_b_path is required".to_owned()))?;

    let mode = input_string(inputs, "mode").unwrap_or_else(|| "max".to_owned());
    let mask_a = read_png_image(&mask_a_path)?;
    let mask_b = read_png_image(&mask_b_path)?;
    let mask_b = if mask_b.width == mask_a.width && mask_b.height == mask_a.height {
        mask_b
    } else {
        resize_png_image(&mask_b, mask_a.width, mask_a.height)
    };

    let composed = compose_masks(&mask_a, &mask_b, &mode)?;
    let output_path = mask_output_path(root, workflow, inputs, &mask_a_path, "composed");
    write_png_image(&output_path, &composed)?;

    let artifact = mask_artifact(
        &mask_a_path,
        &mask_b_path,
        &output_path,
        &mode,
        Some((composed.width, composed.height)),
    );

    super::media::mask_path_outputs(workflow, inputs, &output_path, artifact, &mode)
}
