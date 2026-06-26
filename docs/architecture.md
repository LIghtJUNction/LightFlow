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

Core SDK support crates are different from workflow crates. `lightflow-macros`
is a root Cargo workspace member because it provides procedural macros for the
typed workflow APIs used by the core `lightflow` crate. It is backend/SDK
infrastructure, so it remains beside the core crate and is not managed through
the `projects/` sibling workflow workspace catalog.

## Standard Workflows

`lightflow.std` is a normal workflow crate, not backend code and not a hidden
built-in. Its scope is limited to abstract, domain-neutral building blocks such
as identity / passthrough, no-op control points, structural merge / split
helpers, and basic type adapters when they are broadly useful. It must not
contain agent behavior, provider integrations, model download logic, or
business templates.

The repository also ships small standard workflow nodes for prompt and image
graph composition. `lightflow.text.concat`, `lightflow.text.template`,
`lightflow.text.regex`, `lightflow.json.extract`, `lightflow.image.load`,
`lightflow.image.save`, `lightflow.image.resize`, `lightflow.image.crop`,
`lightflow.image.upscale`, `lightflow.mask.compose`, `lightflow.image.edit`,
`lightflow.image.inpaint`, `lightflow.control.if`,
`lightflow.control.switch`, `lightflow.control.merge`,
`lightflow.control.split`, `lightflow.model.select`,
`lightflow.model.lock_check`, `lightflow.llm.generate`,
`lightflow.llm.classify`, and `lightflow.llm.structured_output` are ordinary
workflow crates with agent skills and Node Schema v1 metadata, but execute
through builtin runtime capabilities. They cover common ComfyUI-style prompt
preparation, PNG artifact handling, mask composition, preview image
edit/inpaint, model selection checks, graph value routing, offline LLM
composition, and simple upscale workflows without forcing users to write a
custom Rust workflow for every adapter.

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

The native FLUX text-to-image backend is process-resident. It caches one loaded
FLUX/Qwen/VAE session keyed by the locked model paths and reuses that session
for later images in the same process. Multi-image text-to-image requests are
executed as one native batch call and then materialized to the individual
workflow output paths. This is the preferred path for ComfyUI-style operation:
run LightFlow as a long-lived process, such as `lfw serve`, so model residency
survives multiple requests. One-shot CLI commands still unload when the process
exits.

Leaf workflow execution is selected through the Executor Registry. The registry
maps runtime capabilities such as `lightflow.image.generate`,
`lightflow.image.edit`, `lightflow.image.inpaint`,
`lightflow.mask.compose`, `lightflow.text.regex`, and
`lightflow.llm.generate`, and reserved future capabilities including
`lightflow.python.node`, `lightflow.command`, `lightflow.onnx`, and
`lightflow.candle` to executor metadata and execution recipes. Builtin preview
executors keep image generation/edit/inpaint runnable offline; model-backed
FLUX and RIG executors can replace them when their feature flags or environment
are available. `lfw info` reports the same registry, so the CLI, future editor,
and planner see one executor contract.

`lfw plan <workflow_id>`, `GET /workflows/{workflow_id}/plan`, and the MCP
`lightflow.workflow.plan` tool expose that executor contract before execution.
Leaf plans include the selected executor, recipe, data policy, atoms, declared
runtimes, and model requirements the executor will plan. Composite plans expose
graph nodes in topological order with deterministic child runtime plans where a
node references a leaf workflow directly.

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
The same registry is projected through HTTP `/patches` endpoints and
`lightflow.patch.*` MCP tools so editor and agent clients can review and reuse
patches without private filesystem knowledge.

The backend service remains stateless. Run history is owned by adapters:
`lfw run`, `lfx`, and HTTP `POST /workflows/{workflow_id}/run` execute through
`ApiService`, then persist a project-local record under
`.lightflow/runs/<run_id>/`. `manifest.json` records the replay contract,
`execution.json` records the materialized result, and `events.jsonl` records
append-only trace events. Composite node executions carry input/output
snapshots, artifact handles, `duration_ms`, and `attempts`; adapters expand
those records into `node_completed` and `node_skipped` trace events between
`run_started` and `run_finished`. Completed node events include the same
selected runtime metadata as the node execution record so timeline clients can
explain executor choice directly from the event stream. Recorded executions
also include `model_locks`, a model-lock fingerprint snapshot for executed
workflows. `lfw trace` reads those files, while `lfw replay` feeds the stored
stages back through the normal runner, compares original and replayed runtime
and model-lock fingerprints, and writes a new immutable run record with a
replay report.
Failed runs follow the same storage path: adapters write
`manifest.status = "failed"`, an error object in `execution.json`, and a
`run_failed` event. Executed leaf workflow and node records include selected
runtime metadata, including executor id, executor kind, capabilities, data
policy, and declared runtime requirements. This gives editor and server
surfaces a stable failure artifact to inspect without parsing terminal output.
HTTP workflow-run failures return `run_id`, `run_dir`, and `trace_path` on the
structured error body, and MCP workflow-run failures return the same fields in
JSON-RPC error `data`.
`lfw runs list|get|rm` exposes the run directory as a small local run-history
API: list returns compact summaries, get returns full trace data, and rm deletes
one run directory.

