use lightflow::workflow::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.std")
        .version("0.1.0")
        .name("LightFlow Standard Library")
        .description("Minimal standard workflow crate for abstract reusable building blocks.")
        .input("value", "json")
        .output("value", "json")
        .build()
}
