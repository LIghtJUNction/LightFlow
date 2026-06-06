# LightFlow

Lightweight AI Pipelines. Built by Agent, Directed by You.

LightFlow is a backend-first alternative to ComfyUI. Instead of asking users to build AI workflows by dragging boxes around a canvas, LightFlow treats workflows as code that agents can read, write, review, and evolve.

## Positioning

LightFlow is for AI pipeline authorship where:

- a node is a Rust file
- a composition is a Rust file
- a workflow is a Rust file
- agents generate and modify pipeline code directly
- humans direct intent, constraints, and review
- the backend exposes an OpenAPI-compatible surface

The frontend is intentionally out of scope for now.

## Scope

Current scope:

- Rust backend project foundation
- node / composition / workflow source layout
- OpenAPI-first backend contract direction
- minimal compileable crate scaffold

Not in scope yet:

- frontend UI
- node runtime implementation
- workflow scheduler
- model provider integrations
- persistence layer
- auth / permissions
- concrete API handler logic

## Project Shape

```text
src/
  api.rs           # OpenAPI-facing backend boundary
  nodes.rs         # Node module entry, no mod.rs
  nodes/           # Single-file Rust nodes, added as needed
  compositions.rs  # Composition module entry, no mod.rs
  compositions/    # Single-file Rust reusable node compositions, added as needed
  workflows.rs     # Workflow module entry, no mod.rs
  workflows/       # Single-file Rust executable workflow definitions, added as needed
docs/
  architecture.md  # Product and architecture intent
openapi/
  lightflow.yaml   # API contract placeholder
```

## Design Principle

LightFlow should make the natural authoring path for AI agents be the same path engineers use: write Rust files, compose typed building blocks, and expose inspectable backend contracts.
