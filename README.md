# LightFlow

LightFlow is a backend-first, source-controlled workflow environment for
human-directed AI pipelines. The current backend deliberately keeps the domain
model small:

- Workflow: a reusable leaf unit or a directed graph that nests other workflows.

There is no autonomous built-in agent planner, no CortexFS runtime dependency,
and no visual-editor-owned workflow format. Workflows are Rust library crates
in the repository so normal coding tools, including Codex, can edit and review
them. LightFlow provides the local loop around that source model: validate,
run, inspect, patch, replay, and publish through shared CLI, HTTP, MCP, and
editor-facing contracts.

## Current Scope

- Rust workflow crates under `workflows/<category>/<short-name>/`
- workflow validation, including nested workflow references and DAG cycle checks
- recursive workflow dependency resolution
- lightweight workflow execution plans with temporary node toggles
- CLI, HTTP, and MCP surfaces over the same backend service

## Out Of Scope

- external execution substrates as runtime dependencies
- component/model/node/composition as separate top-level concepts
- built-in agent planning
- frontend-owned workflow source formats

## Layout

```text
src/
  api.rs         # framework-independent service
  workflow.rs    # workflow domain types and Rust DSL builder
  cli.rs         # command-line interface
  cli/mcp.rs     # MCP JSON-RPC adapter and CLI subcommand
  server.rs      # HTTP adapter
workflows/       # categorized Rust workflow crates
openapi/
  lightflow.yaml # API contract
LightFlowUI/     # static backend-backed editor client
```

## Rust Workflow Crates

Reusable workflows are library crates with `src/lib.rs` and no `src/main.rs`:

```rust
use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
        .name("Text Plan")
        .input("value", "json")
        .input_description("value", "Structured request payload.")
        .input_required("value", true)
        .input_widget("value", "json")
        .output("result", "text")
        .output_description("result", "Generated text result.")
        .depends_on("lightflow.text_prompt", "0.1.0")
        .depends_on("lightflow.text_result", "0.1.0")
        .node("prompt", "lightflow.text_prompt")
        .node("result", "lightflow.text_result")
        .edge("prompt", "prompt", "result", "text")
        .build()
}
```

`workflow!()` reads `CARGO_PKG_NAME` and `CARGO_PKG_VERSION` in the workflow
crate. A package named `lightflow-text-plan` therefore owns workflow id
`lightflow.text_plan` at the package version; workflow source does not repeat or
override either value.

The backend parses this DSL statically from Rust ASTs; it does not execute or
compile workflow source files.

Ports can include Node Schema v1 metadata such as descriptions,
required/default constraints, numeric ranges, enum values, widget hints,
artifact kinds, and model requirement bindings. That metadata is used by
`lfw help`, OpenAPI, and future editor node panels.

For the full workflow authoring path, see
[Workflow Development Guide](docs/workflow-development.md). It covers creating
workflow projects, adding workflow dependencies, writing leaf and runtime-backed
workflows, composing workflows with `.node()` / `.edge()`, and validating nodes
with `lfw node test`.

For the longer product and architecture direction, see
[Long-Term Goals](docs/long-term-goals.md).
For the near-term local authoring, run, inspect, replay, and publish loop, see
[Local Workflow Loop](docs/local-workflow-loop.md).

## Quickstart

Clone with submodules, or initialize them before running the default workflow
catalog:

```bash
git clone --recurse-submodules https://github.com/lightjunction/LightFlow.git
git submodule update --init --recursive
```

```bash
cargo run --bin lfw -- init --workflow
cargo run --bin lfw -- new demo_echo --category demo --name "Demo Echo"
cargo run --bin lfw -- run lightflow.demo_echo --input value='"hello"'
cargo run --bin lfw -- serve --port 5174
```

After `lfw serve` starts, inspect the backend contract used by editor clients:

```bash
curl http://127.0.0.1:5174/ui
curl http://127.0.0.1:5174/openapi.yaml
curl http://127.0.0.1:5174/nodes
curl http://127.0.0.1:5174/runs
```

When `LightFlowUI/` is present, the same server also serves the static editor
at `http://127.0.0.1:5174/ui`.

## CLI

```bash
cargo run --bin lfw -- init --workflow
cargo run --bin lfw -- init --plugin
cargo run --bin lfw -- new my_flow --category std --name "My Flow"
cargo run --bin lfw -- new my_flux_sampler --category image --runtime lightflow.image.generate
cargo run --bin lfw -- new my_global_flow --category std --global
cargo run --bin lfw -- info
cargo run --bin lfw -- home
cargo run --bin lfw -- add lightflow-text-prompt --version 0.1.0
cargo run --bin lfw -- add lightflow-text-prompt --path projects/lightflow-std/workflows/std/text_prompt --editable
cargo run --bin lfw -- add lightflow-text-prompt --version 0.1.0 --global
cargo run --bin lfw -- import projects/lightflow-flux --global
cargo run --bin lfw -- list
cargo run --bin lfw -- list --categories
cargo run --bin lfw -- ls --detail
cargo run --bin lfw -- workflows list
cargo run --bin lfw -- workflows get lightflow.text_plan
cargo run --bin lfw -- help lightflow.text_plan
cargo run --bin lfw -- workflows help lightflow.text_plan
cargo run --bin lfw -- deps lightflow.text_plan
cargo run --bin lfw -- plan lightflow.text_plan
cargo run --bin lfw -- run lightflow.text_plan --input value='{"topic":"demo"}'
cargo run --bin lfw -- run lightflow.text_plan --input value='{"topic":"demo"}' --patch @patch.json
cargo run --bin lfw -- patch save qa-debug @patch.json
cargo run --bin lfw -- run lightflow.text_plan --input value='{"topic":"demo"}' --patch qa-debug
cargo run --bin lfw -- trace last
cargo run --bin lfw -- runs list
cargo run --bin lfw -- runs list --limit 20 --workflow lightflow.text_plan --status completed
cargo run --bin lfw -- runs get last
cargo run --bin lfw -- artifacts
cargo run --bin lfw -- artifacts --run last --kind image --limit 20
cargo run --bin lfw -- replay
cargo run --bin lfw -- replay run-1781797000000
cargo run --bin lfx -- lightflow.text_plan --input value='{"topic":"demo"}' --disable prompt
cargo run --bin lfw -- workflows validate '{"id":"lightflow.example","version":"0.1.0","name":"Example"}'
cargo run --bin lfw -- node test lightflow.text_to_image
cargo run --bin lfw -- models requirements
cargo run --bin lfw -- models requirements lightflow.text_to_image
cargo run --bin lfw -- models requirements --blocked
cargo run --bin lfw -- publish lightflow.text_prompt
cargo run --bin lfw -- loop check lightflow.text_plan
cargo run --bin lfw -- release check
cargo run --bin lfw -- mcp '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
cargo run --bin lfw -- mcp '{"jsonrpc":"2.0","id":2,"method":"tools/call","params":{"name":"lightflow.model.list","arguments":{}}}'
cargo run --bin lfw -- serve --port 5174
curl http://127.0.0.1:5174/nodes
curl http://127.0.0.1:5174/nodes/lightflow.text_to_image
curl http://127.0.0.1:5174/executors
curl http://127.0.0.1:5174/models
curl 'http://127.0.0.1:5174/models?workflow_id=lightflow.text_to_image&status=blocked'
curl http://127.0.0.1:5174/runs
curl 'http://127.0.0.1:5174/runs?limit=20&workflow_id=lightflow.text_plan&status=completed'
curl http://127.0.0.1:5174/runs/last/events
curl -X POST http://127.0.0.1:5174/runs/last/replay
curl http://127.0.0.1:5174/artifacts
```

