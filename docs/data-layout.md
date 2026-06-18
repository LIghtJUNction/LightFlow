# Data Layout

LightFlow project files are ordinary source-controlled files under `workflows/`.
`lfw init --workflow` creates this layout:

```text
workflows/
  <category>/
    <short-name>/
      Cargo.toml
      src/
        lib.rs
```

Shared user workflows follow XDG paths. The shell sources:

```text
$XDG_CONFIG_HOME/lightflow/.lfwrc
# default: ~/.config/lightflow/.lfwrc
```

For bash and zsh, the rc file uses shell-style export syntax:

```bash
export LFW_PATH="$HOME/.local/share/lightflow"
```

For fish, `lfw init` writes fish syntax instead:

```fish
set -gx LFW_PATH "$HOME/.local/share/lightflow"
```

`lfw init` detects `SHELL` and appends `source <rc>` to `.bashrc`, `.zshrc`,
or `$XDG_CONFIG_HOME/fish/config.fish`. Project workflows are discovered from
the current working directory's `workflows/` tree and never need `LFW_PATH`.
`LFW_PATH` is only for global or shared workflow collections. If there is no
exported `LFW_PATH`, `lfw` uses the XDG default
`$XDG_DATA_HOME/lightflow`, or
`~/.local/share/lightflow` when `XDG_DATA_HOME` is not set. `lfw`
does not parse `.lfwrc` directly at runtime; it reads the environment provided
by the shell.

Generated image outputs default to the user's XDG Pictures directory, under a
`lightflow` subdirectory. On Linux, LightFlow resolves this from
`$XDG_PICTURES_DIR` when exported, then `$XDG_CONFIG_HOME/user-dirs.dirs`, then
falls back to `$HOME/Pictures/lightflow`. Explicit `output_path` inputs always
win.

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
`execution.json` stores the actual workflow or pipeline result. Composite node
records include status, inputs, outputs, artifact handles, `duration_ms`, and
`attempts`. Failed runs store `status: "failed"` and an error object in
`execution.json`. `events.jsonl` is append-only trace data: it starts with
`run_started`, includes one `node_completed` or `node_skipped` event for each
successful graph node, and ends with `run_finished` or `run_failed`. `lfw trace
last` reads this directory without executing anything, and `lfw replay <run_id>`
uses the stored stage definitions to create a new run.

Run history can also be managed directly:

```bash
lfw runs list
lfw runs get last
lfw runs get run-1781797000000
lfw runs rm run-1781797000000
```

`lfw runs list` returns compact manifest summaries sorted by newest completion
time first. `lfw runs get` returns the same full data as `lfw trace`. Removing
a run deletes only that run directory and clears `last` if it pointed at the
removed run.

Trace snapshots follow the same zero-copy boundary as workflow execution:
large files are represented as artifact handles and paths, not embedded file
bytes, model weights, or tensor payloads.

Runtime patches are part of the stored stage definition, so replay uses the
same patch that the original run used. `lfw run` and `lfx` accept the patch as
inline JSON, stdin, an `@file` reference, or a project registry name:

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

Reusable project patches are stored in `.lightflow/patches/<name>.json` and
managed through:

```bash
lfw patch save qa-debug @patch.json
lfw patch list
lfw patch get qa-debug
lfw patch validate qa-debug
lfw patch rm qa-debug
```

The registry is a convenience for authoring and editor tooling. A run manifest
stores the expanded patch data, not a pointer to the registry name, so replay
remains stable when registry entries are edited later.

Each `LFW_PATH` entry may be a LightFlow home or a legacy workflow collection.
A LightFlow home is a normal Cargo workspace:

```text
$XDG_DATA_HOME/lightflow/
  Cargo.toml
  repos/
  workflows/
    <category>/
      <short-name>/
        Cargo.toml
        src/
          lib.rs
```

The default home at `$XDG_DATA_HOME/lightflow` is initialized as a Cargo
workspace root. Its generated manifest uses `members = ["workflows/*/*"]`.
This gives globally installed workflows one shared dependency environment,
analogous to a small language-specific environment for LightFlow workflows.
`lfw home` prints this root, its `Cargo.toml`, the workflow source directory,
and the repo cache. `lfw new --global` creates workflow crates under
`workflows/<category>/<short-name>`, and `lfw add --global` writes dependencies
to the home `Cargo.toml`. The backend scans this global workspace manifest for
Cargo `path` dependencies, so global CLI-installed path workflows are available
through normal workflow lookup. `lfw update --global` runs `cargo fetch` in this
workspace, and
`lfw upgrade --global` runs `cargo update`. Version resolution remains Cargo's
job; LightFlow only chooses the workspace where the command runs.