## Installation Model

Installing a workflow means adding a Cargo dependency or creating a workflow
crate in a project or global Cargo workspace. The backend reads project
workflow crates from the current working directory's `workflows/` tree, reads
the project workflow sources listed in
`projects/lightflow-projects.toml` `[workflows].default_sources`, reads global
homes from `LFW_PATH`, scans project and global home manifests for Cargo `path`
dependencies, then statically parses any dependency crate that exposes
`pub fn define() -> WorkflowSpec` in `src/lib.rs`. This lets a project depend
on `lightflow-std = { path = "..." }` and immediately use `lightflow.std` in
`.depends_on(...)` or `.node(...)`. Remote git dependencies keep the same Cargo
manifest shape; `lfw sync` handles Cargo fetching and model/resource
synchronization.

The local `projects/` directory is a multi-repo workspace catalog, not an
unbounded implicit plugin loader. `lightflow-std` is the baseline standard node
library and is listed as the default workflow source in this repository.
Domain-specific sibling projects such as `lightflow-flux` and `lightflow-rig`
are discovered when they are added through `LFW_PATH`, `lfw import`, explicit
workflow search paths, or the configured default source list.

Local workflow collections are organized as one category level plus one crate
level, such as `std/text_plan`. Project workflows and the baseline std project
are loaded before global `LFW_PATH` workflows, and Cargo dependency workflows
are scanned after both. This keeps project-local definitions authoritative
while still allowing global and dependency-provided workflows.

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
crate, workflow id, explicit crate path, or all workflow crates, validates
manifests for common crates.io blockers, and returns the exact
`cargo publish --manifest-path ...` commands. The command is a plan by default;
`--apply` first executes Cargo's publish dry-run command and then the real
publish command. Batch workflow publishing still publishes ordinary Cargo
packages one by one, ordered so local workflow path dependencies are published
before dependents. Dirty worktrees remain blocked by Cargo unless the caller
passes `--allow-dirty`.

This keeps workflow importing and workflow publishing on the same primitive:
ordinary Rust crates.

## Release Check Model

Release checks turn release discipline into a backend contract exposed through
CLI, HTTP, and MCP. The report is a dry-run by default: it confirms required
release artifacts exist
and verifies `CHANGELOG.md` has the expected CLI, API, workflow, runtime,
known limitation, and migration sections. It also directly reviews
source-change safety, sibling project workspace health, and workflow publish
readiness, then lists the exact command gates for formatting, project and
selected-workflow loop readiness, source-change safety, sibling project
workspace inspection, strict workflow publish readiness, clippy, default tests,
repository workflow agent-skill coverage, RIG-backed LLM tests, and native FLUX
build verification.
The selected workflow gate uses `lfw loop check <workflow_id> --require-replay`
so release readiness includes completed-run replay evidence. It defaults to
`lightflow.text_plan`, and projects can override it with
`lfw release check --workflow <workflow_id>` or the `workflow_id` parameter on
the HTTP/MCP release-check surfaces. The project workspace review is separately
filterable with `--project <name>` or the `project` HTTP/MCP parameter; this
narrows the project workspace review and project catalog commands
without changing the selected workflow gate. HTTP `GET /release`,
`lightflow.release.check`, and `lightflow://release` intentionally expose only
the non-mutating dry-run report. `lightflow://release?workflow_id=<id>` and
`lightflow://release?workflow_id=<id>&project=<name>` expose the same scoped
dry-run reports through MCP resources and are advertised through
`resources/templates/list`. CLI `lfw release check --apply` executes the
planned commands in that fail-fast order and returns the same structured checks
with pass/fail status and command output tails for diagnosis. Once any release
gate fails in apply mode, later command gates are reported as skipped rather
than executed.

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

