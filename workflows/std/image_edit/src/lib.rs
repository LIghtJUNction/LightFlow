use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.image.edit")
        .version("0.1.0")
        .name("Image Edit Preview")
        .description("Create a deterministic preview image edit from a source PNG and prompt.")
        .input("image_path", "path")
        .input_description("image_path", "Source PNG image path.")
        .input_required("image_path", true)
        .input_widget("image_path", "image")
        .input_artifact_kind("image_path", "image")
        .input("prompt", "text")
        .input_description("prompt", "Edit prompt.")
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
        .output_description("image", "Edited preview image artifact metadata.")
        .output_artifact_kind("image", "image")
        .output("image_path", "path")
        .output_description("image_path", "Path to the edited preview PNG image.")
        .output_artifact_kind("image_path", "image")
        .output("prompt", "text")
        .output_description("prompt", "Prompt used for the preview edit.")
        .builtin_runtime("image_edit_preview", "lightflow.image.edit", "builtin.preview.edit.v1")
        .hf_model(
            "image_model",
            "flux-edit-preview",
            "text-to-image",
            "gguf",
            "lightflow/preview",
            "flux-edit-preview.gguf",
        )
        .build()
}
