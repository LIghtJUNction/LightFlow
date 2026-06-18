use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.std")
        .version("0.1.0")
        .name("LightFlow Std Identity")
        .description("Domain-neutral identity workflow for passing one JSON value through unchanged.")
        .input("value", "json")
        .output("value", "json")
        .build()
}
