# LightFlow

LightFlow is a backend-first workflow system. The current backend deliberately
keeps the domain model small:

- Workflow: a reusable leaf unit or a directed graph that nests other workflows.

There is no built-in agent loop, no CortexFS runtime dependency, and no
visual-editor-owned workflow format. Workflow files are Rust source files in
the repository so normal coding tools, including Codex, can edit and review
them.

## Current Scope

- Rust workflow files under `lightflow/workflows/`
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
  workflows/     # source-controlled Rust workflow files
openapi/
  lightflow.yaml # API contract
```

## Rust Workflow Files

```rust
use lightflow::workflow::*;

pub fn define() -> WorkflowSpec {
    workflow("workflow.text_plan")
        .version("0.1.0")
        .name("Text Plan")
        .input("value", "json")
        .output("result", "text")
        .depends_on("workflow.text_prompt", "0.1.0")
        .node("prompt", "workflow.text_prompt")
        .build()
}
```

The backend parses this DSL statically from Rust ASTs; it does not execute
workflow source files.

## CLI

```bash
cargo run --bin lfw -- init
cargo run --bin lfw -- add workflow.my_flow --name "My Flow"
cargo run -- workflows list
cargo run -- workflows get workflow.text_plan
cargo run --bin lfw -- deps workflow.text_plan
cargo run -- workflows validate '{"id":"workflow.example","version":"0.1.0","name":"Example"}'
cargo run -- serve --port 5174
```

## HTTP

```bash
curl http://127.0.0.1:5174/workflows
curl http://127.0.0.1:5174/workflows/workflow.text_plan
curl http://127.0.0.1:5174/workflows/workflow.text_plan/dependencies
```
