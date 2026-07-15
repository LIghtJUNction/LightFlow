# Data Layout

LightFlow project workflow files are ordinary source-controlled files under
`.lightflow/workflows/`.
`lfw init --workflow` creates this layout:

```text
.lightflow/
  workflows/
    <crate>/
      Cargo.toml
      src/
        lib.rs
```

Legacy two-level collections are not discovered implicitly. Run `lfw migrate`
at their repository root to move `<category>/<crate>` entries to `<crate>` and
update known Cargo workspace member globs after a full conflict preflight.

The core repository Cargo workspace contains only the backend crate and core
support crates that ship with it:

```toml
members = [".", "lightflow-macros"]
```

`lightflow-macros` is part of the core SDK surface because it provides
procedural macros used by typed workflow APIs. It is not a workflow project and
does not belong under `projects/`. The `projects/` directory is reserved for
independent workflow/plugin repositories such as `lightflow-std`,
`lightflow-flux`, and `lightflow-rig`.

Shared user workflow configuration still follows XDG config paths. The shell
sources:

```text
$XDG_CONFIG_HOME/lightflow/.lfwrc
# default: ~/.config/lightflow/.lfwrc
```

For bash and zsh, the rc file uses shell-style export syntax:

```bash
export LFW_PATH="$HOME/.lightflow"
```

For fish, `lfw init` writes fish syntax instead:

```fish
set -gx LFW_PATH "$HOME/.lightflow"
```

`lfw init` detects `SHELL` and appends `source <rc>` to `.bashrc`, `.zshrc`,
or `$XDG_CONFIG_HOME/fish/config.fish`. Project workflows are discovered from
the current working directory's `./.lightflow/workflows/` tree and never need
`LFW_PATH`. `LFW_PATH` is only for global or shared workflow collections. If
there is no exported `LFW_PATH`, `lfw` uses `~/.lightflow`. `lfw` does not
parse `.lfwrc` directly at runtime; it reads the environment provided by the
shell.

Generated media outputs use the core `MediaPathProvider` so workflows do not
hand-roll platform paths. Image outputs default to the user's XDG Pictures
directory, video outputs to XDG Videos, and music/audio outputs to XDG Music,
each under a `lightflow` subdirectory. On Linux, LightFlow resolves these from
`$XDG_PICTURES_DIR`, `$XDG_VIDEOS_DIR`, or `$XDG_MUSIC_DIR` when exported, then
`$XDG_CONFIG_HOME/user-dirs.dirs`, then falls back to
`$HOME/Pictures/lightflow`, `$HOME/Videos/lightflow`, or
`$HOME/Music/lightflow`. Explicit `output_path` inputs always win.

Single workflow and pipeline runs are recorded in the current project under:

```text
.lightflow/
  patches/
    <name>.json
  runs/
    last
    <run_id>/
      manifest.json
      execution.json
      events.jsonl
```

`manifest.json` stores the run id, timestamps, workflow stages, inputs, and
temporary node toggles. It also records `status` as `completed` or `failed`.
Pipeline manifests record resolved stage inputs after upstream outputs and
explicit overrides are merged, so replay can execute the same effective stage
definitions. Older manifests without `stage_input_resolution: "resolved"` are
replayed with the legacy pipeline propagation rule.
`execution.json` stores the actual workflow or pipeline result. Composite node
records include status, inputs, outputs, artifact handles, `duration_ms`, and
`attempts`, plus selected runtime metadata for executed leaf nodes. Leaf
workflow executions also record the selected executor when a runtime is chosen.
Failed runs store `status: "failed"` and an error object in `execution.json`.
When a CLI pipeline fails after one or more stages completed, `execution.json`
also includes `partial_execution` with the completed stage executions, outputs,
and artifacts that were available before the failing stage, and `events.jsonl`
includes the completed stage node events before `run_failed`.
`events.jsonl` is append-only trace data: it starts with
`run_started`, includes one `node_completed` or `node_skipped` event for each
successful graph node, adds `stage_completed` events for pipeline stages,
carries selected runtime metadata on completed node and stage events, and ends
with `run_finished` or `run_failed`. CLI commands, HTTP
`POST /workflows/{workflow_id}/run`, and MCP run tools write the same run
history shape, with `surface` labels in run-level events, so editor clients can
inspect runs without private backend state. Failed HTTP workflow-run responses
include `run_id`, `run_dir`, and `trace_path`; failed MCP workflow-run errors
include the same fields in JSON-RPC error `data`. `lfw trace last` reads this
directory without executing anything, and `lfw replay` replays `last` by
default. `lfw replay <run_id>` uses one explicit run's stored stage definitions
to create a new run.

Run history can also be managed directly:

