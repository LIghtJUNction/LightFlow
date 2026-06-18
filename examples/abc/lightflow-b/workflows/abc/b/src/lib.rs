use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.b")
        .version("0.1.0")
        .name("B")
        .description("ABC example leaf workflow B.")
        .input("value", "text")
        .output("value", "text")
        .build()
}
