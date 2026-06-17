use lightflow::workflow::*;

pub fn define() -> WorkflowSpec {
    workflow("workflow.text_result")
        .version("0.1.0")
        .name("Text Result")
        .description("Normalize generated text into a final result.")
        .input("text", "text")
        .output("result", "text")
        .build()
}
