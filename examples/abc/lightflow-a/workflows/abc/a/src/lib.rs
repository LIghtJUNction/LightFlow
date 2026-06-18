use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.a")
        .version("0.1.0")
        .name("A")
        .description("Chooses workflow B or C with an if node.")
        .input("use_b", "boolean")
        .input("value", "text")
        .output("value", "text")
        .depends_on_path(
            "lightflow.b",
            "0.1.0",
            "lightflow-b",
            "../lightflow-b/workflows/abc/b",
        )
        .depends_on_path(
            "lightflow.c",
            "0.1.0",
            "lightflow-c",
            "../lightflow-c/workflows/abc/c",
        )
        .if_node("choose", "use_b", true, "lightflow.b", "lightflow.c")
        .build()
}