```bash
lfw runs list
lfw runs list --limit 20 --workflow lightflow.text_plan --status completed
lfw runs get last
lfw runs get run-1781797000000
lfw artifacts
lfw artifacts --run last --workflow lightflow.text_plan --kind image --limit 20
lfw runs replay last
lfw runs rm run-1781797000000
lfw replay
curl 'http://127.0.0.1:5174/runs?limit=20&workflow_id=lightflow.text_plan&status=completed'
curl -X POST http://127.0.0.1:5174/runs/last/replay
curl -X DELETE http://127.0.0.1:5174/runs/last
```

`lfw runs list` returns compact manifest summaries sorted by newest completion
time first. `--limit`, `--workflow`, and `--status` narrow the returned
summary catalog for large local histories; the HTTP `/runs` endpoint accepts
the same `limit`, `workflow_id`, and `status` query parameters, and MCP clients
can pass the same arguments to `lightflow.run.list` or read
`lightflow://runs?workflow_id=<id>&status=<status>&limit=<n>`. `lfw runs get`
returns the same full data as `lfw trace`, `lfw artifacts` returns the same
run/stage/node artifact catalog as `/artifacts`, and `lfw runs replay` is the
namespaced run history form of `lfw replay`. `lfw artifacts` accepts `--run`,
`--workflow`, `--kind`, and `--limit` so large local histories can be narrowed
without a server; HTTP `/artifacts`, MCP `lightflow.artifact.list`, and
`lightflow://artifacts?run_id=<run>&workflow_id=<id>&kind=<kind>&limit=<n>`
accept matching filters. Removing a run deletes only that run directory and
clears `last` if it pointed at the removed run.

Trace snapshots follow the same zero-copy boundary as workflow execution:
large files are represented as artifact handles and paths, not embedded file
bytes, model weights, or tensor payloads.

ComfyUI stage inputs retain inline `workflow`, `node_inputs`, and `uploads`
values; they are not replaced with a pointer to a mutable graph file. The
selected runtime fingerprint stores the normalized ComfyUI server URL, resolved
submitted graph SHA-256, and ordered upload target/content hashes. It omits
prompt ids, timestamps, artifact destinations, and Authorization values.
Downloaded files default to
`.lightflow/artifacts/comfyui/<safe-workflow-id>/<prompt-id>/`. Relative
`output_dir` values resolve from the repository root. Upload paths and output
directories must remain under that root after canonicalization; traversal and
symlink escapes are rejected. Remote filenames are
untrusted: LightFlow uses only a sanitized basename with node, field, and index
prefixes. Multipart uploads and downloads stream rather than buffering whole
files, and downloads use unique create-new temporary files plus no-clobber
persistence. `LIGHTFLOW_COMFYUI_AUTHORIZATION` is sent only to the same origin
as configured `LIGHTFLOW_COMFYUI_URL`. The run deadline covers hashing, upload,
submit, polling, and download. Completed histories without files still retain
`history` and `remote_outputs` and succeed.

Nested `NodeExecution.nodes` remain in `execution.json`. Their trace events
store `depth`, `node_path`, and parent identity, while artifact catalog rows use
the deepest producer's `node_path` and avoid duplicates propagated through
parent nodes. Replay walks the same tree when comparing runtime fingerprints.

The HTTP server holds one shared blocking-run permit across workflow execution
and recording, replay, or HTTP MCP handling inside `spawn_blocking`.
`LIGHTFLOW_MAX_BLOCKING_RUNS` defaults to `4`, accepts `1..=64`, and falls back
to `4` for missing or invalid values; `/health` remains outside this pool.

Runtime patches are part of the stored stage definition, so replay uses the
same patch that the original run used. Recorded executions include
`model_locks`, a snapshot of model lock status for executed workflows. Replay
responses include a `replay` report comparing original and replayed runtime
fingerprints plus model-lock fingerprints, so runtime and model changes are
explicit. Pipeline model-lock fingerprints include `stage_index`, so drift is
attributed to the stage that used the model-backed workflow. `lfw run` and
`lfx` accept the patch as inline JSON, stdin, an
`@file` reference, or a project registry name:

```bash
lfw run lightflow.qa --input question=hello --patch @patch.json
lfw run lightflow.qa --input question=hello --patch qa-debug
```

