use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.text_plan")
        .version("0.1.0")
        .name("Text Plan")
        .description("Example composite workflow built from workflow nodes.")
        .input("value", "json")
        .input_description("value", "JSON value passed into the example plan.")
        .input_required("value", true)
        .input_widget("value", "json")
        .output("result", "text")
        .output_description("result", "Final text result from the example plan.")
        .depends_on("lightflow.std", "0.1.0")
        .depends_on("lightflow.text_prompt", "0.1.0")
        .depends_on("lightflow.text_result", "0.1.0")
        .node("identity", "lightflow.std")
        .node("prompt", "lightflow.text_prompt")
        .node("result", "lightflow.text_result")
        .edge("identity", "value", "prompt", "value")
        .edge("prompt", "prompt", "result", "text")
        .build()
}
