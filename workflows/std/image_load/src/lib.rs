use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.image.load")
        .version("0.1.0")
        .name("Image Load")
        .description("Load a PNG image path into an image artifact handle.")
        .input("image_path", "path")
        .input_description("image_path", "Source PNG image path.")
        .input_required("image_path", true)
        .input_widget("image_path", "image")
        .input_artifact_kind("image_path", "image")
        .output("image", "artifact")
        .output_description("image", "Loaded image artifact metadata.")
        .output_artifact_kind("image", "image")
        .output("image_path", "path")
        .output_description("image_path", "Source PNG image path.")
        .output_artifact_kind("image_path", "image")
        .runtime("image_load", "lightflow.image.load")
        .build()
}