The patch file is serializable graph-runner data:

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
    },
    "draft": {
      "enable": true
    }
  }
}
```

Node keys are graph node ids from the workflow source. `replace_with` and
`fallback_workflow_id` must name workflows already visible through project
discovery, `LFW_PATH`, or Cargo dependencies. `enable` overrides author-time or
CLI disables for that node. `disable` without a fallback skips the node.
Before execution, LightFlow validates patch node keys and `--enable` /
`--disable` toggles against the selected workflow, so patches aimed at another
workflow shape or typoed node ids fail before a run manifest is written.
Replacement and fallback workflows must preserve the patched node's input and
output port names and types; extra ports are allowed, but missing or mismatched
ports fail the run before execution. Extra input ports must be optional or have
defaults, because a graph patch cannot invent values for new required inputs.

Reusable project patches are stored in `.lightflow/patches/<name>.json` and
managed through:

```bash
lfw patch save qa-debug @patch.json
lfw patch list
lfw patch get qa-debug
lfw patch validate qa-debug
lfw patch validate qa-debug --workflow lightflow.qa
lfw patch rm qa-debug
```

The registry is a convenience for authoring and editor tooling. A run manifest
stores the expanded patch data, not a pointer to the registry name, so replay
remains stable when registry entries are edited later.
`lfw patch validate` returns `valid` and `issues`, checking that saved patch
node ids, replacement workflows, fallback workflows, and retry values can be
resolved against the current workflow catalog. With `--workflow <workflow_id>`,
the same command validates the patch against one selected workflow's graph node
ids and replacement/fallback port contracts before execution. `lfw loop check`
treats invalid saved patches as a readiness failure.
The registry is also available over HTTP through `/patches`, `/patches/{name}`,
and `/patches/validate`, and over MCP through `lightflow.patch.*` tools plus
`lightflow://patches/<name>` for read-only patch lookup.
HTTP `/patches/validate?workflow_id=...` and MCP `lightflow.patch.validate`
with `workflow_id` expose the same selected-workflow validation as the CLI.
Those surfaces expose the same serializable patch objects as the CLI.

Each `LFW_PATH` entry may be a LightFlow home or a legacy workflow collection.
A LightFlow home is a normal Cargo workspace:

```text
~/.lightflow/
  Cargo.toml
  repos/
  workflows/
    <crate>/
      Cargo.toml
      src/
        lib.rs
```

The default home at `~/.lightflow` is initialized as a Cargo workspace root.
Its generated manifest uses `members = ["workflows/*"]`.
This gives globally imported workflows one shared dependency environment,
analogous to a small language-specific environment for LightFlow workflows.
`lfw home` prints this root, its `Cargo.toml`, the workflow source directory,
and the repo cache. `lfw new --global` creates workflow crates under
`workflows/<crate>`, and `lfw add --global` writes dependencies
to the home `Cargo.toml`. The backend scans this global workspace manifest for
Cargo `path` dependencies, so global CLI-installed path workflows are available
through normal workflow lookup. `lfw update --global` runs `cargo fetch` in this
workspace, and
`lfw upgrade --global` runs `cargo update`. Version resolution remains Cargo's
job; LightFlow only chooses the workspace where the command runs.

There is no separate LightFlow package database. The global home is a Cargo
workspace, and global installation means adding workspace members or Cargo
dependencies to that workspace. `lfw add` targets one known Cargo package;
`lfw import` targets a workflow repository or collection, discovers
`workflows/<crate>` entries, and adds each discovered workflow crate
as a Cargo path dependency.

The directory name is a short slug, not the full workflow id. For example,
`lightflow.text_plan` can live at `std/text_plan/src/lib.rs`; the Rust DSL
still declares `workflow!()`.

Project workflows are read from `./.lightflow/workflows` before global
`LFW_PATH` workflows. Legacy `./workflows` collections are still read for
compatibility. If both define the same workflow id, the project workflow wins.
Cargo dependency workflows are then scanned as extension crates and cannot
override a project workflow.

## Workflow Crates

For a practical authoring tutorial, see
[Workflow Development Guide](workflow-development.md). This section describes
the underlying project layout and data model.

Each workflow is a Rust library crate with embedded metadata and definition in
`src/lib.rs`:

```rust
use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow! {
        input "value": "json" {
            description: "Value to process.",
            required: true,
            widget: "json",
        }
        output "text": "text" { description: "Processed text output.", }
    }
        .name("Example")
        .description("Reusable workflow definition.")
        .build()
}
```

Workflow id and version are not duplicated in Rust source. `workflow!()` uses
the crate's Cargo package metadata, and static discovery reads the same values
from `Cargo.toml`. Package `lightflow-example-flow` maps to
`lightflow.example_flow`.

Input and output ports can carry Node Schema v1 metadata for editor and agent
tooling: descriptions, required/default values, numeric ranges, enum choices,
widget hints, artifact kinds, and model requirement bindings. The block form
`workflow! { input ... output ... }` keeps metadata with each port, and
legacy `.input(...)` / `.output(...)` calls remain source-compatible.

Composite workflows nest other workflows with `.node()` and connect node ports
with `.edge()`:

