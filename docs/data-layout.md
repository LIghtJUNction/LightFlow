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

Nodes can be marked disabled in source with `.disabled_node(...)`. Execution
commands can temporarily override node state with `--disable <node>` and
`--enable <node>` without editing the workflow file.

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

`lfw add-dep` writes these dependencies into the workspace manifest:

```bash
lfw add-dep lightflow-std --path lightflow/workflows/lightflow.std
lfw add-dep lightflow-std --git https://github.com/lightjunction/LightFlow --package lightflow-std
```

`lfw sync` delegates Rust module fetching to Cargo.

## Publishing Workflow Crates

`lfw init` and `lfw add` generate workflow crates with versioned `lightflow`
dependencies and without `publish = false`, so they can become crates.io
packages once their metadata is ready.

```bash
lfw publish lightflow.example
lfw publish lightflow.example --apply
```

Repository-internal examples can still opt out with `publish = false`.
`lfw publish` reports those as non-publishable instead of trying to upload
them.

## Versioning

Workflow definitions use SemVer strings:

```rust
workflow("lightflow.std")
    .version("0.1.0")
```

Explicit workflow dependencies currently use exact SemVer requirements:

```rust
.depends_on("lightflow.std", "0.1.0")
```

The backend also accepts `*` for an unconstrained local dependency. Range
requirements such as `^0.1` and `>=0.1` are intentionally not supported yet;
they will be added after the exact-version update path is stable.

## Model Requirements

Model requirements are embedded in the workflow file. A workflow can declare an
abstract model capability and provide multiple Hugging Face variants:

```rust
workflow("lightflow.image_prompt")
    .version("0.1.0")
    .hf_model(
        "image_model",
        "flux2-safetensors",
        "text-to-image",
        "safetensors",
        "black-forest-labs/FLUX.2-dev",
        "flux2-dev.safetensors",
    )
    .hf_model(
        "image_model",
        "flux2-gguf",
        "text-to-image",
        "gguf",
        "city96/FLUX.2-dev-gguf",
        "flux2-dev-q4.gguf",
    )
```

`lfw sync` will not download every variant. Without a `--model
image_model=<variant>` selection it reports the unresolved model requirement.
With a selection it builds an `hf download ...` command and, when `--apply` is
used, executes it through the Hugging Face CLI so the artifact is managed by
the global HF cache.

## Execution Inputs

`lfwx` is the short workflow executor:

```bash
lfwx lightflow.image_prompt --input positive="a quiet lake" --input negative=blur
lfwx lightflow.image_prompt --input positive="a quiet lake" --disable render
```

Input values are parsed as JSON when possible and otherwise treated as strings.
The execution result records workflow inputs, workflow outputs, and per-node
status, inputs, and outputs.

## Not Stored Here

Do not commit runtime state, credentials, generated artifacts, caches, or model
weights under `lightflow/`.
