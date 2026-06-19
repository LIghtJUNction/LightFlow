use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.image.inpaint")
        .version("0.1.0")
        .name("Image Inpaint Preview")
        .description("Create a deterministic preview inpaint from a source PNG, mask, and prompt.")
        .input("image_path", "path")
        .input_description("image_path", "Source PNG image path.")
        .input_required("image_path", true)
        .input_widget("image_path", "image")
        .input_artifact_kind("image_path", "image")
        .input("mask_path", "path")
        .input_description("mask_path", "PNG mask path. White pixels receive the strongest preview edit.")
        .input_required("mask_path", true)
        .input_widget("mask_path", "image")
        .input_artifact_kind("mask_path", "mask")
        .input("prompt", "text")
        .input_description("prompt", "Inpaint prompt.")
        .input_required("prompt", true)
        .input_widget("prompt", "prompt")
        .input("negative", "text")
        .input_description("negative", "Optional negative prompt.")
        .input_required("negative", false)
        .input_widget("negative", "textarea")
        .input("seed", "integer")
        .input_description("seed", "Optional deterministic seed.")
        .input_required("seed", false)
        .input_widget("seed", "number")
        .input("output_path", "path")
        .input_description("output_path", "Optional destination PNG path.")
        .input_required("output_path", false)
        .input_widget("output_path", "file_save")
        .input_artifact_kind("output_path", "image")
        .output("image", "artifact")
        .output_description("image", "Inpainted preview image artifact metadata.")
        .output_artifact_kind("image", "image")
        .output("image_path", "path")
        .output_description("image_path", "Path to the inpainted preview PNG image.")
        .output_artifact_kind("image_path", "image")
        .output("prompt", "text")
        .output_description("prompt", "Prompt used for the preview inpaint.")
        .builtin_runtime(
            "image_inpaint_preview",
            "lightflow.image.inpaint",
            "builtin.preview.inpaint.v1",
        )
        .hf_model(
            "image_model",
            "flux-inpaint-preview",
            "text-to-image",
            "gguf",
            "lightflow/preview",
            "flux-inpaint-preview.gguf",
        )
        .build()
}
