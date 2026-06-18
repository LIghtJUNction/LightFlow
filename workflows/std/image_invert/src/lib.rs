use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.image.invert")
        .version("0.1.0")
        .name("Image Invert")
        .description("Invert the colors of an input PNG image.")
        .input("image_path", "path")
        .input("output_path", "path")
        .output("image", "artifact")
        .output("image_path", "path")
        .builtin_runtime(
            "image_runtime",
            "lightflow.image.invert",
            "builtin.image.invert.v1",
        )
        .build()
}
