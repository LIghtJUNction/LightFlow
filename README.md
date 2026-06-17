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
- CLI, HTTP, and MCP surfaces over the same backend service

## Out Of Scope

- external execution substrates as runtime dependencies
- component/model/node/composition as separate top-level concepts
- built-in agent planning
- frontend implementation
- workflow execution engine

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
        .depends_on("lightflow.text_prompt", "0.1.0")
        .node("prompt", "lightflow.text_prompt")
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
cargo run -- workflows validate '{"id":"lightflow.example","version":"0.1.0","name":"Example"}'
cargo run -- serve --port 5174
```

## HTTP

```bash
curl http://127.0.0.1:5174/workflows
curl http://127.0.0.1:5174/workflows/lightflow.text_plan
curl http://127.0.0.1:5174/workflows/lightflow.text_plan/dependencies
```