`lfw new --runtime lightflow.image.generate` generates a runnable deterministic
preview scaffold backed by `builtin.preview.v1`. It is intended to verify the
workflow pipeline and PNG artifact path, not FLUX, ComfyUI, or production model
quality. A real model backend must replace the preview runtime with its own
backend contract and declare the concrete model requirements it needs.

`lfw new --runtime lightflow.comfyui.workflow` generates a separate
`comfyui.api.v1` scaffold for a real ComfyUI HTTP API handoff. Its required
`workflow` input is an inline graph exported with ComfyUI **Save (API Format)**,
not the editor/UI workflow JSON. `node_inputs` can override arbitrary prompt,
seed, sampler, model, control, or custom-node inputs. `uploads` can send images
and masks to `/upload/image` and bind returned references to node inputs.
LightFlow then submits `/prompt`, polls `/history/<prompt_id>`, recursively
discovers file descriptors, and downloads them through `/view`. This supports
text-to-image, image-to-image, inpainting, and other installed ComfyUI graph
shapes without hard-coding their node types. ComfyUI remains responsible for
models, custom nodes, hardware scheduling, and model quality. Registry
availability means only that this LightFlow build has the executor; the
endpoint is checked when the workflow runs. Local mock-HTTP tests cover the
protocol, but no real ComfyUI endpoint or model quality is claimed as verified.
Upload paths and `output_dir` are confined to the LightFlow project root;
canonical path checks reject `..` traversal and symlinks that escape it.
`LIGHTFLOW_COMFYUI_AUTHORIZATION` is sent only when the resolved endpoint has
the same origin as configured `LIGHTFLOW_COMFYUI_URL`. The total timeout covers
upload hashing, streaming multipart upload, submit, polling, and streamed
download. Downloads use create-new temporary files and no-clobber persistence,
so an existing artifact is never overwritten.

HTTP workflow runs, replays, and HTTP MCP calls execute in a bounded blocking
pool so `/health` remains responsive. `LIGHTFLOW_MAX_BLOCKING_RUNS` configures
the permit count, defaults to `4`, and accepts only `1..=64`; invalid values
fall back to the default.

`lfx` is an alias for `lfw run`. It accepts generic JSON inputs, common text /
image / output-path flags, and temporary node toggles:

```bash
lfx lightflow.text_plan --input value=hello
lfx lightflow.text_plan --inputs '{"value":{"topic":"demo"}}'
lfx lightflow.text_to_image --text "a quiet lake" --output ./out.png
lfx lightflow.text_plan --input value=hello --disable prompt --enable prompt
lfx lightflow.text_plan --input value=hello --patch @patch.json
```

The current runner validates the workflow graph, executes nodes in topological
order, and uses passthrough semantics for generic leaf workflows. FLUX image
generation, edit, and inpaint workflows declare synced model requirements and
delegate sampling to LightFlow's native `flux-native` backend when that feature
is enabled. The native text-to-image path keeps a loaded FLUX/Qwen/VAE session
inside the LightFlow process and reuses it for later images with the same model
paths. Multi-image text-to-image runs are sent to the native backend as one
batch request, avoiding repeated model loads and reducing per-image dispatch
overhead during long-lived `lfw serve` sessions. Builds without the native
backend can fall back to the executable named by `LIGHTFLOW_FLUX_RUNNER`;
LightFlow passes the task, prompt, optional source image and mask paths,
sampling settings, output path, and locked model paths to that runner.

Text-generation workflows can declare the `lightflow.llm.generate` runtime
capability. Builds compiled with `--features rig` execute that runtime through
`rig-core`, with the provider, model, prompt, system prompt, API key, base URL,
temperature, max token count, and extra provider parameters supplied as workflow
inputs or environment defaults. The runtime currently supports OpenAI-compatible
chat APIs, OpenAI Responses, Anthropic, Ollama, OpenRouter, DeepSeek, xAI, and
a local `mock` provider for tests.

Use `lfw info` to inspect the current LightFlow build and project architecture:
package version, enabled build features, project workflow search paths,
workflow counts by category, declared runtime capabilities, model requirement
count, and the Executor Registry. `lfw arch` and `lfw architecture` are
aliases. The registry is the single contract for current executors such as
passthrough, preview image generation/edit/inpaint, FLUX, image transforms,
builtin text/JSON/mask/control/model helpers, offline mock LLM generation, and
RIG, plus reserved future capabilities such as command, Python node, ONNX, and
Candle execution.

Runtime executor status is labeled consistently across `lfw info`, node cards,
`/executors`, and documentation. Each executor entry includes a status label,
availability reason, data policy, and whether it plans model resources through
lockfile requirements:

- `preview`: deterministic local image generation/edit/inpaint executors for
  demos, tests, and UI development; these do not prove production model quality.
- `mock`: deterministic local LLM output for offline workflow composition and
  tests.
- `external`: process handoff through an environment variable such as
  `LIGHTFLOW_FLUX_RUNNER`; the external executable owns model sampling.
- `native`: in-process model-backed runtime enabled by build features such as
  `flux-native` or `rig`.
- `reserved`: declared future executor contract that is visible to clients but
  not runnable in the current build.

Use `lfw plan <workflow_id>` or `lfw workflows plan <workflow_id>` to inspect
the selected executor, atoms, data policy, and planned model requirements
without executing the workflow. Composite plans list graph nodes in topological
order and include each deterministic child node's leaf runtime plan.

