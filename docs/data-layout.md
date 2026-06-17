# Data Layout

LightFlow project files are ordinary source-controlled files under `lightflow/`.

```text
lightflow/
  workflows/
    <workflow_id>/
      Cargo.toml
      src/
        lib.rs
```

## Workflow Crates

Each workflow is a Rust library crate with embedded metadata and definition in
`src/lib.rs`:

```rust
use lightflow::workflow::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.example")
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
    workflow("lightflow.parent")
        .version("0.1.0")
        .name("Parent")
        .depends_on("lightflow.child", "0.1.0")
        .node("child", "lightflow.child")
        .build()
}
```

Reusable workflows do not include `src/main.rs`. If a workflow crate has no
`main.rs`, it is imported or nested by other workflows instead of used as an
executable entrypoint.

The backend accepts `WorkflowSpec` JSON over HTTP/MCP/CLI for tool integration,
but the source-controlled project format is Rust.

## Installed Workflow Dependencies

A workflow can be installed as a Cargo dependency. The backend scans local
workflow crates under `lightflow/workflows/` and also scans `path`
dependencies declared in the project `Cargo.toml`:

```toml
[workspace.dependencies]
lightflow-std = { path = "lightflow/workflows/lightflow.std" }
```

If the dependency target contains `src/lib.rs` with `pub fn define() ->
WorkflowSpec`, it is added to the workflow registry and can satisfy
`.depends_on(...)` and `.node(...)` references.

Git dependencies use the same manifest shape:

```toml
[dependencies]
lightflow-std = { git = "https://github.com/lightjunction/LightFlow", package = "lightflow-std" }
```

The first implemented discovery path is local `path` dependencies. Remote git
dependencies will be made local by `lfw sync` in the next installation pass.

## Not Stored Here

Do not commit runtime state, credentials, generated artifacts, caches, or model
weights under `lightflow/`.
