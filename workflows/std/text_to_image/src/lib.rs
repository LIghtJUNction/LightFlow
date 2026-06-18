use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.text_to_image")
        .version("0.1.0")
        .name("Text To Image")
        .description("Generate an image artifact from a text prompt through a pluggable image runtime.")
        .input("prompt", "text")
        .input("negative", "text")
        .input("width", "integer")
        .input("height", "integer")
        .input("seed", "integer")
        .input("output_path", "path")
        .input("model", "text")
        .output("image", "artifact")
        .output("image_path", "path")
        .builtin_runtime(
            "image_runtime",
            "lightflow.image.generate",
            "builtin.preview.v1"
        )
        .hf_model(
            "image_model",
            "sdxl-gguf-q4",
            "text-to-image",
            "gguf",
            "city96/stable-diffusion-xl-base-1.0-gguf",
            "sd_xl_base_1.0-q4_k_m.gguf"
        )
        .hf_model(
            "image_model",
            "sdxl-safetensors",
            "text-to-image",
            "safetensors",
            "stabilityai/stable-diffusion-xl-base-1.0",
            "sd_xl_base_1.0.safetensors"
        )
        .build()
}