For the verified runtime commands and native FLUX build prerequisites, see
[Runtime Verification](docs/runtime-verification.md).

Use `lfw help <workflow_id>` when you know a workflow id but not its contract.
It returns the workflow metadata, input and output ports, dependency status,
model requirements, runtime capabilities, graph nodes and edges, plus example
`lfw run` input flags and a JSON input shape. `lfw workflows help <workflow_id>`
is the equivalent namespaced form.

When the HTTP backend is running, `/nodes` returns editor-facing node cards for
all visible workflows: Node Schema v1 ports, runtime executor availability,
model requirements, graph summary, and validation status. `/nodes/{workflow_id}`
returns one node card, `/workflows/{workflow_id}` returns the source workflow
graph, `/executors` returns the shared Executor Registry, and `/models` returns
model requirements with top-level total/available/blocked counts, readiness
issues, port bindings, `lfw.lock` status, hashes, local paths, and per-workflow
sync/verify commands. `/models?workflow_id=<id>&status=blocked` narrows the same
catalog for focused model-lock triage. These endpoints are the backend contract
for a ComfyUI-style node palette without introducing a separate node source format.
`/openapi.yaml` serves the same OpenAPI contract checked into the repository so
editor and tool clients can discover the HTTP surface from a running backend.
The static editor renders `/models` lock status, variants, hashes, local paths,
missing paths, sync/verify commands, and catalog-level readiness counts so
missing model resources are visible before a run. Its model view can filter by
workflow id and lock status.

The same backend records HTTP workflow runs into project-local history and
exposes that history for editor panels: `/runs` lists `.lightflow/runs`,
`/runs/{run_id}` returns manifest, execution, and events,
`/runs/{run_id}/events` returns only append-only events,
`POST /runs/{run_id}/replay` executes the stored manifest stages into a new
recorded run, `DELETE /runs/{run_id}` removes a local run directory, and
`/artifacts` lists artifact handles found in recorded executions. Failed HTTP
workflow-run responses include `run_id`, `run_dir`, and `trace_path`, so
clients can open the failed trace directly instead of guessing from `/runs`.

The MCP adapter exposes the same backend categories for tool clients:
workflow list/get/dependencies/run/validate/save, node list/get, executor list,
model list, run list/get/events/replay/rm, and artifact list with run/stage/node
context. Resource clients can read `lightflow://workflows/<workflow_id>`,
`lightflow://workflows/<workflow_id>/dependencies`,
`lightflow://workflows/<workflow_id>/plan`,
`lightflow://workflows/<workflow_id>/publish`, and
`lightflow://nodes/<workflow_id>` for the common selected workflow and node
views. `lightflow.run.list` accepts the same `limit`, `workflow_id`, and
`status` filters as the CLI and HTTP run catalog.
`lightflow://openapi` exposes the same OpenAPI document as an MCP resource.
`lightflow.workflow.run` records MCP-triggered executions in `.lightflow/runs`
with `surface: "mcp"` events, and includes the failed `run_id`, `run_dir`, and
`trace_path` in JSON-RPC error `data` when execution fails.

The standard node library now includes executable text and JSON helpers for
prompt graphs: `lightflow.text_concat`, `lightflow.text_template`,
`lightflow.text_regex`, and `lightflow.json_extract`; image and mask helpers
`lightflow.image_load`, `lightflow.image_save`, `lightflow.image_resize`,
`lightflow.image_crop`, `lightflow.image_upscale`, and
`lightflow.mask_compose`; preview diffusion nodes `lightflow.image_edit` and
`lightflow.image_inpaint`; control helpers `lightflow.control_if`,
`lightflow.control_switch`, `lightflow.control_merge`, and
`lightflow.control_split`; model helpers `lightflow.model_select` and
`lightflow.model_lock_check`; and LLM helpers `lightflow.llm_generate`,
`lightflow.llm_classify`, and `lightflow.llm_structured_output`, alongside
identity, prompt/result, image generation, and image invert workflows.

Every `lfw run` and `lfx` execution is recorded under
`.lightflow/runs/<run_id>/`:

```text
manifest.json  # run id, timestamps, workflow stages, inputs, toggles
execution.json # workflow or pipeline execution result
events.jsonl   # run and node trace events
```

Use trace and replay while debugging:

```bash
lfw trace last
lfw trace run-1781797000000
lfw runs list
lfw runs get last
lfw artifacts
lfw artifacts --run last --kind image --limit 20
lfw runs replay last
lfw runs rm run-1781797000000
lfw replay
lfw replay run-1781797000000
```

Replay responses include `replay.runtime_changed` and
`replay.model_lock_changed` flags with original/replayed runtime and model-lock
fingerprints, so runtime or model drift is visible when a run is reproduced.
For ComfyUI runs, the runtime fingerprint records the normalized server URL,
resolved submitted graph SHA-256, and stable upload target/content hashes. It
excludes prompt ids, timestamps, output paths, and Authorization values, so a
different remote prompt id does not create false drift while graph, endpoint,
or uploaded-byte changes do.
Nested execution evidence is recursive: `execution.json` retains child
`NodeExecution.nodes`, events carry `depth`, `node_path`, and parent context,
and replay/runtime fingerprints and artifact catalog entries identify the
deepest producing leaf without duplicating the same artifact at every parent.
The static editor renders both runtime and model-lock replay fingerprints,
including stage, workflow, requirement, path, and hash evidence.

Replay uses the stored stage definitions and writes a new run directory, so the
original history remains immutable.
`lfw runs list` returns compact run summaries for editor/history browsers,
including every staged workflow id for pipeline runs, duration, and the
originating surface when recorded. It also includes top-level total, completed,
failed, and unknown-status counts plus `unknown_run_ids` for compact dashboards.
Use `lfw runs list --limit <n>`, `--workflow <workflow_id>`, and
`--status <status>` or the matching `/runs?limit=...&workflow_id=...&status=...`
query parameters to inspect a focused slice of large local histories. MCP
clients use the same arguments on `lightflow.run.list`, or read
`lightflow://runs?workflow_id=<id>&status=<status>&limit=<n>` when a resource
URI is more convenient. Use `lightflow://runs/<run_id>` for the full recorded
run and `lightflow://runs/<run_id>/events` for the event timeline; `last` is
accepted as a run id in the same places as the CLI.
For legacy run manifests that predate explicit status fields, the catalog
infers completed or failed status from terminal trace events or execution
status before falling back to `unknown`. It reports non-fatal local history
issues without hiding valid runs. `lfw runs get` returns the same full
manifest, execution, and event data as `lfw trace`.
The static editor shows those compact summary fields in the run browser so CLI,
HTTP, and MCP runs can be compared by surface, duration, stage count, and
workflow list before opening the full trace.

