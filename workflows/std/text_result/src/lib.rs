use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.text_result")
        .version("0.1.0")
        .name("Text Result")
        .description("Normalize generated text into a final result.")
        .input("text", "text")
        .input_description("text", "Generated text to expose through the final result port.")
        .input_required("text", true)
        .input_widget("text", "textarea")
        .output("result", "text")
        .output_description("result", "Final normalized text result.")
        .build()
}
