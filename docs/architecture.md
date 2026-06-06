# Architecture

LightFlow is a backend-only AI workflow system. Its core design treats pipeline structure as source code, not serialized canvas state.

## Concept Model

Node:

- smallest reusable pipeline unit
- represented by one Rust file
- expected to be understandable and editable by agents

Composition:

- reusable group of nodes
- represented by one Rust file
- used to encode common pipeline patterns without turning them into hidden runtime magic

Workflow:

- top-level pipeline definition
- represented by one Rust file
- intended to be generated, reviewed, and changed as normal code

## Backend Boundary

The backend should expose an OpenAPI-compatible contract. API shape should be stable enough for future UIs, CLIs, and agents to inspect workflows, request runs, and observe results without depending on an internal Rust API.

## Non-Goals

- No frontend in the initial project.
- No visual canvas assumptions in the backend model.
- No concrete execution engine is defined at initialization time.
- No provider-specific AI integration is part of the initial skeleton.

## Early Repository Rules

- Keep each node, composition, and workflow file readable as a standalone artifact.
- Prefer explicit Rust modules over generated opaque config.
- Use `src/<module>.rs` module entries instead of `mod.rs`.
- Keep API contract files separate from runtime implementation.
- Avoid committing to a web framework before the API surface is clearer.