Composite workflow executions also record node-level trace data. Each node in
`execution.json` includes its status, input/output snapshots, artifact handles,
duration in milliseconds, attempt count, and selected runtime metadata when a
leaf executor is chosen. `events.jsonl` expands that into append-only
`node_completed` or `node_skipped` events between `run_started` and
`run_finished`; completed node events carry the selected runtime metadata for
timeline views.
Artifact catalog entries include run, stage, node, and workflow context so
pipeline outputs can be mapped back to the trace that produced them. The static
editor uses that context to let artifact rows jump directly to the producing
run trace, and `lfw artifacts` exposes the same catalog to CLI users. Use
`--run`, `--workflow`, `--kind`, and `--limit` to keep large local histories
inspectable from scripts and terminals. HTTP `/artifacts` accepts the matching
`run_id`, `workflow_id`, `kind`, and `limit` query parameters, and MCP
`lightflow.artifact.list` accepts the same arguments. MCP resource clients can
use `lightflow://artifacts?run_id=<run>&workflow_id=<id>&kind=<kind>&limit=<n>`
for the same filtered artifact catalog.

Failed `lfw run` and `lfx` executions are recorded too. The CLI still exits
non-zero, but stderr includes the `run_id` and `trace_path`; `lfw trace last`
then shows `manifest.status = "failed"`, the error message in `execution.json`,
and a `run_failed` event.

Graph workflow patches can be passed inline or from a file with `--patch`.
They apply to node boundaries for that run and are stored in the run manifest:

```json
{
  "nodes": {
    "search": {
      "replace_with": "lightflow.mock_search",
      "retry": 3,
      "timeout_ms": 5000
    },
    "payment": {
      "disable": true,
      "fallback_workflow_id": "lightflow.payment_skipped"
    }
  }
}
```

`replace_with` and `fallback_workflow_id` point at other discovered workflow
ids. Typed Rust function replacement uses the SDK `HookRegistry`; the CLI
patch format stays serializable so traces, replay, and editor tooling can store
it without compiling user code. Run patches are validated against the selected
workflow before execution, so unknown node ids, patches meant for a different
workflow, missing replacement/fallback workflows, and invalid retry counts fail
before a run record is written. Replacement and fallback workflows must also
preserve the patched node's input and output port names and types, so a patch
cannot silently drop downstream data. Extra replacement inputs are allowed only
when they are optional or have defaults, because the existing graph cannot
supply new required inputs.

Reusable graph patches can be saved in the project patch registry:

```bash
lfw patch save qa-debug @patch.json
lfw patch list
lfw patch get qa-debug
lfw patch validate qa-debug
lfw patch validate qa-debug --workflow lightflow.text_plan
lfw run lightflow.text_plan --input value=hello --patch qa-debug
lfw patch rm qa-debug
```

The registry lives under `.lightflow/patches/<name>.json`. `lfw run --patch
<name>` expands the registered patch before execution, and the expanded patch
is stored in the run manifest. Replay therefore uses the original run's patch
even if the registry entry changes later.
`lfw patch validate` reports `valid` plus `issues`; it checks that patch node
ids match available workflow nodes, replacement and fallback workflow ids are
discoverable, and retry counts are usable. Add `--workflow <workflow_id>` to
preflight the patch against one selected workflow's node ids and
replacement/fallback port contracts before running it. `lfw loop check` fails
when saved patches are invalid so temporary graph edits remain reviewable
before handoff.
The same registry is exposed through HTTP as `/patches`, `/patches/{name}`,
and `/patches/validate`, with optional `?workflow_id=...` selected validation,
and through MCP as `lightflow.patch.*` tools plus
`lightflow://patches/<name>` for read-only patch lookup. MCP
`lightflow.patch.validate` accepts the same optional `workflow_id` argument.
Editor clients load registered patches from the backend and submit expanded
patch JSON in the normal workflow run body. The static editor validates patch
JSON against the selected workflow by default, so node id mistakes and
replacement/fallback contract mismatches are visible before running.

## Importing Workflows

For a start-to-finish authoring guide, see
[Workflow Development Guide](docs/workflow-development.md). This section
summarizes the installation and discovery model.

LightFlow stores user shell configuration in:

```text
$XDG_CONFIG_HOME/lightflow/.lfwrc
# default: ~/.config/lightflow/.lfwrc
```

`lfw init` creates the file when missing and appends a source line to the
detected shell startup file (`.bashrc`, `.zshrc`, or fish `config.fish`):

```bash
source ~/.config/lightflow/.lfwrc
export LFW_PATH='/home/alice/.lightflow'
```

Project workflows are discovered automatically from the current working
directory's `./.lightflow/workflows/` tree. Legacy `./workflows/` collections
are still read for compatibility. `LFW_PATH` is reserved for global or shared
workflow homes. The default global home is `~/.lightflow`, so global workflows
live under `~/.lightflow/workflows`. Run `lfw home` to print the active home,
manifest, workflow source directory, and repo cache. `LFW_PATH` uses the platform
path-list format, so multiple global homes or legacy workflow collections can
be searched. `lfw` itself reads the environment variable provided by the shell;
it does not parse `.lfwrc` as a runtime config file. The default global home is
initialized as a Cargo workspace with `members = ["workflows/*/*"]`, so
globally imported workflow crates share one dependency environment.

Global installation is Cargo-backed. The default home is not a custom package
database; it is a normal Cargo workspace whose `Cargo.toml` records global
workflow crates as workspace members or dependencies. `lfw add --global` and
`lfw import --global` edit that manifest, while `lfw update --global` and
`lfw upgrade --global` delegate to Cargo.

`lfw init --workflow` creates a project workflow collection under
`./.lightflow/workflows`. Its root `Cargo.toml` is both the workflow workspace
and a non-publishable host package whose library is `.lightflow/workspace.rs`;
normal Cargo dependencies added to that host are therefore visible to
LightFlow. `lfw init --plugin` creates a single standard Cargo
crate that can expose a workflow from `src/lib.rs`. `lfw new --global` creates
a workflow crate under the default global home's `workflows/` tree; `lfw add --global`
writes dependencies to the default global home's `Cargo.toml`. Those global
path dependencies are discovered from the global home manifest, so a workflow
installed with `lfw add --global --path ...` can be used from any project that
uses the same global home or `LFW_PATH`.

