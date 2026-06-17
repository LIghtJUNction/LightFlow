# Architecture

LightFlow currently has two domain concepts.

## Component

A component is a reusable leaf building block. It declares:

- stable `id`
- display `name`
- optional `description`
- typed input ports
- typed output ports
- optional JSON config schema

Components are stored as source-controlled specs under
`lightflow/components/*.json`.

## Workflow

A workflow is a directed graph. It declares:

- stable `id`
- display `name`
- optional `description`
- public input ports
- public output ports
- graph nodes
- directed edges

Each workflow node uses either:

- a component by `component_id`
- another workflow by `workflow_id`

That is the only composition mechanism. There is no separate composition asset
type.

## Validation

The backend validates:

- id values are safe path segments
- component and workflow names are present
- port names are non-empty and unique per direction
- referenced components exist
- referenced workflows exist
- a workflow does not directly nest itself
- edge source and target ports exist
- workflow graph edges are acyclic

The current validation deliberately does not implement execution scheduling,
provider routing, or agent behavior.

## Boundaries

`src/api.rs` is the framework-independent service. CLI, HTTP, and MCP call this
service instead of owning behavior.

`src/server.rs` is only an HTTP adapter.

`src/mcp.rs` is only a JSON-RPC/MCP adapter for external tools.

`src/component.rs` and `src/workflow.rs` hold the public domain types.
