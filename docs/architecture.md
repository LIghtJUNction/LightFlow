# Architecture

LightFlow is a backend-only AI workflow system. Its core design treats pipeline structure as code assets, not serialized canvas state. Project workflows are assets, while the engine may ship built-in assets.

## Product Thesis

ComfyUI is optimized for human-visible experimentation. That is valuable during feedback: a person can inspect an image, move a slider, change a parameter, and quickly decide whether the result is better.

The bottleneck is the workflow construction phase. Large visual graphs often encode logic that maps cleanly to code: branching, data conversion, typed function calls, model loading, reusable subroutines, and validation. Agents are better suited to that construction work than to visual taste judgment.

LightFlow separates those responsibilities. Agents author and repair workflows as self-contained Rust asset files. Humans direct goals and keep the immediate feedback loop where visual judgment matters most.

## Platform Target

LightFlow is Linux-first.

The core runtime should optimize for Linux servers and Linux workstations: XDG directories, Unix sockets, process ownership, file permissions, local daemons, server deployment, and backend APIs. Cross-platform local runtime support is not a design constraint for the initial architecture.

Other systems can use LightFlow through a network API exposed by a Linux host. That means HTTP/OpenAPI is a remote access boundary, not a reason to flatten the Linux runtime model.

CortexFS is the required Linux execution substrate. LightFlow assumes CortexFS is available as a submodule and mounted at `/ctx` on the target Linux host. LightFlow should use CortexFS for provider/model/tool/MCP/thread/policy/audit execution surfaces instead of building parallel local abstractions.

CortexFS is a userspace/FUSE boundary for LightFlow. LightFlow is not a Linux kernel subsystem, and `/ctx` is not a proposed kernel ABI. Kernel-facing work is limited to separately justified generic Linux primitives, as defined in [kernel-policy.md](kernel-policy.md).

## Concept Model

Node:

- smallest reusable pipeline unit
- represented by one Rust asset file under `lightflow/nodes/`
- expected to be understandable and editable by agents
- contains metadata and definition in the same file
- discovered by scanning project assets and built-in assets

Composition:

- reusable group of nodes
- represented by one Rust asset file under `lightflow/compositions/`
- used to encode common pipeline patterns without turning them into hidden runtime magic
- contains metadata and definition in the same file
- discovered by scanning project assets and built-in assets

Workflow:

- top-level pipeline definition
- represented by one Rust asset file under `lightflow/workflows/`
- intended to be generated, reviewed, and changed as normal code
- contains metadata and definition in the same file
- discovered by scanning project assets and built-in assets

Model:

- project-owned asset for a model alias, provider, capabilities, and artifact expectations
- represented by one Rust asset file under `lightflow/models/`
- contains metadata and resolver hints in the same file
- discovered by scanning `lightflow/models/*.rs`
- does not imply that heavyweight model weights are committed to the repository

Run:

- runtime execution record for a workflow request
- stored under the XDG state directory by default
- not committed unless reduced into an explicit fixture or documentation example

## Data Ownership

LightFlow has four zones:

- `src/` is engine source: API code, asset loading, validation, runtime implementation, provider adapters, and execution plumbing.
- `src/builtins/` is for LightFlow-owned built-in node, composition, and workflow assets that ship with the engine.
- `lightflow/` is versioned project assets: self-contained Rust model, node, composition, and workflow files plus presets, policies, and small fixtures.
- `cortexfs/` is the vendored CortexFS submodule that defines the Linux filesystem execution ABI.
- XDG directories hold user-local config, state, cache, runtime sockets, locks, and secrets.

Asset files are the source of truth. Metadata must live inside the same `.rs` file as the executable definition so an agent can review, move, copy, or delete one complete asset without chasing sidecar files.

Built-in assets are allowed, but they are part of LightFlow's distribution. User/project assets should stay under `lightflow/` so workflow work remains separate from engine implementation work.

Full rules are defined in [data-layout.md](data-layout.md).

## Backend Boundary

The backend should expose an OpenAPI-compatible contract. API shape should be stable enough for future UIs, CLIs, agents, and non-Linux clients to inspect workflows, request runs, and observe results from a Linux-hosted LightFlow server without depending on an internal Rust API.

The API should read inspectable project assets through the LightFlow asset loader and runtime state from the XDG state directory. Clients should not parse Rust source themselves; the backend owns asset scanning, validation, and metadata extraction.

When a workflow step needs an AI model, tool, MCP tool, thread, route, policy, or audit surface, the runtime should go through `/ctx`, not a separate LightFlow provider subsystem. LightFlow run records should store CortexFS request ids, route metadata, fingerprints, response paths, and audit correlation fields.

The code boundary follows that split:

- `src/cortex.rs` plans CortexFS paths, commits request files with atomic rename semantics, and reads CortexFS outbox files.
- `src/runs.rs` stores LightFlow-owned run manifests under XDG state.
- `src/asset.rs` discovers self-contained Rust asset metadata without sidecar registries.
- `src/api.rs` maps OpenAPI operations to framework-independent service methods.
- `openapi/lightflow.yaml` exposes run manifests and CortexFS exchange paths without exposing internal Rust APIs.

The boundary is queryable through `lightflow ctx abi`, `GET /ctx/abi`, and `lightflow://ctx-abi`.

## Non-Goals

- No frontend in the initial project.
- No visual canvas assumptions in the backend model.
- No concrete execution engine is defined at initialization time.
- No provider-specific AI integration is part of the initial skeleton.
- No non-Linux local runtime support in the initial project.
- No optional CortexFS mode; CortexFS is required for the Linux runtime path.
- No attempt to upstream LightFlow, CortexFS product protocols, provider routing, model aliases, MCP, HTTP, OpenAPI, or JSON workflow contracts into the Linux kernel tree.

## Early Repository Rules

- Keep each model, node, composition, and workflow file readable as a standalone asset.
- Keep asset metadata and executable definition together in one `.rs` file.
- Keep project workflow/node/composition assets outside `src/`; `src/` is for the LightFlow engine and built-ins that ship with it.
- Put shipped built-ins under `src/builtins/`, not mixed into runtime plumbing modules.
- Use `src/<module>.rs` module entries instead of `mod.rs`.
- Keep API contract files separate from runtime implementation.
- Avoid committing to a web framework before the API surface is clearer.
- Prefer Linux-native runtime mechanisms over cross-platform abstractions in core design.
- Use `/ctx` as the canonical CortexFS mount point.
- Integrate with CortexFS instead of duplicating provider, model, tool, policy, thread, or audit concepts.
- Keep heavyweight model weights outside Git; commit manifests and aliases instead.
- Keep real run output under the XDG state directory, not under `lightflow/`.
- Regenerate local indexes under the XDG cache directory if needed; do not use committed sidecar registries as the source of truth.
