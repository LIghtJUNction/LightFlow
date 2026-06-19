use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.text_prompt")
        .version("0.1.0")
        .name("Text Prompt")
        .description("Build a text prompt from structured input.")
        .input("value", "json")
        .input_description("value", "Structured source value to convert into prompt text.")
        .input_required("value", true)
        .input_widget("value", "json")
        .output("prompt", "text")
        .output_description("prompt", "Prompt text generated from the source value.")
        .build()
}
