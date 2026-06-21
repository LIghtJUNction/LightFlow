# LightFlow

LightFlow is a backend-first workflow system. The current backend deliberately
keeps the domain model small:

- Workflow: a reusable leaf unit or a directed graph that nests other workflows.

There is no built-in agent loop, no CortexFS runtime dependency, and no
visual-editor-owned workflow format. Workflows are Rust library crates in the
repository so normal coding tools, including Codex, can edit and review them.

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
- frontend implementation

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
```

## Rust Workflow Crates

Reusable workflows are library crates with `src/lib.rs` and no `src/main.rs`:

```rust
use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.text_plan")
        .version("0.1.0")
        .name("Text Plan")
        .input("value", "json")
        .input_description("value", "Structured request payload.")
        .input_required("value", true)
        .input_widget("value", "json")
        .output("result", "text")
        .output_description("result", "Generated text result.")
        .depends_on("lightflow.std", "0.1.0")
        .depends_on("lightflow.text_prompt", "0.1.0")
        .node("identity", "lightflow.std")
        .node("prompt", "lightflow.text_prompt")
        .edge("identity", "value", "prompt", "value")
        .build()
}
```

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

## Quickstart

```bash
cargo run --bin lfw -- init --workflow
cargo run --bin lfw -- new demo_echo --category demo --name "Demo Echo"
cargo run --bin lfw -- run lightflow.demo_echo --input value='"hello"'
cargo run --bin lfw -- serve --port 5174
```

After `lfw serve` starts, inspect the backend contract used by editor clients:

```bash
curl http://127.0.0.1:5174/nodes
curl http://127.0.0.1:5174/runs
```

## CLI

```bash
cargo run --bin lfw -- init --workflow
cargo run --bin lfw -- init --plugin
cargo run --bin lfw -- new my_flow --category std --name "My Flow"
cargo run --bin lfw -- new my_flux_sampler --category image --runtime lightflow.image.generate
cargo run --bin lfw -- new my_global_flow --category std --global
cargo run --bin lfw -- info
cargo run --bin lfw -- home
cargo run --bin lfw -- add lightflow-std --version 0.1.1
cargo run --bin lfw -- add lightflow-std --path ../lightflow-std --editable
cargo run --bin lfw -- add lightflow-std --version 0.1.1 --global
cargo run --bin lfw -- import ../lightflow-flux --global
cargo run --bin lfw -- list
cargo run --bin lfw -- list --categories
cargo run --bin lfw -- ls --detail
cargo run --bin lfw -- workflows list
cargo run --bin lfw -- workflows get lightflow.text_plan
cargo run --bin lfw -- help lightflow.text_plan
cargo run --bin lfw -- workflows help lightflow.text_plan
cargo run --bin lfw -- deps lightflow.text_plan
cargo run --bin lfw -- run lightflow.text_plan --input value='{"topic":"demo"}'
cargo run --bin lfw -- run lightflow.text_plan --input value='{"topic":"demo"}' --patch @patch.json
cargo run --bin lfw -- patch save qa-debug @patch.json
cargo run --bin lfw -- run lightflow.text_plan --input value='{"topic":"demo"}' --patch qa-debug
cargo run --bin lfw -- trace last
cargo run --bin lfw -- runs list
cargo run --bin lfw -- runs get last
cargo run --bin lfw -- replay run-1781797000000
cargo run --bin lfx -- lightflow.text_plan --input value='{"topic":"demo"}' --disable prompt
cargo run --bin lfw -- workflows validate '{"id":"lightflow.example","version":"0.1.0","name":"Example"}'
cargo run --bin lfw -- node test lightflow.text_to_image
cargo run --bin lfw -- publish lightflow.std
cargo run --bin lfw -- mcp '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
cargo run --bin lfw -- serve --port 5174
curl http://127.0.0.1:5174/nodes
curl http://127.0.0.1:5174/nodes/lightflow.text_to_image
curl http://127.0.0.1:5174/models
curl http://127.0.0.1:5174/runs
curl http://127.0.0.1:5174/runs/last/events
curl http://127.0.0.1:5174/artifacts
```

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
and documentation:

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
returns one node card, and `/models` returns model requirements with their port
bindings. These endpoints are the backend contract for a ComfyUI-style node
palette without introducing a separate node source format.

The same backend exposes project-local run history for editor history panels:
`/runs` lists `.lightflow/runs`, `/runs/{run_id}` returns manifest, execution,
and events, `/runs/{run_id}/events` returns only append-only events, and
`/artifacts` lists artifact handles found in recorded executions.

The standard node library now includes executable text and JSON helpers for
prompt graphs: `lightflow.text.concat`, `lightflow.text.template`,
`lightflow.text.regex`, and `lightflow.json.extract`; image and mask helpers
`lightflow.image.load`, `lightflow.image.save`, `lightflow.image.resize`,
`lightflow.image.crop`, `lightflow.image.upscale`, and
`lightflow.mask.compose`; preview diffusion nodes `lightflow.image.edit` and
`lightflow.image.inpaint`; control helpers `lightflow.control.if`,
`lightflow.control.switch`, `lightflow.control.merge`, and
`lightflow.control.split`; model helpers `lightflow.model.select` and
`lightflow.model.lock_check`; and LLM helpers `lightflow.llm.generate`,
`lightflow.llm.classify`, and `lightflow.llm.structured_output`, alongside
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
lfw runs rm run-1781797000000
lfw replay run-1781797000000
```