Use `lfw add` when the target is one known Cargo package. Use `lfw import`
when the target is a workflow repository or collection and LightFlow should
scan `workflows/<category>/<crate>` for multiple workflow crates.

Workflow dependencies are Cargo dependencies. A local standard workflow can be
installed with:

```toml
[dependencies]
lightflow-text-prompt = { path = "projects/lightflow-std/workflows/std/text_prompt" }
```

Registry and Git workflow crates use Cargo directly; no LightFlow package
format or registry resolver is involved:

```bash
cargo add lightflow-text-prompt
cargo add --path ../lightflow-text-prompt
cargo add --git https://github.com/example/my-workflow my-workflow
```

LightFlow asks `cargo metadata` for the resolved package graph and discovers
direct dependency crates whose library target exposes `pub fn define() ->
WorkflowSpec`. If that workflow depends on other workflow crates, those are
discovered recursively. Cargo owns registry downloads, Git checkouts, feature
resolution, and `Cargo.lock`.

A directly executable workflow remains the same package: keep the reusable
`define()` function in `src/lib.rs`, then add a `src/bin/<name>.rs` target that
passes it to `lightflow::runner::run_workflow_from_env`. Install that binary
with Cargo:

```bash
cargo install my-workflow --bin my-workflow
```

Publish a crate with standard `cargo publish`; `lfw publish <workflow_id>` is
the optional LightFlow readiness/dry-run gate before Cargo publishing.

Use `--editable` with `--path` for a development install. It records the same
Cargo path dependency, keeps the source tree live for edits, and marks the CLI
result as editable:

```bash
lfw add lightflow-text-prompt --path projects/lightflow-std/workflows/std/text_prompt --editable
```

Use an external checkout path such as `../lightflow-std/workflows/std/text_prompt`
only when the standard workflow repository is not checked out under
`projects/`.

Refresh and upgrade workflow dependency resolution through Cargo:

```bash
lfw update          # cargo fetch in the current workspace
lfw upgrade         # cargo update in the current workspace
lfw update --global # run against the default global workflow workspace
lfw upgrade --global
```

`lfw` does not reimplement Cargo dependency solving; these commands delegate to
Cargo and let `Cargo.lock` record the resolved workflow crate versions.

Any dependency crate that exposes `pub fn define() -> WorkflowSpec` in
`src/lib.rs` is discovered by the backend and can be referenced from
`.depends_on(...)` or `.node(...)`.

Workflow repositories with multiple workflow crates can be imported in one
step:

```bash
lfw import --global /path/to/lightflow-flux
lfw import --global https://github.com/lightjunction/lightflow-flux.git
```

The repository remains a self-contained Cargo workspace. `lfw import`
discovers `workflows/<category>/<crate>` and records each workflow crate as a
path dependency in the target project or global workspace.

For git URLs, `lfw import` clones the repository into the LightFlow repo cache
under the selected home, then records path dependencies to the workflow crates
inside that clone. This keeps Cargo as the dependency resolver while allowing a
single import command to install a multi-crate workflow collection.

Workflow crates may also define standard Rust binary targets for direct
execution:

```rust
fn main() -> lightflow::runner::RunnerResult<()> {
    lightflow::runner::run_workflow_from_env(my_workflow_crate::define())
}
```

That keeps `lfw` useful for installing, discovering, syncing, and composing
workflows, while a workflow package can still ship normal executable commands.

Typed Rust workflows use `Workflow<I, O>` as a composable function boundary.
Internal nodes may use private context, but cross-workflow composition is
checked by Rust input/output types:

```rust
use lightflow::preload::*;

let result = classify_flow
    .then(search_flow)
    .then(answer_flow)
    .run(input)
    .await?;
```

The typed SDK also exposes hook and patch primitives at node boundaries:

```rust
let hooks = HookRegistry::new()
    .hook("search", LogHook)
    .replace("search", |query| async move { mock_search(query).await })
    .retry("search", 3)
    .timeout_ms("search", 5_000);

let result = run_node("search", query, |query| async move {
    search(query).await
}, &hooks).await?;
```

Use `run_node_borrowed` for large inputs when cloning would be wasteful; this
keeps model paths and artifacts on the zero-copy path.

The `#[node]` macro uses the same boundary for single-input typed nodes. It
keeps the original function name and also generates `<node>_with_hooks`:

```rust
#[node("classify")]
async fn classify(input: UserInput) -> lightflow::anyhow::Result<Intent> {
    Ok(Intent::from(input))
}

let patched = HookRegistry::new().replace("classify", |input| async move {
    Ok(mock_intent(input))
});
let intent = classify_with_hooks(input, &patched).await?;
```

Workflow files can also embed the Cargo installation hint:

```rust
workflow!()
    .depends_on_path(
        "lightflow.text_prompt",
        "0.1.0",
        "lightflow-text-prompt",
        "projects/lightflow-std/workflows/std/text_prompt",
    )
    .depends_on_git(
        "lightflow.text_prompt",
        "0.1.0",
        "lightflow-text-prompt",
        "https://github.com/lightjunction/lightflow-std",
        "lightflow-text-prompt",
    )
```

`lfw sync --apply` uses those hints to add missing Cargo dependencies before
running `cargo fetch`.

## Versioning

Workflow versions are SemVer strings. The current resolver supports exact
requirements and `*`:

```rust
workflow!()
    .depends_on("lightflow.other", "0.1.0")
```

Range requirements such as `^0.1` and `>=0.1` are planned after the exact
version update path is stable.

## Publishing

`lfw publish` creates a Cargo publish plan by default. It checks the target
manifest for basic crates.io blockers such as `publish = false`, non-SemVer
versions, git dependencies, and path dependencies without a version. Workflow
crate publish checks also parse `src/lib.rs` and block unresolved generated
`TODO` placeholders in workflow, input, or output descriptions.
Dependency checks cover normal, build, dev, and target-specific dependency
sections, including `workspace = true` entries inherited from
`[workspace.dependencies]`.

```bash
lfw publish                  # root lightflow crate plan
lfw publish lightflow.text_prompt    # workflow crate plan
lfw publish --crate path/to/crate
lfw publish --workflows      # all workflow crate plans in dependency order
lfw publish --workflows --require-publishable
lfw publish --workflows --project lightflow-std
lfw publish lightflow.text_prompt --apply
lfw publish --workflows --apply --allow-dirty
```