```rust
use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
        .name("Parent")
        .depends_on("lightflow.child", "0.1.0")
        .node("child", "lightflow.child")
        .build()
}
```

Nodes can be marked disabled in source with `.disabled_node(...)`. Execution
commands can temporarily override node state with `--disable <node>` and
`--enable <node>` without editing the workflow file.

Reusable workflows do not include `src/main.rs`. If a workflow crate has no
`main.rs`, it is imported or nested by other workflows instead of used as an
executable entrypoint.

The backend accepts `WorkflowSpec` JSON over HTTP/MCP/CLI for tool integration,
but the source-controlled project format is Rust.

Workflow crates can also expose executable workflow entrypoints with normal
Rust `src/bin/*.rs` targets. The library remains the reusable workflow
definition, while each bin is a direct executable for one workflow:

```rust
fn main() -> lightflow::runner::RunnerResult<()> {
    lightflow::runner::run_workflow_from_env(my_workflow_crate::define())
}
```

This keeps workflow projects self-contained: agents and `lfw` can import the
library workflow definition, while users can run the workflow with `cargo run
--bin <name>` or an installed binary without going through the `lfw run`
subcommand.

For fully typed Rust workflows that need internal state machines, implement
the `ContextWorkflow` trait. `Input` is converted into a mutable `Context`,
nodes mutate that context and return the next state, and `Output` is assembled
once at the end:

```rust
use lightflow::preload::*;

struct MyWorkflow;

impl ContextWorkflow for MyWorkflow {
    type Input = Input;
    type Output = Output;
    type Context = Context;
    type State = State;

    fn context(&self, input: Input) -> Context {
        Context { input, answer: None }
    }

    fn initial_state(&self) -> State {
        State::Classify
    }

    async fn step(&self, state: State, ctx: &mut Context) -> anyhow::Result<State> {
        match state {
            State::Classify => classify(ctx).await,
            State::Answer => answer(ctx).await,
            State::End => Ok(State::End),
        }
    }

    fn output(&self, ctx: Context) -> anyhow::Result<Output> {
        Ok(Output { answer: ctx.answer.unwrap_or_default() })
    }
}
```

For cross-workflow composition, use the typed `Workflow<I, O>` function
boundary. A workflow is also a node: it implements `Runnable<I, O>` and can be
placed anywhere a typed task is expected.

```rust
use lightflow::preload::*;

let classify_flow: Workflow<UserInput, Intent> = build_classify_flow();
let search_flow: Workflow<Intent, SearchResult> = build_search_flow();
let answer_flow: Workflow<SearchResult, FinalAnswer> = build_answer_flow();

let answer = classify_flow
    .then(search_flow)
    .then(answer_flow)
    .run(input)
    .await?;
```

The rule is:

```text
external composition: Workflow<Input, Output>
internal implementation: Context + State
```

That keeps workflow composition compile-time checked while still allowing each
workflow to keep rich private execution state.

## Patch And Hook Runtime

Patch behavior is applied at node call boundaries, not by rewriting workflow
source. Hand-written nodes and macro-generated nodes can both use the same
typed hook registry:

```rust
use lightflow::preload::*;

let hooks = HookRegistry::new().hook("search", LogHook);

let result = run_node("search", query, |query| async move {
    search(query).await
}, &hooks).await?;
```

`NodeHook<I, O>` supports `before`, `after`, and `on_error`.
`AroundHook<I, O>` wraps execution through `Next<I, O>` and is the foundation
for logging, metrics, auth, caching, retry, timeout, mock, disable, and
replacement behavior. Registries are typed per node input/output shape so Rust
still checks patch compatibility.

The first concrete patch operations are:

```rust
let hooks = HookRegistry::new()
    .replace("search", |query| async move { mock_search(query).await })
    .disable_with("payment", |request| async move { Ok(PaymentResult::skipped(request)) })
    .retry("llm", 3)
    .timeout_ms("http", 5_000);
```

`run_node` applies `before` once, retries the around/base/replacement/fallback
execution according to policy, applies timeout per attempt, then calls `after`
once on success or `on_error` once after final failure. `#[node]` generates a
`<node>_with_hooks(input, &HookRegistry<_, _>)` entrypoint for single-input
typed nodes, so macro-authored nodes and hand-written nodes share the same
runtime boundary.

CLI graph patches use the same boundary idea but remain workflow-id based and
serializable. They cannot directly name Rust function pointers or closures;
typed function replacement belongs in the SDK `HookRegistry`.

Use `run_node_borrowed` when the input should not be cloned. This is the right
shape for large artifacts and model paths: the workflow layer passes paths and
handles while the backend owns decoding or mmap.

## Agent Skills

Workflow and plugin projects can include agent skills that explain their usage:

```text
<workflow-or-plugin-root>/
  .agent/
    skills/
      <skill-name>/
        SKILL.md
```

`SKILL.md` uses standard agent skill frontmatter with `name`, `description`,
and `version`, followed by concise instructions for the workflow or plugin.
`lfw init --workflow`, `lfw new`, and `lfw init --plugin` generate a starter
skill. When workflow inputs, outputs, model requirements, runtime behavior, or
common commands change, update the corresponding skill in the same change.
`lfw new --runtime <capability>` selects a runtime-aware node template. For
example, `--runtime lightflow.image.generate` generates prompt/image ports,
Node Schema v1 metadata, a runnable deterministic `builtin.preview.v1`
scaffold, a skill example, and a `tests/contract.rs` scaffold for the workflow
crate. The preview verifies pipeline and PNG artifact handling; it does not
represent FLUX, ComfyUI, or production model quality. A real backend contract
must declare its concrete model requirements.
Run `lfw node test <workflow_id>` before publishing or installing a node. The
conformance check validates the workflow graph, `lfw help` contract, Node
Schema v1 metadata, model requirement bindings, and the workflow crate's agent
skill. Runtime conformance builds the real plan, resolves logical FLUX recipes
to the selected physical native or external backend, checks its current
availability, and recursively verifies every reachable leaf in composite and
conditional branches without executing the workflow.
External FLUX runner availability requires `LIGHTFLOW_FLUX_RUNNER` to name a
regular executable file, not merely contain a value.

The backend exposes workflow-backed nodes over HTTP for editor palettes:

```text
GET /openapi.yaml
GET /nodes
GET /nodes/<workflow_id>
GET /workflows/<workflow_id>
GET /workflows/<workflow_id>/plan
GET /executors
GET /models
GET /models?workflow_id=<workflow_id>&status=blocked
GET /runs
GET /runs/<run_id>
GET /runs/<run_id>/events
POST /runs/<run_id>/replay
DELETE /runs/<run_id>
GET /artifacts
```

`/openapi.yaml` serves the same API contract checked into `openapi/`, so
running backends are self-describing for editor and tool clients.

The node endpoints do not create another node file format. They project the
source-controlled workflow crates into node cards with schema, runtime, model,
graph, validation metadata, and model lock status from `lfw.lock`. Runtime
executor entries include a status label, availability reason, data policy, and
model-planning flag, matching `lfw info`; `/executors` exposes that registry
directly for clients that need runtime status without a workflow-specific node
card. `/workflows/<workflow_id>` exposes the source workflow graph for read-only
inspection, and `/workflows/<workflow_id>/plan` projects the selected executor,
atoms, data policy, and model requirements without creating `.lightflow/runs`
entries, so editors and MCP clients can preview runtime choices before
execution. `/models` exposes top-level total, available, blocked, and issue
counts alongside lock status, selected variants, hashes, local paths, and
missing paths plus per-workflow sync/verify commands for model-resource
inspection. `workflow_id` and `status=all|available|blocked` query parameters
filter the same catalog for focused editor and agent views; `lfw models
requirements` returns the same catalog without starting the HTTP server or
touching the Hugging Face cache, and
`lfw models requirements <workflow_id>` filters that catalog to one workflow's
declared requirements. `--blocked`, `--available`, and
`--status all|available|blocked` filter the same catalog by lock readiness for
agent-readable model triage. The run and artifact endpoints project the existing
`.lightflow/runs` directory for editor history, event, replay, removal, and
artifact browsers. Legacy run manifests that predate an
explicit status field are summarized by inferring completed or failed status
from terminal trace events or execution status before falling back to
`unknown`.
The MCP adapter exposes the same workflow, node, executor, model, run, replay,
run removal, plan, OpenAPI, local loop, source-change safety, sibling project
workspace catalog, publish readiness, and artifact projections as JSON-RPC
tools/resources; MCP runs write the same history files with `surface: "mcp"`
events. Parameterized MCP resources are advertised through
`resources/templates/list`, including selected workflow definitions,
dependencies, plans, publish readiness, node cards, filtered model requirement,
run history, run details, run event timelines, artifact catalog,
selected-workflow loop readiness, patch lookup, project-scoped publish
readiness, project workspace catalog, and selected-workflow or project-scoped
release readiness templates.