The directory name is a short slug, not the full workflow id. For example,
`lightflow.text_plan` can live at `std/text_plan/src/lib.rs`; the Rust DSL
still declares `workflow("lightflow.text_plan")`.

Project workflows are read from `./workflows` before global `LFW_PATH`
workflows. If both define the same workflow id, the project workflow wins.
Cargo dependency workflows are then scanned as extension crates and cannot
override a project workflow.

## Workflow Crates

Each workflow is a Rust library crate with embedded metadata and definition in
`src/lib.rs`:

```rust
use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.example")
        .version("0.1.0")
        .name("Example")
        .description("Reusable workflow definition.")
        .input("value", "json")
        .output("text", "text")
        .build()
}
```

Composite workflows nest other workflows with `.node()` and connect node ports
with `.edge()`:

```rust
use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.parent")
        .version("0.1.0")
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

`lfw sync --apply` discovers skills from installed workflow/plugin projects,
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
```

Plugin crates and workflow crates have the same Rust/Cargo status: both expose
`pub fn define() -> WorkflowSpec`, both can use normal Cargo dependencies, and
both import `lightflow`. The core `lightflow` crate does not import plugin or
workflow crates.

## Installed Workflow Dependencies

A workflow can be installed as a Cargo dependency. The backend scans local
workflow crates under `workflows/<category>/<short-name>/` and also
scans `path` dependencies declared in the project `Cargo.toml`:

```toml
[workspace.dependencies]
lightflow-std = { path = "workflows/std/std" }
```

If the dependency target contains `src/lib.rs` with `pub fn define() ->
WorkflowSpec`, it is added to the workflow registry and can satisfy
`.depends_on(...)` and `.node(...)` references.

Git dependencies use the same manifest shape:

```toml
[dependencies]
lightflow-std = { git = "https://github.com/lightjunction/LightFlow", package = "lightflow-std" }
```

`lfw add` writes these dependencies into the workspace manifest:

```bash
lfw add lightflow-std --version 0.1.1
lfw add lightflow-std --path workflows/std/std
lfw add lightflow-std --path ../lightflow-std --editable
lfw add lightflow-std --git https://github.com/lightjunction/LightFlow --package lightflow-std
lfw add lightflow-std --version 0.1.1 --global
```

For a self-contained workflow repository or local collection that contains
multiple workflow crates, use `lfw install` instead of adding each crate by
hand:

```bash
lfw install --global /path/to/lightflow-flux
lfw install --global https://github.com/lightjunction/lightflow-flux.git
```

`lfw install` discovers `workflows/<category>/<crate>` entries and records each
workflow crate as a Cargo path dependency in the target project or global
workspace. Git sources are cloned into LightFlow's managed repo store first,
then installed from that clone. The original workflow repository remains the
self-contained Cargo workspace that owns its `[workspace.dependencies]`.

`--editable` is only valid with `--path`. It keeps the manifest as a standard
Cargo path dependency and makes the CLI result report `"editable": true`,
which distinguishes a deliberate live-source development install from a normal
path install.

Workflow dependencies can embed the same install metadata in the Rust file:

```rust
workflow("lightflow.image_prompt")
    .depends_on_crate("lightflow.std", "0.1.0", "lightflow-std")
    .depends_on_path("lightflow.local_std", "0.1.0", "lightflow-std", "../lightflow-std")
    .depends_on_git(
        "lightflow.remote_std",
        "0.1.0",
        "lightflow-std",
        "https://github.com/lightjunction/LightFlow",
        "lightflow-std",
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
```

Repository-internal examples can still opt out with `publish = false`.
`lfw publish` reports those as non-publishable instead of trying to upload
them.

## Versioning

Workflow definitions use SemVer strings:

```rust
workflow("lightflow.std")
    .version("0.1.0")
```

Explicit workflow dependencies currently use exact SemVer requirements:

```rust
.depends_on("lightflow.std", "0.1.0")
```

The install-aware forms keep the same exact workflow version and add Cargo
resolution metadata:

```rust
.depends_on_crate("lightflow.std", "0.1.0", "lightflow-std")
```

The backend also accepts `*` for an unconstrained local dependency. Range
requirements such as `^0.1` and `>=0.1` are intentionally not supported yet;
they will be added after the exact-version update path is stable.

## Model Requirements

Model requirements are embedded in the workflow file. A workflow can declare an
abstract model capability and provide multiple Hugging Face variants:

```rust
workflow("lightflow.image_prompt")
    .version("0.1.0")
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