`--apply` first runs `cargo publish --manifest-path ... --dry-run`; only after
that succeeds does it run the real `cargo publish --manifest-path ...`.
Without `--apply`, no network publish is attempted and the generated command
includes `--dry-run`.
`--require-publishable` keeps the command non-networked but exits non-zero when
the selected publish plan has blockers, which is useful for CI and release
gates.
`--workflows` scans workflow crates under `workflows/*/*` plus present linked
workflow project workspaces under `projects/`, orders local path dependencies
before dependents, and refuses to upload anything unless every workflow crate
passes the static publish checks. Duplicate workflow ids are deduped in favor
of root workspace definitions in the default catalog. The dependency order
accounts for direct path dependencies and inherited workspace path dependencies.
The dry-run plan includes top-level total/publishable/blocked counts plus
per-crate workflow ids, workspace labels, and blockers.
With `--apply`, workflow targets first run the `lfw loop changes` review gate,
then run the dry-run publish commands, then run the real publish commands.
`--allow-dirty` forwards Cargo's explicit dirty-worktree override to both
preflight and upload commands.
HTTP and MCP clients can inspect non-mutating publish preflights through
`GET /publish`, `lightflow.workflow.publish_list`, and `lightflow://publish`
for every local workflow crate in dependency order, including package name,
version, workspace label, internal workflow path dependencies, per-crate
dry-run commands, a top-level dependency-ordered command list, blockers, and
top-level total/publishable/blocked counts; or through
`GET /workflows/{workflow_id}/publish` and
`lightflow.workflow.publish_check` for one selected workflow. MCP resource
clients can read the same selected workflow preflight from
`lightflow://workflows/<workflow_id>/publish`.
Use `GET /publish?project=<name>`, MCP `lightflow.workflow.publish_list` with
`project`, or `lightflow://publish?project=<name>` to inspect one linked
project workspace by full name, `projects/<name>` label, path, or short
`lightflow-*` alias. Project-scoped publish views return that linked
workspace's matching workflow crates even when the default catalog dedupes the
same workflow id in favor of the root workspace. The HTTP and MCP inspection
surfaces use the same linked-workspace, project-filter, and workflow-id dedupe
rules as the CLI. MCP clients can discover the parameterized resource through
`resources/templates/list` as `lightflow://publish?project={project}`.

## Release Checks

`lfw loop check` reports whether the current checkout has the local workflow
loop prerequisites: the loop document, discoverable workflow crates, colocated
agent skills with CLI/API run examples, the optional `projects/`
sibling-workspace view, executor
catalog, model-lock readiness, source-change safety, run-history and
saved-patch readiness, and workflow crates for `lfw publish --workflows`. With
a workflow id, it also checks validation, dependencies, execution planning,
planned executor availability, model-lock readiness, recorded runs, replay
readiness, and publish dry-run readiness for local crates in that workflow's
dependency graph. Pipeline runs count as recorded history for every staged
workflow.
At project scope it also summarizes workspace publish readiness and points to
`/publish` or `lfw publish --workflows` when local workflow crates still have
publish blockers. Non-fatal run-history catalog issues appear as readiness
warnings so valid runs remain inspectable while damaged local records are still
visible. Unknown-status run summaries also warn at readiness level and expose
their run ids in `/runs` so legacy or partially migrated history remains
visible without blocking the loop. Missing, invalid, or incomplete model locks
warn at readiness level using the same `/models` catalog issues, so `lfw sync`
gaps are visible before a model-backed run. `lfw models requirements` exposes
the same non-network model-requirement catalog for CLI users, including lock
status plus sync and verify commands, and `lfw models requirements
<workflow_id>` narrows the catalog to one workflow before syncing.
`--blocked`, `--available`, or `--status all|available|blocked` filters that
catalog by lock readiness. The
suggested local-loop command chain includes `lfw models requirements
<workflow_id> --blocked` followed by `lfw sync <workflow_id> --auto-model
--apply`, and selected workflow model warnings also mention `--locked --apply`
for verifying an existing lockfile/cache.

```bash
lfw loop check
lfw loop check lightflow.text_plan
lfw loop changes
lfw loop projects
lfw loop projects --dirty
lfw loop projects --project lightflow-std
```