## Architecture Info

`lfw info` reports the architecture visible to the current process. It includes
the LightFlow package version, enabled build features, project root, workflow
search paths, workflow counts by category, declared runtime capabilities, model
requirement count, and known executors. Executor availability is build- and
environment-sensitive: built-in passthrough, preview image generation, and image
invert are always available; the external FLUX runner is available when
`LIGHTFLOW_FLUX_RUNNER` is set; native FLUX and RIG executors depend on the
`flux-native` and `rig` features. The registry reports status labels,
availability reasons, data policies, and model-planning flags so clients can
explain preview/mock/native/external/reserved tradeoffs without private rules.
The same catalog is available through HTTP `GET /executors` and the MCP
`lightflow.executor.list` tool/resource. `lfw arch` and `lfw architecture` are
aliases.

`lfw help <workflow_id>` reports the contract for one workflow. It returns
workflow metadata, input and output ports, dependency status, model and runtime
requirements, graph structure, and runnable input examples. `lfw workflows help
<workflow_id>` is the namespaced form. Node Schema v1 extends port metadata
with optional descriptions, required/default constraints, numeric ranges, enum
values, widget hints, artifact kinds, and model requirement bindings. The same
metadata feeds CLI help, OpenAPI, and future editor node panels.

The HTTP node directory is the editor-facing projection of the same workflow
model. `GET /nodes` returns node cards for visible workflows, including Node
Schema v1 ports, model requirements, runtime executor status, graph counts, and
validation status. `GET /nodes/{workflow_id}` returns one card, `GET
/workflows/{workflow_id}` returns the source workflow graph, `GET /executors`
returns the shared Executor Registry, and `GET /workflows/{workflow_id}/plan`
gives clients the selected runtime/model plan without creating a run. `GET
/models` returns model requirements with input/output port bindings, `lfw.lock`
status, local paths, hashes, and missing-file status. `workflow_id` and
`status=all|available|blocked` query parameters expose the same focused model
triage view used by the CLI and MCP tool. This keeps the UI palette, graph
inspection, and planner on the `ApiService` interface instead of having a
frontend infer nodes by stitching together workflow, help, info, and model data.
Runtime executor cards use the same status, reason, data-policy, and
model-planning fields as `lfw info`.
`GET /openapi.yaml` serves the checked-in OpenAPI document from the running
backend, which lets editor and tool clients discover the HTTP contract without
private repository access.
`GET /loop` and `GET /workflows/{workflow_id}/loop` expose the same local
workflow-loop readiness report as `lfw loop check` and the MCP
`lightflow.loop.check` tool, so readiness checks stay on the `ApiService`
contract instead of living in one adapter. `lightflow://loop?workflow_id=<id>`
and `lightflow://loop?workflow_id=<id>&require_replay=true` expose the same
selected-workflow readiness report for MCP resource clients. Selected workflow
reports include `replay_run_id` when a completed run is available, so editor
and agent clients can link directly to trace/replay evidence without parsing
suggested commands.
In the core repository, that report also checks the `projects/` workspace view
for expected project workspaces declared in `projects/lightflow-projects.toml`,
falling back to `lightflow-flux`, `lightflow-std`, and `lightflow-rig` when
that file is absent, plus any extra project checkouts. This keeps the
local multi-repo iteration setup visible to CLI, HTTP, MCP, and UI clients.
The same readiness report summarizes source-change safety, failing when changed
workflow files are not paired with colocated agent skill updates.
`GET /loop/changes`, `lightflow.loop.changes`, and
`lightflow://loop/changes` expose the same source-change safety report as
`lfw loop changes`, including untracked workflow files and whether each changed
workflow has a colocated agent skill update.
`GET /loop/projects?project=<name>`, MCP `lightflow.loop.projects` with
`project`, and `lightflow://loop/projects?project=<name>` narrow the sibling
project workspace catalog with the same project filter semantics; the matching
`dirty=true` query or argument returns only workspaces that need git review.
`GET /publish`, `lightflow.workflow.publish_list`, and `lightflow://publish`
expose non-mutating Cargo publish preflights for every local workflow crate in
the same dependency order used by `lfw publish --workflows`, including package
name, version, internal path dependencies, dry-run command, and static blockers.
`GET /publish?project=<name>`, MCP `lightflow.workflow.publish_list` with
`project`, and `lightflow://publish?project=<name>` narrow the same catalog to
one linked project workspace using the shared project filter semantics. The MCP
adapter advertises the parameterized resources through
`resources/templates/list`, including
`lightflow://workflows/{workflow_id}`,
`lightflow://workflows/{workflow_id}/dependencies`,
`lightflow://workflows/{workflow_id}/plan`,
`lightflow://workflows/{workflow_id}/publish`,
`lightflow://nodes/{workflow_id}`,
`lightflow://models?workflow_id={workflow_id}&status={status}`,
`lightflow://runs?workflow_id={workflow_id}&status={status}&limit={limit}`,
`lightflow://runs/{run_id}`,
`lightflow://runs/{run_id}/events`,
`lightflow://artifacts?run_id={run_id}&workflow_id={workflow_id}&kind={kind}&limit={limit}`,
`lightflow://patches/{name}`,
`lightflow://publish?project={project}`,
`lightflow://loop?workflow_id={workflow_id}`,
`lightflow://loop?workflow_id={workflow_id}&require_replay={require_replay}`,
`lightflow://loop/projects?project={project}`, and
`lightflow://loop/projects?project={project}&dirty={dirty}`.
`GET /workflows/{workflow_id}/publish` and
`lightflow.workflow.publish_check` expose the same dry-run command and static
blockers for one selected workflow crate. MCP resource clients can read the
same report from `lightflow://workflows/{workflow_id}/publish`.
Selected workflow loop readiness applies that same non-mutating publish check
to every local workflow crate in the selected workflow dependency graph, so a
composite workflow cannot appear publish-ready while a child workflow crate is
blocked.

