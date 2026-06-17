# Architecture

LightFlow currently has one domain concept: workflow.

## Workflow

A workflow can be either:

- a reusable leaf unit with typed input and output ports
- a composite directed graph whose nodes reference other workflows

A workflow declares:

- stable `id`
- semantic `version`
- display `name`
- optional `description`
- public input ports
- public output ports
- optional explicit workflow dependencies
- graph nodes
- directed edges between node ports

There is no separate component concept. A leaf workflow is the replacement for
what would otherwise become a component.

## Workflow Crates

Workflows are source-controlled Rust library crates under `lightflow/workflows/`.
Reusable workflows define `src/lib.rs` and do not define `src/main.rs`.
Metadata and graph structure live in the library entrypoint:

```rust
use lightflow::workflow::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.example")
        .version("0.1.0")
        .name("Example")
        .input("value", "json")
        .output("value", "json")
        .build()
}
```

The backend statically parses the supported builder DSL from the Rust AST. It
does not compile or execute workflow files.

A future executable workflow entrypoint can be marked by adding `src/main.rs`.
Until then, `lfw init` and `lfw add` generate reusable library workflows only.

## Standard Workflows

`lightflow.std` is a normal workflow crate, not backend code and not a hidden
built-in. Its scope is limited to abstract, domain-neutral building blocks such
as identity / passthrough, no-op control points, structural merge / split
helpers, and basic type adapters when they are broadly useful. It must not
contain agent behavior, provider integrations, model download logic, or
business templates.

## Validation

The backend validates:

- id values are safe path segments
- workflow names and versions are present
- port names are non-empty and unique per direction
- referenced workflows exist
- a workflow does not directly nest itself
- edge source and target ports exist
- workflow graph edges are acyclic
- declared dependency versions match local workflow versions

## Dependency Resolution

The backend resolves workflow dependencies recursively from a root workflow.
The report includes:

- reachable workflows, including the root workflow
- resolved local workflow versions
- dependency-first workflow order
- missing workflow ids
- version mismatches
- workflow nesting cycles

The command-line form is:

```bash
lfw deps lightflow.text_plan
```

The current validation deliberately does not implement execution scheduling,
provider routing, or agent behavior.

## Installation Model

Installing a workflow means adding a Cargo dependency. The backend reads local
workflow crates and Cargo `path` dependencies, then statically parses any
dependency crate that exposes `pub fn define() -> WorkflowSpec` in `src/lib.rs`.
This lets a project depend on `lightflow-std = { path = "..." }` and immediately
use `lightflow.std` in `.depends_on(...)` or `.node(...)`. Remote git
dependencies keep the same Cargo manifest shape; `lfw sync` will handle fetching
and model/resource synchronization in a later iteration.

## Boundaries

`src/api.rs` is the framework-independent service. CLI, HTTP, and MCP call this
service instead of owning behavior.

`src/workflow.rs` holds the public workflow domain types and Rust DSL builder.

`src/server.rs` is only an HTTP adapter.

`src/mcp.rs` is only a JSON-RPC/MCP adapter for external tools.
