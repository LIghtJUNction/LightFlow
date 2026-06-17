use lightflow::workflow::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.example")
        .version("0.1.0")
        .name("Example Workflow")
        .description("TODO: describe this workflow.")
        .input("value", "json")
        .output("value", "json")
        .build()
}
