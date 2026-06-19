use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.text_to_image")
        .version("0.1.0")
        .name("Text To Image")
        .description("Generate an image artifact from a text prompt through a pluggable image runtime.")
        .input("prompt", "text")
        .input_description("prompt", "Positive text prompt used for image generation.")
        .input_required("prompt", true)
        .input_widget("prompt", "prompt")
        .input("negative", "text")
        .input_description("negative", "Optional negative prompt.")
        .input_required("negative", false)
        .input_default_json("negative", "\"\"")
        .input_widget("negative", "textarea")
        .input("width", "integer")
        .input_description("width", "Output image width in pixels.")
        .input_required("width", false)
        .input_default_json("width", "512")
        .input_range("width", 64.0, 2048.0, 8.0)
        .input_widget("width", "number")
        .input("height", "integer")
        .input_description("height", "Output image height in pixels.")
        .input_required("height", false)
        .input_default_json("height", "512")
        .input_range("height", 64.0, 2048.0, 8.0)
        .input_widget("height", "number")
        .input("seed", "integer")
        .input_description("seed", "Optional deterministic generation seed.")
        .input_required("seed", false)
        .input_widget("seed", "seed")
        .input("output_path", "path")
        .input_description("output_path", "Optional destination PNG path.")
        .input_required("output_path", false)
        .input_widget("output_path", "file_save")
        .input_artifact_kind("output_path", "image")
        .input("model", "text")
        .input_description("model", "Optional model variant id for the image_model requirement.")
        .input_required("model", false)
        .input_enum_json("model", "[\"sdxl-gguf-q4\",\"sdxl-safetensors\"]")
        .input_widget("model", "model_select")
        .input_model_requirement("model", "image_model")
        .output("image", "artifact")
        .output_description("image", "Generated image artifact metadata.")
        .output_artifact_kind("image", "image")
        .output_model_requirement("image", "image_model")
        .output("image_path", "path")
        .output_description("image_path", "Path to the generated PNG image.")
        .output_artifact_kind("image_path", "image")
        .output_model_requirement("image_path", "image_model")
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
