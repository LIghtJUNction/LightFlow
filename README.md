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
        .output("result", "text")
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

## CLI

```bash
cargo run --bin lfw -- init --workflow
cargo run --bin lfw -- init --plugin
cargo run --bin lfw -- new my_flow --category std --name "My Flow"
cargo run --bin lfw -- new my_global_flow --category std --global
cargo run --bin lfw -- add lightflow-std --version 0.1.1
cargo run --bin lfw -- add lightflow-std --path ../lightflow-std --editable
cargo run --bin lfw -- add lightflow-std --version 0.1.1 --global
cargo run --bin lfw -- list
cargo run --bin lfw -- list --categories
cargo run --bin lfw -- ls --detail
cargo run --bin lfw -- workflows list
cargo run --bin lfw -- workflows get lightflow.text_plan
cargo run --bin lfw -- deps lightflow.text_plan
cargo run --bin lfw -- run lightflow.text_plan --input value='{"topic":"demo"}'
cargo run --bin lfx -- lightflow.text_plan --input value='{"topic":"demo"}' --disable prompt
cargo run --bin lfw -- workflows validate '{"id":"lightflow.example","version":"0.1.0","name":"Example"}'
cargo run --bin lfw -- publish lightflow.std
cargo run --bin lfw -- mcp '{"jsonrpc":"2.0","id":1,"method":"tools/list"}'
cargo run --bin lfw -- serve --port 5174
```

`lfx` is an alias for `lfw run`. It accepts generic JSON inputs, common text /
image / output-path flags, and temporary node toggles:

```bash
lfx lightflow.text_plan --input value=hello
lfx lightflow.text_plan --inputs '{"value":{"topic":"demo"}}'
lfx lightflow.text_to_image --text "a quiet lake" --output ./out.png
lfx lightflow.text_plan --input value=hello --disable prompt --enable prompt
```

The current runner validates the workflow graph, executes nodes in topological
order, and uses passthrough semantics for generic leaf workflows. FLUX image
generation, edit, and inpaint workflows declare synced model requirements and
delegate sampling to LightFlow's native `flux-native` backend when that feature
is enabled. Builds without the native backend can fall back to the executable
named by `LIGHTFLOW_FLUX_RUNNER`; LightFlow passes the task, prompt, optional
source image and mask paths, sampling settings, output path, and locked model
paths to that runner.

Text-generation workflows can declare the `lightflow.llm.generate` runtime
capability. Builds compiled with `--features rig` execute that runtime through
`rig-core`, with the provider, model, prompt, system prompt, API key, base URL,
temperature, max token count, and extra provider parameters supplied as workflow
inputs or environment defaults. The runtime currently supports OpenAI-compatible
chat APIs, OpenAI Responses, Anthropic, Ollama, OpenRouter, DeepSeek, xAI, and
a local `mock` provider for tests.

## Installing Workflows

LightFlow stores user shell configuration in:

```text
$XDG_CONFIG_HOME/lightflow/.lfwrc
# default: ~/.config/lightflow/.lfwrc
```

`lfw init` creates the file when missing and appends a source line to the
detected shell startup file (`.bashrc`, `.zshrc`, or fish `config.fish`):

```bash
source ~/.config/lightflow/.lfwrc
export LFW_PATH='/home/alice/.local/share/lightflow/workflows'
```

Project workflows are discovered automatically from the current working
directory's `workflows/` tree. `LFW_PATH` is reserved for global or shared
workflow collections. If `XDG_DATA_HOME` is not set, the default global
collection is `~/.local/share/lightflow/workflows`. `LFW_PATH` uses the
platform path-list format, so multiple global collections can be searched.
`lfw` itself reads the environment variable provided by the shell; it does not
parse `.lfwrc` as a runtime config file. The default global collection is also
initialized as a Cargo workspace with `members = ["*/*"]`, so globally
installed workflow crates share one dependency environment.

`lfw init --workflow` creates a project workflow collection under
`./workflows`. `lfw init --plugin` creates a single standard Cargo crate that
can expose a workflow from `src/lib.rs`. `lfw new --global` creates a workflow
crate in the default global collection; `lfw add --global` writes dependencies
to the default global collection's `Cargo.toml`. Those global path
dependencies are discovered from the global collection manifest, so a workflow
installed with `lfw add --global --path ...` can be used from any project that
uses the same XDG data directory or `LFW_PATH`.

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
lfw publish lightflow.std --apply
```

`--apply` runs `cargo publish --manifest-path ...`. Without `--apply`, no
network publish is attempted and the generated command includes `--dry-run`.

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