MCP clients can request the same readiness report through
`lightflow.loop.check`, or read the project-level report from
`lightflow://loop`. Use `lightflow://loop?workflow_id=<id>` or
`lightflow://loop?workflow_id=<id>&require_replay=true` for a selected
workflow and replay-required gate. HTTP and editor clients use `GET /loop` for
project readiness and `GET /workflows/{workflow_id}/loop` for a selected workflow.
Loop readiness reports include top-level passed, warning, and failed counts so
clients can summarize readiness without re-counting every check. Failed checks
are also summarized in `issues`, while non-blocking warnings are summarized in
`warning_messages`. Selected workflow reports include `replay_run_id` when a
completed run is available as concrete trace/replay evidence.
`lfw loop check` summarizes source-change safety, while `lfw loop changes`
returns the detailed local review gate for agent edits: if a workflow crate file
changed, the colocated `.agent/skills/.../SKILL.md` must change in the same
worktree diff. Complete workflow crate removals are allowed without a separate
skill edit because the skill is removed with the crate. Skill-only documentation
edits remain visible as passed review rows. Saved patch registry edits under
`.lightflow/patches/*.json` are surfaced as warning-level patch changes in both
commands, so temporary runtime behavior edits remain visible without blocking
valid loop readiness.
The same source-change safety report is available through `GET /loop/changes`,
`lightflow.loop.changes`, and `lightflow://loop/changes`, with top-level
passed, warning, and failed counts for quick inspection. Inspection problems
are reported as `issues`; unsafe changed workflows are reported separately as
`blockers`.
The sibling workspace catalog is available through `lfw loop projects`,
`GET /loop/projects`, `lightflow.loop.projects`, and
`lightflow://loop/projects`. It reports the expected project workspaces from
`projects/lightflow-projects.toml`, falling back to `lightflow-std`,
`lightflow-flux`, and `lightflow-rig` when that file is absent, plus any extra
linked project directories and workflow-crate counts. This lets agents inspect
the local multi-repo workspace before editing linked workflows. When a linked
workspace is a git repository, the catalog also reports whether it is dirty,
how many paths changed, changed paths relative to that workspace, current
branch, upstream branch, origin remote URL, and short HEAD commit, so submodule
commits can be prepared before updating the parent gitlink. For submodule
workspaces, it also reports the parent gitlink commit and whether that gitlink
differs from the child checkout HEAD. It includes
`git_status_command`, `git_stage_command`, `git_commit_command`,
`git_push_command`, and `parent_gitlink_stage_command` so tools can present the
next child-repo and parent-gitlink commands directly. The catalog includes
`known_workspace_names` so
clients can show available project filters even when the returned `workspaces`
list is filtered. It also exposes `known_project_workspaces` and
`known_project_aliases` as compatibility aliases matching the release/dev
report naming. Each workspace row includes its own `aliases` list for row-level
labels and selector hints. `lfw loop check` and release review surface dirty or
uninspectable project workspace git state as warnings before the parent gitlink
is updated. Developer and release review details include the same inspect and
parent-gitlink stage commands so agents and editor clients can turn a warning
into the next concrete command without querying the catalog again. The catalog
also returns `project_config_path`, `project_config_present`, and
`project_config_valid`, `project_config_error`, and
`default_workflow_sources`, so clients can show where the project-set config
lives, whether it parsed, and which project workflow crates are loaded without
parsing `projects/lightflow-projects.toml`. `known_optional_workspace_names`
identifies configured optional project workspaces even when the response is
filtered, while `optional_workspace_names` names optional workspaces in the
returned rows. Optional workspaces are recognized when present but do not fail
the catalog when absent. `directory_count`, `symlink_count`, and
`submodule_count` expose the concrete local checkout shape; `not_symlink_count`
is retained only as a deprecated compatibility alias for `directory_count`.
It also includes `project_config_template_command`,
`project_config_write_command`, and `project_submodule_update_command` for
rendering one-click config repair and configured submodule initialization
actions.
Use `lfw loop projects --dirty`, `GET /loop/projects?dirty=true`,
`lightflow.loop.projects` with `dirty: true`, or
`lightflow://loop/projects?dirty=true` to show only workspaces with changed
paths, stale parent gitlinks, or uninspectable git status. The response sets
`dirty_filter` so clients can tell whether counts and workspace rows are from
the full catalog or a review-only slice.
Use `--project <name>`, `GET /loop/projects?project=<name>`, MCP
`project: "<name>"`, or `lightflow://loop/projects?project=<name>` to inspect
one workspace by name, label, path, or the conventional `lightflow-*` short
alias, such as `std` for `lightflow-std` or `auto-editing` for
`lightflow-auto-editing`. Path filters may be labels like
`projects/lightflow-std`, relative forms like `./projects/lightflow-std`, or
absolute checkout paths. MCP resource URI query values may be percent-encoded,
for example `lightflow://loop/projects?project=%2Ftmp%2Flightflow-std`.
Unknown project filters are reported as catalog issues instead of returning a
silent empty result, and include the known workspace names and aliases. MCP
clients can discover the parameterized project catalog resources through
`resources/templates/list` as
`lightflow://loop?workflow_id={workflow_id}`,
`lightflow://loop?workflow_id={workflow_id}&require_replay={require_replay}`,
`lightflow://loop/projects?project={project}` and
`lightflow://loop/projects?project={project}&dirty={dirty}`.
`projects/lightflow-projects.toml` also declares
`[workflows].default_sources`, the project workspaces whose workflow crates are
loaded by the core repository without extra search paths. This repository keeps
`lightflow-std` there because it supplies the baseline standard node set.
Project config entries are directory names under `projects/`, not filesystem
paths; use `lightflow-std`, not `projects/lightflow-std` or
`../lightflow-std`.
Domain-specific sibling projects such as `lightflow-flux` and `lightflow-rig`
stay opt-in through `LFW_PATH`, `lfw import`, explicit workflow search paths,
or an explicit addition to `default_sources`, so local startup does not
silently depend on every checked-out project. Related repositories that should
be visible to tooling but not required by every checkout belong in
`[workspaces].optional`. A workspace listed in both optional and required
sources is treated as required. Any project listed in
`default_sources` is treated as required by `lfw loop projects`, so a missing
default source is reported as workspace drift instead of silently removing
nodes from the active catalog.

`lfw dev check` reports the developer gate plan to run before handing off code
changes. It reuses the same backend gate definitions as release checks, so
formatting, local loop readiness, source-change safety, sibling workspace
health, publish readiness, clippy, tests, workflow skill coverage, and
feature-specific runtime checks stay in one extensible contract.
The development profile skips release-only artifact and document-section gates;
`lfw release check` keeps those gates for publish readiness.
Pass `--project <name>` to `lfw dev check` when iterating on one linked
workspace. `<name>` accepts the same full names, labels, relative or absolute
paths, and short aliases as `lfw loop projects`. The report records the
project filter, narrows the project workspace review to that workspace, and
plans `lfw loop projects --project <name>` plus `lfw loop projects --dirty
--project <name>`, while the selected workflow gate remains controlled by
`--workflow`. Unknown project filters make the gate invalid and list the known
workspace names and aliases, so typos do not look like clean project state.
Release/dev reports also include
`known_project_workspaces` and `known_project_aliases` as structured data for
editor pickers and agent repair prompts, plus `project_filter_matched` when a
project filter was supplied so clients can distinguish a typo from a matched
but unhealthy workspace. When a filter matches, `matched_project_workspace`
contains the canonical workspace name, such as `lightflow-rig` for `rig`.
The same reports expose `project_config_path`, `project_config_present`,
`project_config_valid`, `project_config_error`, `default_workflow_sources`,
`project_config_template_command`, `project_config_write_command`, and
`project_submodule_update_command`, plus `known_optional_workspace_names`, so
developer clients can point directly at the project-set config or offer a
repair action without first calling `lfw loop projects`.
When `dev check` reports an incomplete workflow skill, use
`lfw dev skill-template <workflow_id>` to generate a compliant `SKILL.md`
starter with the required CLI and HTTP run examples. Add `--write` to place it
under the workflow crate, and add `--force` only when replacing an existing
skill is intentional.
When a project set is missing `projects/lightflow-projects.toml`, use
`lfw dev project-config-template` to inspect the effective default config, or
`lfw dev project-config-template --write` to create it. Existing config files
are not overwritten unless `--force` is passed. If an existing project config
is invalid, the same command still returns a repair template plus
`project_config_error`, `project_config_template_command`,
`project_config_write_command`, and `project_submodule_update_command`, and
`--write --force` can replace the bad file.

