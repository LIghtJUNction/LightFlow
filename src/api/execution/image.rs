use super::artifacts::{PreviewTransformArtifact, build_preview_transform_artifact};
use super::media::{
    image_transform_output_path, input_image_path, input_mask_path, input_string, input_u64,
    preview_image_outputs,
};
use super::png::{preview_edit_image, read_png_image, resize_png_image, write_png_image};
use super::types::LeafExecution;
use crate::api::ApiResult;
use crate::api::plan::{
    IMAGE_EDIT_CAPABILITY, IMAGE_INPAINT_CAPABILITY, PREVIEW_EDIT_ENGINE, PREVIEW_INPAINT_ENGINE,
};
use crate::workflow::WorkflowSpec;
use std::path::Path;

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