Replay uses the stored stage definitions and writes a new run directory, so the
original history remains immutable.
`lfw runs list` returns compact run summaries for editor/history browsers,
while `lfw runs get` returns the same full manifest, execution, and event data
as `lfw trace`.

Composite workflow executions also record node-level trace data. Each node in
`execution.json` includes its status, input/output snapshots, artifact handles,
duration in milliseconds, and attempt count. `events.jsonl` expands that into
append-only `node_completed` or `node_skipped` events between `run_started` and
`run_finished`.

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
it without compiling user code.

Reusable graph patches can be saved in the project patch registry:

```bash
lfw patch save qa-debug @patch.json
lfw patch list
lfw patch get qa-debug
lfw patch validate qa-debug
lfw run lightflow.text_plan --input value=hello --patch qa-debug
lfw patch rm qa-debug
```

The registry lives under `.lightflow/patches/<name>.json`. `lfw run --patch
<name>` expands the registered patch before execution, and the expanded patch
is stored in the run manifest. Replay therefore uses the original run's patch
even if the registry entry changes later.

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
export LFW_PATH='/home/alice/.local/share/lightflow'
```

Project workflows are discovered automatically from the current working
directory's `workflows/` tree. `LFW_PATH` is reserved for global or shared
workflow homes. If `XDG_DATA_HOME` is not set, the default global home is
`~/.local/share/lightflow`. Run `lfw home` to print the active home, manifest,
workflow source directory, and repo cache. `LFW_PATH` uses the platform
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
`./workflows`. `lfw init --plugin` creates a single standard Cargo crate that
can expose a workflow from `src/lib.rs`. `lfw new --global` creates a workflow
crate under the default global home's `workflows/` tree; `lfw add --global`
writes dependencies to the default global home's `Cargo.toml`. Those global
path dependencies are discovered from the global home manifest, so a workflow
installed with `lfw add --global --path ...` can be used from any project that
uses the same XDG data directory or `LFW_PATH`.

Use `lfw add` when the target is one known Cargo package. Use `lfw import`
when the target is a workflow repository or collection and LightFlow should
scan `workflows/<category>/<crate>` for multiple workflow crates.

Workflow dependencies are Cargo dependencies. A local standard workflow can be
installed with:

```toml
[workspace.dependencies]
lightflow-std = { path = "workflows/std/std" }
```

Use `--editable` with `--path` for a development install. It records the same
Cargo path dependency, keeps the source tree live for edits, and marks the CLI
result as editable:

```bash
lfw add lightflow-std --path ../lightflow-std --editable
```

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
workflow("lightflow.image_prompt")
    .depends_on_path("lightflow.std", "0.1.0", "lightflow-std", "../lightflow-std")
    .depends_on_git(
        "lightflow.std",
        "0.1.0",
        "lightflow-std",
        "https://github.com/lightjunction/LightFlow",
        "lightflow-std",
    )
```

`lfw sync --apply` uses those hints to add missing Cargo dependencies before
running `cargo fetch`.

## Versioning

Workflow versions are SemVer strings. The current resolver supports exact
requirements and `*`:

```rust
workflow("lightflow.std")
    .version("0.1.0")
    .depends_on("lightflow.other", "0.1.0")
```

Range requirements such as `^0.1` and `>=0.1` are planned after the exact
version update path is stable.

## Publishing

`lfw publish` creates a Cargo publish plan by default. It checks the target
manifest for basic crates.io blockers such as `publish = false`, non-SemVer
versions, git dependencies, and path dependencies without a version.

```bash
lfw publish                  # root lightflow crate plan
lfw publish lightflow.std    # workflow crate plan
lfw publish --crate path/to/crate
lfw publish --workflows      # all workflow crate plans in dependency order
lfw publish lightflow.std --apply
lfw publish --workflows --apply --allow-dirty
```

`--apply` runs `cargo publish --manifest-path ...`. Without `--apply`, no
network publish is attempted and the generated command includes `--dry-run`.
`--workflows` scans workflow crates under `workflows/*/*`, orders local path
dependencies before dependents, and refuses to upload anything unless every
workflow crate passes the static publish checks. With `--apply`, it first runs
the dry-run publish commands for every workflow crate, then runs the real
publish commands.
`--allow-dirty` forwards Cargo's explicit dirty-worktree override to both
preflight and upload commands.

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
  --workflow lightflow.image.inpaint \
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
curl http://127.0.0.1:5174/workflows
curl http://127.0.0.1:5174/workflows/lightflow.text_plan
curl http://127.0.0.1:5174/workflows/lightflow.text_plan/dependencies
curl -X POST http://127.0.0.1:5174/workflows/lightflow.text_plan/run \
  -H 'content-type: application/json' \
  -d '{"inputs":{"value":"hello"},"disabled_nodes":["prompt"]}'
```
