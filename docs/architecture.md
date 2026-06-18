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
- optional default-disabled nodes

There is no separate component concept. A leaf workflow is the replacement for
what would otherwise become a component.

## Workflow Crates

Workflows are source-controlled Rust library crates under
`workflows/<category>/<short-name>/`. Reusable workflows define
`src/lib.rs` and do not define `src/main.rs`.
Metadata and graph structure live in the library entrypoint:

```rust
use lightflow::preload::*;

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

Workflow crates are reusable libraries by default. `lfw init --workflow`
creates the collection project, and `lfw new` creates one workflow crate inside
a required category.

## Standard Workflows

`lightflow.std` is a normal workflow crate, not backend code and not a hidden
built-in. Its scope is limited to abstract, domain-neutral building blocks such
as identity / passthrough, no-op control points, structural merge / split
helpers, and basic type adapters when they are broadly useful. It must not
contain agent behavior, provider integrations, model download logic, or
business templates.

The repository dogfoods this model: `lightflow.text_plan` declares an exact
dependency on `lightflow.std` and includes a `lightflow.std` node in its graph.

## Validation

The backend validates:

- id values are safe path segments
- workflow names and SemVer versions are present
- port names are non-empty and unique per direction
- referenced workflows exist
- a workflow does not directly nest itself
- edge source and target ports exist
- workflow graph edges are acyclic
- declared dependency versions match local workflow versions

Version matching is intentionally strict in the current iteration: dependency
requirements are either exact SemVer strings such as `0.1.0` or `*`. SemVer
ranges such as `^0.1` and `>=0.1` are reserved for a later update path.

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

## Execution

The first execution engine is intentionally small. It validates the workflow,
uses the graph topological order as the schedule, executes leaf workflows with
passthrough semantics, and records each node as `completed` or `skipped`.

Execution accepts temporary toggles:

```bash
lfx lightflow.text_plan --input value=hello --disable prompt
lfw run lightflow.text_plan --input value=hello --disable prompt --enable prompt
```

`--disable <node>` skips a node for that run only. `--enable <node>` cancels a
temporary or author-time disable for that run. The same engine is exposed over
HTTP at `POST /workflows/{workflow_id}/run`, through the `lfw mcp` JSON-RPC
subcommand, and through the MCP `lightflow.workflow.run` tool served at
`POST /mcp`.

The current runner deliberately does not implement provider routing, remote
execution substrates, or agent behavior. FLUX image generation, edit, and
inpaint are the first runtime adapter boundary: the core resolves workflow
inputs and synced model paths, then runs the native Rust `flux-native` backend
when compiled in. Builds without that feature can call `LIGHTFLOW_FLUX_RUNNER`
with the same stable task contract. The selected backend owns sampling and
writes the PNG artifact.

LLM text generation is the second runtime adapter boundary. Workflows that
declare `lightflow.llm.generate` are executed by the RIG adapter when LightFlow
is compiled with `--features rig`. The workflow carries provider-neutral inputs
such as `provider`, `model`, `prompt`, `system`, `api_key`, `base_url`,
`temperature`, `max_tokens`, and `additional_params`; the adapter maps those to
`rig-core` clients for OpenAI-compatible chat APIs, OpenAI Responses,
Anthropic, Ollama, OpenRouter, DeepSeek, and xAI. Without the feature, the
runtime reports a clear configuration error.

The FLUX boundary is designed around zero-copy handoff at the LightFlow layer.
Workflows, lockfiles, and execution plans carry paths and artifact handles, not
model bytes or tensor payloads. Native FLUX generation enables mmap for GGUF
weights, reads model files directly from the Hugging Face cache paths recorded
in `lfw.lock`, and keeps image inputs as file paths until the backend has to
decode them for sampling. LightFlow should not copy model artifacts into a
project directory or serialize large intermediate tensors through workflow
JSON.

Typed Rust workflows add a second execution surface for code-first workflow
projects. The public composition boundary is `Workflow<I, O>` / `Runnable<I,
O>`: tasks, tools, and sub-workflows all share the same typed `run(input) ->
Output` contract, so `Workflow<A, B>.then(Workflow<B, C>)` produces
`Workflow<A, C>` and mismatched composition fails at compile time. Internal
state machines use `ContextWorkflow`; context remains private to one workflow
and is not the cross-workflow data model.

Patch and hook behavior is applied at node call boundaries. `NodeHook<I, O>`
provides before/after/error hooks, `AroundHook<I, O>` wraps execution through a
typed `Next<I, O>`, and `HookRegistry<I, O>` records the patch set for a node
shape. This keeps Rust source as the source of truth while allowing runner,
editor, and test tooling to apply logging, metrics, retry, timeout, mock,
disable, and replacement behavior without editing workflow source. The concrete
SDK operations are `replace`, `disable_with`, `retry`, `timeout`, and
`timeout_ms`, all checked against the node's `I -> O` type.

The graph runner exposes the same idea as serializable CLI patches:
`lfw run ... --patch <json|-|@file>` can enable, disable, retry, time-limit,
replace a graph node with another discovered workflow id, or route a disabled
node to a fallback workflow id. This patch format is deliberately data-only so
it can be stored in `manifest.json`, replayed later, and edited by tooling.
Direct Rust closure/function replacement remains the typed SDK
`HookRegistry` path.

Project-local patch registry entries live under `.lightflow/patches` and are
managed with `lfw patch save|get|list|validate|rm`. `lfw run --patch <name>`
expands a registry entry before execution; the run manifest stores the expanded
patch rather than a mutable registry reference.

The backend service remains stateless. Run history is owned by the CLI runner:
`lfw run` and `lfx` execute through `ApiService`, then persist a project-local
record under `.lightflow/runs/<run_id>/`. `manifest.json` records the replay
contract, `execution.json` records the materialized result, and `events.jsonl`
records append-only trace events. Composite node executions carry input/output
snapshots, artifact handles, `duration_ms`, and `attempts`; the CLI expands
those records into `node_completed` and `node_skipped` trace events between
`run_started` and `run_finished`. `lfw trace` reads those files, while `lfw
replay` feeds the stored stages back through the normal runner and writes a new
immutable run record.
Failed runs follow the same storage path: the CLI exits non-zero after writing
`manifest.status = "failed"`, an error object in `execution.json`, and a
`run_failed` event. This gives editor and server surfaces a stable failure
artifact to inspect without parsing terminal output.
`lfw runs list|get|rm` exposes the run directory as a small local run-history
API: list returns compact summaries, get returns full trace data, and rm deletes
one run directory.

## Installation Model

Installing a workflow means adding a Cargo dependency or creating a workflow
crate in a project or global Cargo workspace. The backend reads project
workflow crates from the current working directory's `workflows/` tree, reads
global homes from `LFW_PATH`, scans project and global home manifests for Cargo
`path` dependencies, then statically parses any dependency crate that exposes
`pub fn define() -> WorkflowSpec` in `src/lib.rs`. This lets a project depend
on `lightflow-std = { path = "..." }` and immediately use `lightflow.std` in
`.depends_on(...)` or `.node(...)`. Remote git dependencies keep the same Cargo
manifest shape; `lfw sync` handles Cargo fetching and model/resource
synchronization.

Local workflow collections are organized as one category level plus one crate
level, such as `std/text_plan`. Project workflows are loaded before global
`LFW_PATH` workflows, and Cargo dependency workflows are scanned after both.
This keeps project-local definitions authoritative while still allowing global
and dependency-provided workflows.

Workflow crates and plugin crates are both standard Rust packages that import
`lightflow`; the core `lightflow` crate does not import them.

`lfw init` writes `$XDG_CONFIG_HOME/lightflow/.lfwrc`, defaulting to
`~/.config/lightflow/.lfwrc`, and appends a shell-specific source line to the
detected bash, zsh, or fish startup file. At runtime `lfw` reads `LFW_PATH`
from the process environment. If it is not set, the default workflow home is
`$XDG_DATA_HOME/lightflow`, or `~/.local/share/lightflow` when `XDG_DATA_HOME`
is not set. `lfw home` prints the active home, its `Cargo.toml`, its
`workflows/` source tree, and its repo cache.

## Publishing Model

Publishing is intentionally delegated to Cargo. `lfw publish` selects a root
crate, workflow id, or explicit crate path, validates the manifest for common
crates.io blockers, and returns the exact `cargo publish --manifest-path ...`
command. The command is a plan by default; `--apply` executes it.

This keeps workflow installation and workflow publishing on the same primitive:
ordinary Rust crates.

## Sync Model

`lfw sync` separates module dependencies from model resources:

- module dependencies are resolved by Cargo, currently planned as `cargo fetch`
- model resources are declared as workflow metadata and downloaded through the
  Hugging Face CLI

Model declarations are capability-oriented. A workflow can say it needs an
`image_model` with a `text-to-image` capability and list concrete HF variants
such as safetensors or GGUF. `lfw sync` does not pick or download every variant
automatically; users must select a variant with `--model image_model=<variant>`.
This keeps large model artifacts out of the repository and lets HF manage
deduplication, sharding, compression, and cache placement.

## Boundaries

`src/api.rs` is the framework-independent service. CLI, HTTP, and MCP call this
service instead of owning behavior.

`src/workflow.rs` holds the public workflow domain types and Rust DSL builder.

`src/server.rs` is only an HTTP adapter.

`src/cli/mcp.rs` is the JSON-RPC/MCP adapter shared by the `lfw mcp` subcommand
and the HTTP `/mcp` route.
