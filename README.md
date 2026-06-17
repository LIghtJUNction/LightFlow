# LightFlow

LightFlow is a backend-first workflow system. The current backend deliberately
keeps the domain model small:

- Component: a reusable leaf building block with typed input and output ports.
- Workflow: a directed graph that can use components or nest other workflows.

There is no built-in agent loop and no visual-editor-owned workflow format.
Workflow files live in the repository so normal coding tools, including Codex,
can edit and review them.

## Current Scope

- component and workflow specs under `lightflow/`
- workflow validation, including nested workflow references and DAG cycle checks
- CLI, HTTP, and MCP surfaces over the same backend service

## Out Of Scope

- external execution substrates as runtime dependencies
- model/node/composition as separate top-level concepts
- built-in agent planning
- frontend implementation
- workflow execution engine

## Layout

```text
src/
  api.rs         # framework-independent service
  component.rs   # component domain types
  workflow.rs    # workflow domain types
  mcp.rs         # MCP JSON-RPC adapter
  server.rs      # HTTP adapter
lightflow/
  components/    # source-controlled component specs
  workflows/     # source-controlled workflow specs
openapi/
  lightflow.yaml # API contract
```

## CLI

```bash
cargo run -- components list
cargo run -- components get component.text_prompt
cargo run -- workflows list
cargo run -- workflows get workflow.text_plan
cargo run -- workflows validate @lightflow/workflows/workflow.text_plan.json
cargo run -- serve --port 5174
```

## HTTP

```bash
curl http://127.0.0.1:5174/components
curl http://127.0.0.1:5174/workflows
curl -X POST http://127.0.0.1:5174/workflows/validate \
  -H 'content-type: application/json' \
  --data @lightflow/workflows/workflow.text_plan.json
```
