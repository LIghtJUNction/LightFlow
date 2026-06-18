use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.text_prompt")
        .version("0.1.0")
        .name("Text Prompt")
        .description("Build a text prompt from structured input.")
        .input("value", "json")
        .output("prompt", "text")
        .build()
}