Standard workflow nodes live in the `lightflow-std` workflow project under
`projects/lightflow-std/workflows/<crate>` when that sibling project is
checked out as part of the local project set. The core repository discovers
project workflow sources listed in `projects/lightflow-projects.toml`
`[workflows].default_sources`; this repo keeps `lightflow-std` there so
baseline text, JSON, image helper, control, model, and LLM nodes are available
after a normal submodule checkout. Other sibling workflow projects under
`projects/`, including `lightflow-flux` and `lightflow-rig`, are opt-in
workflow sources that should be added through `LFW_PATH`, `lfw import`, an
explicit workflow search path, or the default source list when a run needs
those domain-specific nodes.
Current prompt-graph helpers include `lightflow.text_concat`,
`lightflow.text_template`, `lightflow.text_regex`, and
`lightflow.json_extract`; image and mask artifact helpers include
`lightflow.image_load`, `lightflow.image_save`, `lightflow.image_resize`,
`lightflow.image_crop`, `lightflow.image_upscale`, and
`lightflow.mask_compose`; preview diffusion helpers include
`lightflow.image_edit` and `lightflow.image_inpaint`; control helpers include
`lightflow.control_if`, `lightflow.control_switch`, `lightflow.control_merge`,
and `lightflow.control_split`; model helpers include `lightflow.model_select`
and `lightflow.model_lock_check`; LLM helpers include
`lightflow.llm_generate`, `lightflow.llm_classify`, and
`lightflow.llm_structured_output`. Each has a matching
`.agent/skills/<skill-name>/SKILL.md` file and a builtin runtime capability.

`lfw sync --apply` discovers skills from workflow/plugin projects,
asks whether to symlink each skill into the current project's `.agents/skills`
directory or the global `~/.agents/skills` directory, and records the answer in
`lfw.lock`. A recorded answer is not asked again. Delete `lfw.lock` to choose
again.

## Plugin Crates

`lfw init --plugin` creates a single standard Cargo crate:

```text
Cargo.toml
src/
  lib.rs
tests/
  contract.rs
.agent/
  skills/
    <skill-name>/
      SKILL.md
```

Plugin crates and workflow crates have the same Rust/Cargo status: both expose
`pub fn define() -> WorkflowSpec`, both can use normal Cargo dependencies, and
both import `lightflow`. The generated contract test calls `define()` and
checks the basic workflow contract, while the generated skill documents how an
agent should use the plugin workflow. The core `lightflow` crate does not
import plugin or workflow crates.

## Imported Workflow Dependencies

A workflow can be installed as a Cargo dependency. The backend scans local
workflow crates under `workflows/<crate>/` and also
scans `path` dependencies declared in the project `Cargo.toml`:

```toml
[dependencies]
lightflow-text-prompt = { path = "projects/lightflow-std/workflows/text_prompt" }
```

If the dependency target contains `src/lib.rs` with `pub fn define() ->
WorkflowSpec`, it is added to the workflow registry and can satisfy
`.depends_on(...)` and `.node(...)` references.

Git dependencies use the same manifest shape:

```toml
[dependencies]
lightflow-text-prompt = { git = "https://github.com/lightjunction/lightflow-std", package = "lightflow-text-prompt" }
```

`lfw add` writes these dependencies into the root host package's
`[dependencies]` table:

```bash
lfw add lightflow-text-prompt --version 0.1.0
lfw add lightflow-text-prompt --path projects/lightflow-std/workflows/text_prompt
lfw add lightflow-text-prompt --path projects/lightflow-std/workflows/text_prompt --editable
lfw add lightflow-text-prompt --git https://github.com/lightjunction/lightflow-std --package lightflow-text-prompt
lfw add lightflow-text-prompt --version 0.1.0 --global
```

For a self-contained workflow repository or local collection that contains
multiple workflow crates, use `lfw import` instead of adding each crate by
hand:

```bash
lfw import --global /path/to/lightflow-flux
lfw import --global https://github.com/lightjunction/lightflow-flux.git
```

`lfw import` discovers `workflows/<crate>` entries and records each
workflow crate as a Cargo path dependency in the target project or global
workspace. Git sources are cloned into LightFlow's managed repo store first,
then installed from that clone. The original workflow repository remains the
self-contained Cargo workspace that owns its `[workspace.dependencies]`.

This is a thin manifest-editing layer over Cargo. Cargo remains responsible for
fetching, updating, lockfile resolution, feature resolution, and publishing;
LightFlow is responsible for workflow discovery, node metadata, model sync, and
agent skill sync.

New workflow workspaces use the root manifest as a non-publishable host package
(`version = "0.0.0"`, `publish = false`) with
`[lib] path = ".lightflow/workspace.rs"`. This lets standard `cargo add`,
including `cargo add --path` and `cargo add --git`, install one external
workflow library directly. The same manifest retains the official workflow
member glob and `[workspace.dependencies].lightflow` for locally generated
member crates.

`--editable` is only valid with `--path`. It keeps the manifest as a standard
Cargo path dependency and makes the CLI result report `"editable": true`,
which distinguishes a deliberate live-source development install from a normal
path install.

Workflow dependencies can embed the same install metadata in the Rust file:

