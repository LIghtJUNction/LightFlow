use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.c")
        .version("0.1.0")
        .name("C")
        .description("ABC example leaf workflow C.")
        .input("value", "text")
        .output("value", "text")
        .build()
}
