use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.std")
        .version("0.1.0")
        .name("LightFlow Std Identity")
        .description("Domain-neutral identity workflow for passing one JSON value through unchanged.")
        .input("value", "json")
        .input_description("value", "JSON value to pass through unchanged.")
        .input_required("value", true)
        .input_widget("value", "json")
        .output("value", "json")
        .output_description("value", "The same JSON value.")
        .build()
}
