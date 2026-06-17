# LightFlow

LightFlow is a backend-first workflow system. The current backend deliberately
keeps the domain model small:

- Workflow: a reusable leaf unit or a directed graph that nests other workflows.

There is no built-in agent loop, no CortexFS runtime dependency, and no
visual-editor-owned workflow format. Workflows are Rust library crates in the
repository so normal coding tools, including Codex, can edit and review them.

## Current Scope

- Rust workflow crates under `lightflow/workflows/`
- workflow validation, including nested workflow references and DAG cycle checks
- recursive workflow dependency resolution
- lightweight workflow execution plans with temporary node toggles
- CLI, HTTP, and MCP surfaces over the same backend service

## Out Of Scope

- external execution substrates as runtime dependencies
- component/model/node/composition as separate top-level concepts
- built-in agent planning
- frontend implementation

## Layout

```text
src/
  api.rs         # framework-independent service
  workflow.rs    # workflow domain types and Rust DSL builder
  cli.rs         # command-line interface
  mcp.rs         # MCP JSON-RPC adapter
  server.rs      # HTTP adapter
lightflow/
  workflows/     # source-controlled Rust workflow crates
openapi/
  lightflow.yaml # API contract
```

## Rust Workflow Crates

Reusable workflows are library crates with `src/lib.rs` and no `src/main.rs`:

```rust
use lightflow::workflow::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.text_plan")
        .version("0.1.0")
        .name("Text Plan")
        .input("value", "json")
        .output("result", "text")
        .depends_on("lightflow.std", "0.1.0")
        .depends_on("lightflow.text_prompt", "0.1.0")
        .node("identity", "lightflow.std")
        .node("prompt", "lightflow.text_prompt")
        .edge("identity", "value", "prompt", "value")
        .build()
}
```

The backend parses this DSL statically from Rust ASTs; it does not execute or
compile workflow source files.

## CLI

```bash
cargo run --bin lfw -- init
cargo run --bin lfw -- add my_flow --name "My Flow"
cargo run --bin lfw -- list
cargo run --bin lfw -- ls --detail
cargo run -- workflows list
cargo run -- workflows get lightflow.text_plan
cargo run --bin lfw -- deps lightflow.text_plan
cargo run --bin lfw -- run lightflow.text_plan --input value='{"topic":"demo"}'
cargo run --bin lfwx -- lightflow.text_plan --input value='{"topic":"demo"}' --disable prompt
cargo run -- workflows validate '{"id":"lightflow.example","version":"0.1.0","name":"Example"}'
cargo run -- serve --port 5174
```

`lfwx` is the short executor. It accepts `--input <name=json-or-text>` and
temporary node toggles:

```bash
lfwx lightflow.text_plan --input value=hello
lfwx lightflow.text_plan --input value=hello --disable prompt
lfwx lightflow.text_plan --input value=hello --disable prompt --enable prompt
```

The current runner validates the workflow graph, executes nodes in topological
order, and uses passthrough semantics for leaf workflows. This gives the CLI,
HTTP, and MCP surfaces a stable execution contract before provider-specific
runtime adapters are added.

## Installing Workflows

Workflow dependencies are Cargo dependencies. A local standard workflow can be
installed with:

```toml
[workspace.dependencies]
lightflow-std = { path = "lightflow/workflows/lightflow.std" }
```

Any dependency crate that exposes `pub fn define() -> WorkflowSpec` in
`src/lib.rs` is discovered by the backend and can be referenced from
`.depends_on(...)` or `.node(...)`.

## Versioning

Workflow versions are SemVer strings. The current resolver supports exact
requirements and `*`:

```rust
workflow("lightflow.std")
    .version("0.1.0")
    .depends_on("lightflow.other", "0.1.0")
```

Range requirements such as `^0.1` and `>=0.1` are planned after the exact
version update path is stable.

## Sync

`lfw sync` prepares module and model dependencies. It always treats model
requirements as choices, not as mandatory downloads of every possible file.

```bash
lfw sync lightflow.image_prompt --dry-run
lfw sync lightflow.image_prompt --model image_model=flux2-gguf --apply
```

The module side uses Cargo:

```bash
cargo fetch
```

The model side uses Hugging Face's CLI and global cache:

```bash
hf download city96/FLUX.2-dev-gguf flux2-dev-q4.gguf
```

This lets workflows declare a model capability, such as text-to-image, and
offer safetensors / GGUF variants without forcing every user to sync the same
model artifact.

## HTTP

```bash
curl http://127.0.0.1:5174/workflows
curl http://127.0.0.1:5174/workflows/lightflow.text_plan
curl http://127.0.0.1:5174/workflows/lightflow.text_plan/dependencies
curl -X POST http://127.0.0.1:5174/workflows/lightflow.text_plan/run \
  -H 'content-type: application/json' \
  -d '{"inputs":{"value":"hello"},"disabled_nodes":["prompt"]}'
```
