use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.image.invert")
        .version("0.1.0")
        .name("Image Invert")
        .description("Invert the colors of an input PNG image.")
        .input("image_path", "path")
        .input_description("image_path", "Source PNG image path.")
        .input_required("image_path", true)
        .input_widget("image_path", "image")
        .input_artifact_kind("image_path", "image")
        .input("output_path", "path")
        .input_description("output_path", "Optional destination PNG path.")
        .input_required("output_path", false)
        .input_widget("output_path", "file_save")
        .input_artifact_kind("output_path", "image")
        .output("image", "artifact")
        .output_description("image", "Image artifact metadata for the inverted PNG.")
        .output_artifact_kind("image", "image")
        .output("image_path", "path")
        .output_description("image_path", "Path to the inverted PNG.")
        .output_artifact_kind("image_path", "image")
        .builtin_runtime(
            "image_runtime",
            "lightflow.image.invert",
            "builtin.image.invert.v1",
        )
        .build()
}
