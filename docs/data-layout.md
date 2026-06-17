# Data Layout

LightFlow project files are ordinary source-controlled files under `lightflow/`.

```text
lightflow/
  workflows/
    <workflow_id>.rs
```

## Workflow Files

Each workflow file is Rust source code with embedded metadata and definition:

```rust
use lightflow::workflow::*;

pub fn define() -> WorkflowSpec {
    workflow("workflow.example")
        .version("0.1.0")
        .name("Example")
        .description("Reusable workflow definition.")
        .input("value", "json")
        .output("text", "text")
        .build()
}
```

Composite workflows nest other workflows with `.node()` and connect node ports
with `.edge()`:

```rust
use lightflow::workflow::*;

pub fn define() -> WorkflowSpec {
    workflow("workflow.parent")
        .version("0.1.0")
        .name("Parent")
        .depends_on("workflow.child", "0.1.0")
        .node("child", "workflow.child")
        .build()
}
```

The backend accepts `WorkflowSpec` JSON over HTTP/MCP/CLI for tool integration,
but the source-controlled project format is Rust.

## Not Stored Here

Do not commit runtime state, credentials, generated artifacts, caches, or model
weights under `lightflow/`.