`lfw release check` reports the release gates needed for a meaningful
LightFlow release without running expensive commands by default. It verifies
the required release artifacts exist, checks that `CHANGELOG.md` includes the
expected CLI, API, workflow, runtime, known limitation, and migration sections,
checks source-change safety and sibling project workspace health directly, and
lists the exact commands for format, clippy, default tests, workflow agent skill
coverage, project and selected-workflow loop readiness, sibling project
workspace inspection, strict workflow publish readiness, and feature-specific
runtime checks. It reviews project loop warnings that are not already covered by
dedicated source-change, sibling-workspace, or publish-readiness reviews,
reviews the selected workflow loop with completed-run replay evidence required,
reviews the publish-readiness catalog directly, and plans project loop
readiness, the selected workflow loop command, `lfw loop changes`,
`lfw loop projects`, and strict workflow publish readiness before the expensive
clippy/test gates so unsafe workflow edits, missing sibling links, selected
workflow gaps, or publish blockers fail early. The
selected workflow defaults to `lightflow.text_plan`; pass `lfw release check
--workflow <workflow_id>` for another project, and pass `--project <name>` to
focus sibling workspace review and planned project catalog commands on one
linked repository. Unknown project filters fail the review instead of returning
an empty clean report. Release reviews use the same workflow search paths as
normal CLI, HTTP, and MCP operations, including configured `LFW_PATH` entries
and linked `projects/` workspaces.

```bash
scripts/check.sh
scripts/check.sh --list --full --project lightflow-std
scripts/check.sh --full
scripts/check.sh --full --project lightflow-std
scripts/check.sh --full --workflow lightflow.text_plan
scripts/check.sh --full --project lightflow-std --workflow lightflow.text_plan
scripts/check-source-shape.sh
cargo test --test standard_workflow_skills repository_workflow_crates_have_agent_skills
cargo test publish_endpoint_can_filter_project_workspaces
cargo test mcp_exposes_backend_tools
lfw dev check
lfw dev check --project lightflow-std
lfw loop projects
lfw loop projects --dirty
lfw loop projects --project lightflow-std
lfw loop projects --dirty --project lightflow-std
lfw dev check --apply
lfw release check
lfw release check --project lightflow-std
lfw release check --apply
```

`--apply` executes the planned commands and returns a structured pass/fail
report. The command fails only in apply mode when a required artifact or
document section is missing, or when a gate command exits unsuccessfully. After
the first failed release gate in apply mode, later command gates are marked
skipped instead of continuing through expensive checks.
HTTP and MCP clients can inspect the same non-mutating release gate plan through
`GET /release`, `lightflow.release.check`, and `lightflow://release`. The HTTP
and MCP surfaces intentionally expose the dry-run report only; command
execution remains a CLI `--apply` operation. Use
`GET /release?workflow_id=<id>&project=<name>`, MCP `lightflow.release.check`
arguments, or `lightflow://release?workflow_id=<id>&project=<name>` to inspect
one selected workflow and linked project workspace. MCP clients can discover
the parameterized release resources through `resources/templates/list`.
Release reports include the
`project_root` and selected `workflow_id`, so editor and agent clients do not
have to infer release context from command strings. Failed gates are summarized
in a top-level `issues` array, warning gates are summarized in top-level
`warnings`, and top-level `passed`, `warning_count`, `failed`, `planned`, and
`skipped` counts let clients summarize release readiness without re-counting
the full check list. Individual release checks may also include a `count` for
the number of changed workflows, linked workspaces, workflow crates, or planned
commands represented by that row. The full check list still preserves planned,
passed, warning, failed, and skipped gate details. Warning checks remain
non-blocking but visible in the report.

## Sync

`lfw sync` prepares module and model dependencies. It always treats model
requirements as choices, not as mandatory downloads of every possible file.

```bash
lfw sync lightflow.image_prompt --dry-run
lfw sync lightflow.image_prompt --model image_model=flux2-gguf --apply
lfw sync lightflow.image_prompt --hf-model image_model=gguf:owner/repo:model.gguf --apply
lfw sync lightflow.image_prompt --auto-model --apply
```

The module side uses Cargo:

```bash
cargo fetch
```

If a workflow dependency embeds install metadata with `.depends_on_crate(...)`,
`.depends_on_path(...)`, or `.depends_on_git(...)`, `lfw sync` reports missing
Cargo dependencies under `module_dependencies.installs`; `--apply` writes them
to the workspace manifest.

The model side uses Hugging Face's CLI and global cache:

```bash
lfw models requirements
lfw models requirements lightflow.text_to_image
lfw models requirements --blocked
lfw models requirements lightflow.text_to_image --status blocked
hf download city96/FLUX.2-dev-gguf flux2-dev-q4.gguf
```

This lets workflows declare a model capability, such as text-to-image, and
offer safetensors / GGUF variants without forcing every user to sync the same
model artifact. `--hf-model` is an explicit escape hatch for custom HF files;
it still binds to a declared model requirement but marks the download as
`custom: true` instead of treating it as a recommended workflow variant.
`--auto-model` detects available RAM and NVIDIA VRAM, chooses a compatible
declared variant for every unresolved model requirement, and downloads it when
combined with `--apply`.

## Batch Runs

`lfw batch run` executes many workflow jobs from a JSONL queue while limiting
the number of active workflow executions:

```bash
lfw batch run jobs.jsonl \
  --workflow lightflow.image_inpaint \
  --max-gpu-jobs 1 \
  --max-cpu-jobs auto \
  --batch-size auto \
  --reserve-mem 6GB \
  --reserve-vram 1GB
```

Each JSONL line is one job:

```json
{"id":"frame-001","inputs":{"image_path":"input/001.png","mask_path":"masks/001.png","prompt":"repair the scratch"}}
```

Batch state is written under `.lightflow/runs/<run_id>/` as `manifest.json`,
`input.jsonl`, `jobs.jsonl`, and `events.jsonl`. If a run is interrupted, resume
only pending or retryable failed jobs:

```bash
lfw batch resume <run_id> --max-gpu-jobs 1
```

The built-in scheduler currently enforces workflow execution concurrency and
records CPU, batch-size, memory, and VRAM policy in the run manifest. Runtime
adapters can use the same policy to add model-resident workers, micro-batching,
and backend-specific memory probes.

## HTTP

```bash
curl http://127.0.0.1:5174/openapi.yaml
curl http://127.0.0.1:5174/workflows
curl http://127.0.0.1:5174/workflows/lightflow.text_plan
curl http://127.0.0.1:5174/workflows/lightflow.text_plan/dependencies
curl http://127.0.0.1:5174/workflows/lightflow.text_plan/plan
curl http://127.0.0.1:5174/publish
curl http://127.0.0.1:5174/workflows/lightflow.text_plan/publish
curl -X POST http://127.0.0.1:5174/workflows/lightflow.text_plan/run \
  -H 'content-type: application/json' \
  -d '{"inputs":{"value":"hello"},"disabled_nodes":["prompt"]}'
curl -X DELETE http://127.0.0.1:5174/runs/last
```