```rust
workflow!()
    .depends_on_crate("lightflow.text_prompt", "0.1.0", "lightflow-text-prompt")
    .depends_on_path(
        "lightflow.local_std",
        "0.1.0",
        "lightflow-text-prompt",
        "projects/lightflow-std/workflows/text_prompt",
    )
    .depends_on_git(
        "lightflow.remote_std",
        "0.1.0",
        "lightflow-text-prompt",
        "https://github.com/lightjunction/lightflow-std",
        "lightflow-text-prompt",
    )
```

`lfw sync` delegates Rust module fetching to Cargo. If a declared workflow
dependency is not installed yet and has install metadata, `lfw sync --apply`
adds the Cargo dependency to the workspace manifest before running
`cargo fetch`.

## Publishing Workflow Crates

`lfw init --workflow` and `lfw new` generate workflow crates with versioned `lightflow`
dependencies and without `publish = false`, so they can become crates.io
packages once their metadata is ready.

```bash
lfw publish lightflow.example
lfw publish lightflow.example --apply
lfw publish --workflows
lfw publish --workflows --require-publishable
lfw publish --workflows --apply
lfw publish --workflows --apply --allow-dirty
```

Repository-internal examples can still opt out with `publish = false`.
`lfw publish` reports those as non-publishable instead of trying to upload
them. Workflow publish checks also parse each workflow crate's `src/lib.rs` and
block unresolved generated `TODO` placeholders in workflow, input, or output
descriptions. Crates.io dependency blockers are checked across normal, build,
dev, and target-specific dependency sections, including inherited
`workspace = true` entries from `[workspace.dependencies]`.
`lfw publish --workflows` publishes a workflow workspace by
publishing each workflow crate individually, including present linked workflow
project workspaces under `projects/`. The plan is dependency ordered for local
workflow `path` dependencies, including inherited workspace path dependencies,
and includes top-level total/publishable/blocked counts alongside per-crate
workflow ids, workspace labels, and blockers.
Duplicate workflow ids are deduped in favor of root workspace definitions in
the default catalog.
`--apply` requires every workflow crate to pass publish checks and the
`lfw loop changes` review gate before any upload is attempted. Every `--apply`
path runs Cargo's publish dry-run command before the real upload command.
`--allow-dirty` is an explicit opt-in for publishing uncommitted working tree
changes through Cargo.
`--require-publishable` is a non-network gate that exits non-zero when the
computed publish plan has blockers.
`GET /publish`, `lightflow.workflow.publish_list`, and `lightflow://publish`
expose the same dependency-ordered workflow publish plan for HTTP, editor, and
MCP clients, including present linked workflow project workspaces under
`projects/` with top-level total/publishable/blocked counts, per-crate
workflow ids and workspace labels, a top-level dependency-ordered dry-run
command list, and duplicate workflow ids deduped in favor of root workspace
definitions in the default catalog. Use `GET /publish?project=<name>`, MCP
`lightflow.workflow.publish_list` with `project`, or
`lightflow://publish?project=<name>` to narrow the catalog to one linked
project workspace using the same full-name, `projects/<name>` label, path, or
short-alias matching as the CLI; scoped publish views return that linked
workspace's matching workflow crates even when the default catalog dedupes the
same workflow id in favor of the root workspace. MCP `resources/templates/list`
advertises the parameterized publish resource as
`lightflow://publish?project={project}`.

## Versioning

Workflow definitions use SemVer strings:

```rust
workflow!()
```

Explicit workflow dependencies currently use exact SemVer requirements:

```rust
.depends_on("lightflow.text_prompt", "0.1.0")
```

The install-aware forms keep the same exact workflow version and add Cargo
resolution metadata:

```rust
.depends_on_crate("lightflow.text_prompt", "0.1.0", "lightflow-text-prompt")
```

The backend also accepts `*` for an unconstrained local dependency. Range
requirements such as `^0.1` and `>=0.1` are intentionally not supported yet;
they will be added after the exact-version update path is stable.

## Model Requirements

Model requirements are embedded in the workflow file. A workflow can declare an
abstract model capability and provide multiple Hugging Face variants:

```rust
workflow!()
    .hf_model(
        "image_model",
        "flux2-safetensors",
        "text-to-image",
        "safetensors",
        "black-forest-labs/FLUX.2-dev",
        "flux2-dev.safetensors",
    )
    .hf_model(
        "image_model",
        "flux2-gguf",
        "text-to-image",
        "gguf",
        "city96/FLUX.2-dev-gguf",
        "flux2-dev-q4.gguf",
    )
```

