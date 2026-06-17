use lightflow::workflow::*;

pub fn define() -> WorkflowSpec {
    workflow("workflow.text_plan")
        .version("0.1.0")
        .name("Text Plan")
        .description("Example composite workflow built from workflow nodes.")
        .input("value", "json")
        .output("result", "text")
        .depends_on("workflow.text_prompt", "0.1.0")
        .depends_on("workflow.text_result", "0.1.0")
        .node("prompt", "workflow.text_prompt")
        .node("result", "workflow.text_result")
        .edge("prompt", "prompt", "result", "text")
        .build()
}