The HTTP run-history endpoints project the same `.lightflow/runs` layout used
by the CLI. `GET /runs` lists compact run summaries, `GET /runs/{run_id}`
returns manifest, execution, and events, `GET /runs/{run_id}/events` returns
only JSONL events, `POST /runs/{run_id}/replay` executes stored manifest stages
into a new immutable run, `DELETE /runs/{run_id}` removes one local run
directory, and `GET /artifacts` aggregates artifact handles from recorded
executions. CLI and server adapters write the same durable artifacts instead of
inventing separate history stores.
The static editor client consumes those endpoints directly to render run
summaries, event timelines with runtime badges, stage rows, node trace rows,
replay drift, run deletion, raw trace JSON, and per-run artifact rows. Its run
form submits Node Schema inputs plus optional `disabled_nodes`, `enabled_nodes`,
and patch JSON through the same
`WorkflowExecutionOptions` body used by CLI, HTTP, and MCP run surfaces.
The model catalog view renders `/models` lock status directly, including
variant, format, hash, local paths, and missing paths, with workflow and status
filters for focused model-lock triage, so model reproducibility problems are
visible without filesystem-specific client logic.
The loop view renders `/loop`, `/loop/changes`, and `/loop/projects`, and each
selected node can load `/workflows/{workflow_id}/loop`, so readiness failures,
source-change review blockers, sibling workspace catalog state, and replay/publish
warnings are visible before a user leaves the editor. The dashboard loads
project-level readiness panels independently after the core catalogs, so a
single release, publish, or loop-readiness endpoint failure is shown in its own
panel instead of hiding nodes, runs, artifacts, or patches.
The selected node detail also renders `/workflows/{workflow_id}/publish`,
including the Cargo dry-run command and static blockers for that workflow
crate.
The same loop view renders project-level `/release`, while selected workflow
details load `/release?workflow_id=<workflow_id>`, so required release
artifacts, changelog sections, and planned release gate commands are visible
without executing them. Release reports include top-level status counts for
passed, warning, failed, planned, and skipped gates, so editor clients can show
compact readiness badges from the contract without re-counting every check.
Individual release checks can also expose a `count` for the row, such as
changed workflows, linked workspaces, workflow crates, or one planned command.

## Boundaries

`src/api.rs` is the framework-independent service. CLI, HTTP, and MCP call this
service instead of owning behavior.

`src/workflow.rs` holds the public workflow domain types and Rust DSL builder.

`src/server.rs` is only an HTTP adapter.

`src/cli/mcp.rs` is the JSON-RPC/MCP adapter shared by the `lfw mcp` subcommand
and the HTTP `/mcp` route. It projects the same backend categories as HTTP:
workflow tools, node catalog tools, executor catalog tools, model catalog
tools, run history/replay tools, artifact tools, publish-readiness tools, and
the `lightflow://openapi` contract resource. MCP-triggered workflow runs are
recorded in
`.lightflow/runs` with `surface: "mcp"` trace events.