`lfw sync` will not download every variant. Without a `--model
image_model=<variant>` selection it reports the unresolved model requirement.
With a selection it builds an `hf download ...` command and, when `--apply` is
used, executes it through the Hugging Face CLI so the artifact is managed by
the global HF cache.

`--auto-model` is the one-step path for local setup:

```bash
lfw sync lightflow.image_prompt --auto-model --apply
```

It detects total RAM and NVIDIA VRAM, then picks the best declared variant for
each unresolved model requirement. Explicit `--model` and `--hf-model` choices
take precedence. The selection is reported under `auto_model.selections`, and
the detected hardware is reported under `hardware`.

Users can force a custom Hugging Face artifact without editing the workflow:

```bash
lfw sync lightflow.image_prompt \
  --hf-model image_model=gguf:owner/repo:path/to/model.gguf \
  --apply
```

The requirement id still has to exist in the workflow, but the repo/file does
not have to be listed as a recommended variant. The resulting download plan is
marked with `custom: true`.

Runtime adapters read model paths from `lfw.lock`. The FLUX runtime adapter
passes those locked paths to the native `flux-native` backend when it is
compiled in, rather than copying model files into the workflow project. Builds
without the native backend can pass the same locked paths to the executable
configured by `LIGHTFLOW_FLUX_RUNNER`. The same task contract covers
text-to-image, image edit, and inpaint tasks; edit tasks add an input image
path, and inpaint tasks add both input image and mask paths.

Native text-to-image keeps the loaded model session in process memory and keys
that session by the resolved `flux_model`, `llm_model`, and `vae_model` paths.
When `count`, `num_images`, or `batch_count` requests multiple text-to-image
outputs, the native backend receives one batch request while LightFlow still
records each output as an individual artifact path.
This does not change the on-disk layout: `lfw.lock` remains the source of model
paths, run records still store artifact paths, and model weights stay in the
Hugging Face cache. The residency benefit lasts for the lifetime of the
LightFlow process.

RIG LLM workflows declare `lightflow.llm.generate` and keep provider selection
as runtime input. `provider`, `model`, `prompt`, `system`, `api_key`,
`base_url`, `temperature`, `max_tokens`, and `additional_params` are workflow
inputs, not source-controlled secrets. Environment variables such as
`OPENAI_API_KEY`, `OPENAI_BASE_URL`, `ANTHROPIC_API_KEY`,
`OLLAMA_API_BASE_URL`, `OPENROUTER_API_KEY`, `DEEPSEEK_API_KEY`, `XAI_API_KEY`,
`LIGHTFLOW_RIG_PROVIDER`, and `LIGHTFLOW_RIG_MODEL` can provide defaults.

Keep large runtime data zero-copy from LightFlow's perspective: `lfw.lock`
stores cache paths, workflow inputs store image and mask paths, artifacts store
output paths and metadata, and native GGUF loading uses mmap. Do not commit or
copy model weights, decoded images, or tensor intermediates into workflow
source directories.

## Batch Run State

Batch execution state is local runtime data and is not meant to be committed:

```text
.lightflow/
  runs/
    <run_id>/
      manifest.json   # scheduler policy and defaults
      input.jsonl     # original submitted queue
      jobs.jsonl      # durable job status, outputs, artifacts, errors
      events.jsonl    # append-only progress stream
```

Each input JSONL line is one job. A job can include its own `workflow_id`, or
the CLI can provide a default with `--workflow`:

```json
{"id":"frame-001","inputs":{"image_path":"input/001.png","mask_path":"masks/001.png","prompt":"repair the scratch"}}
```

`lfw batch run` uses `--max-gpu-jobs` to limit concurrent workflow executions
so a large queue cannot launch every image job at once. `--max-cpu-jobs`,
`--batch-size`, `--reserve-mem`, `--reserve-vram`, and `--max-load` are stored
with the run policy for runtime adapters that can do preprocessing pools,
micro-batching, or backend-specific resource checks. `lfw batch resume <run_id>`
reuses the same state and skips completed jobs.

## Execution Inputs

`lfx` is an alias for `lfw run`:

```bash
lfx lightflow.image_prompt --input positive="a quiet lake" --input negative=blur
lfx lightflow.image_prompt --text "a quiet lake" --image ./input.png --output ./out.png
lfx lightflow.image_prompt --input positive="a quiet lake" --disable render
```

Input values are parsed as JSON when possible and otherwise treated as strings.
`--inputs <json|-|@file>` merges a JSON object into the run inputs. The
shortcuts `--text`, `--prompt`, `--image`, and `--output` populate `text`,
`prompt`, `image_path`, and `output_path` respectively.
The execution result records workflow inputs, workflow outputs, and per-node
status, inputs, and outputs.

## Not Stored Here

Do not commit runtime state, credentials, generated artifacts, caches, or model
weights into the workflow project.
