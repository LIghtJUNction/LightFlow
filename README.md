# LightFlow

Lightweight AI Pipelines. Built by Agent, Directed by You.

LightFlow is a backend-first alternative to ComfyUI. Instead of asking users to build AI workflows by dragging boxes around a canvas, LightFlow treats workflows as code that agents can read, write, review, and evolve.

ComfyUI proved the value of direct, visual feedback. Humans can look at an output, move a slider, change a seed, adjust a prompt, and immediately tell whether the result improved. That loop is fast, natural, and still hard for agents to match.

But the same human-first canvas becomes expensive when the task is building the workflow itself. Complex node graphs and long chains of links often translate into ordinary control flow, typed inputs, function calls, and reusable modules. What looks visually complicated can be much simpler as code.

LightFlow is built for that split:

- agents handle the setup work: reading docs, installing models, wiring nodes, fixing shape mismatches, and assembling workflows as Rust code
- humans keep the high-value feedback loop: judging outputs, changing intent, tuning parameters, and directing revisions

You should not need to buy an expensive workflow or search through layers of menus just to get started. Give an agent the docs, skills, and requirements; let it build the backend workflow; then use the generated API surface to run, inspect, and refine it.

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
