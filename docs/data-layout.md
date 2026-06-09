# Data Layout

LightFlow separates engine source, built-in assets, project assets, and XDG-managed local state.

The short rule:

- `src/` contains the LightFlow engine.
- `src/builtins/` may contain built-in self-contained assets shipped with LightFlow.
- `lightflow/` contains versioned project assets.
- model, node, composition, and workflow assets are self-contained `.rs` files.
- asset metadata lives in the same `.rs` file as the asset definition.
- runtime state, generated indexes, sockets, locks, secrets, and caches use XDG Base Directory paths.
- external model caches and heavyweight weights stay outside the repository unless a project explicitly vendors a small test fixture.

## Platform Target

This layout is Linux-first. It assumes Linux filesystem conventions, XDG Base Directory paths, Unix sockets, file permissions, process ownership, and server deployment.

Non-Linux systems are expected to access LightFlow through the Linux-hosted HTTP/OpenAPI surface. They are not initial local-runtime targets.

## Repository Layout

```text
src/
  api.rs             # OpenAPI-facing backend boundary
  nodes.rs           # engine support for node assets
  compositions.rs    # engine support for composition assets
  workflows.rs       # engine support for workflow assets
  builtins/          # built-in self-contained assets shipped with LightFlow
lightflow/
  models/            # self-contained Rust model assets
  nodes/             # self-contained Rust node assets
  compositions/      # self-contained Rust composition assets
  workflows/         # self-contained Rust workflow assets
  assets/            # small committed project assets and fixture manifests
  presets/           # named parameter sets for workflows and compositions
  policies/          # routing, sandbox, resource, and approval policy data
  runs/              # committed run schemas and examples, not real run output
cortexfs/            # CortexFS submodule
```

## CortexFS Mount

LightFlow requires CortexFS for the Linux execution path.

The default global mount point is:

```text
/ctx
```

Use the current Linux user entry for runtime execution:

```bash
export CTX_HOME="/ctx/home/$(id -u)"
```

The LightFlow runtime should use CortexFS paths under `/ctx` for model discovery, provider routing, API-format calls, tool/MCP calls, structured jobs, externally triggered hooks, runtime channel configuration, thread state, policy, and audit.

Detailed integration rules live in [cortexfs-integration.md](cortexfs-integration.md).

## XDG Runtime Layout

LightFlow follows the XDG Base Directory Specification for user-local data:

```text
$XDG_CONFIG_HOME/lightflow/      # user config, defaults to ~/.config/lightflow
$XDG_STATE_HOME/lightflow/       # runs, logs, traces, history, defaults to ~/.local/state/lightflow
$XDG_CACHE_HOME/lightflow/       # generated indexes, probes, thumbnails, defaults to ~/.cache/lightflow
$XDG_RUNTIME_DIR/lightflow/      # sockets, locks, pid files, short-lived runtime files
```

If an XDG variable is unset, use the standard fallback path. If `$XDG_RUNTIME_DIR` is unset, LightFlow may fall back to a private directory under the system temporary directory with mode `0700`.

Suggested runtime shape:

```text
$XDG_CONFIG_HOME/lightflow/
  config.toml
  secrets/             # local credentials or pointers to secret providers
$XDG_STATE_HOME/lightflow/
  runs/<run_id>/       # run records, outputs, logs, traces, and temp files
  events.jsonl
$XDG_CACHE_HOME/lightflow/
  assets/index.json    # generated asset index, not source of truth
  models/              # resolved descriptors and probe results
$XDG_RUNTIME_DIR/lightflow/
  api.sock
  locks/
  pid
```

The repository-local `.lightflow/` directory is not the default. It is only allowed as an explicit Linux test or isolated sandbox override.

## Ownership Rules

### Built-ins

Built-in nodes, compositions, and workflows are allowed when they are part of LightFlow itself.

They live under `src/builtins/` and follow the same self-contained asset rule as project assets: metadata and definition stay in one `.rs` file.

Use built-ins for standard primitives, examples that must always be available, compatibility shims, and engine-owned reference workflows. Do not put user/project workflows under `src/builtins/`.

### Nodes

A node is one self-contained asset file:

- `lightflow/nodes/<node_id>.rs`
- or `src/builtins/nodes/<node_id>.rs` for built-in nodes shipped with LightFlow

The file owns both behavior and metadata: inputs, outputs, validation, execution, title, description, categories, default UI hints, required model capabilities, and stability level.

Nodes must not encode project-specific model paths directly. They ask for capabilities or model aliases, and the workflow resolves those through model assets and the runtime resolver.

### Compositions

A composition is one self-contained asset file:

- `lightflow/compositions/<composition_id>.rs`
- or `src/builtins/compositions/<composition_id>.rs` for built-in compositions shipped with LightFlow

The file owns both executable composition logic and metadata: title, description, included node IDs, default presets, expected resource shape, and stability level.

Compositions are reusable patterns made from nodes or other compositions. They should express common pipeline structure without becoming hidden runtime magic. A composition can define default parameters, but a workflow may override them.

### Workflows

A workflow is one self-contained asset file:

- `lightflow/workflows/<workflow_id>.rs`
- or `src/builtins/workflows/<workflow_id>.rs` for built-in workflows shipped with LightFlow

The file owns both the actual pipeline and the public contract: user-facing name, input schema pointer, output schema pointer, expected resources, default presets, and which model aliases it requires.

### Models

Model data is split into three layers:

- `lightflow/models/*.rs` describes model aliases, providers, capabilities, expected artifact names, quantization, license notes, and source hints.
- `$XDG_CACHE_HOME/lightflow/models/` may hold local resolved descriptors and probe results.
- actual heavyweight weights live in an external cache such as Hugging Face cache, a user-configured model directory, or a provider-managed store.

The repository should commit model assets, not large weights. A model asset may point to a small fixture model only when that fixture is required for tests.

Workflows depend on model aliases, not raw file paths. Example aliases:

- `image.base`
- `image.refiner`
- `llm.planner`
- `embedding.text`

At runtime, model aliases resolve through CortexFS user-visible model and route views under `/ctx/home/<uid>/model/` and `/ctx/home/<uid>/route/`.

### Runs

Real run output is local runtime state:

- `$XDG_STATE_HOME/lightflow/runs/<run_id>/manifest.json`
- `$XDG_STATE_HOME/lightflow/runs/<run_id>/request.json`
- `$XDG_STATE_HOME/lightflow/runs/<run_id>/resolved_workflow.json`
- `$XDG_STATE_HOME/lightflow/runs/<run_id>/events.jsonl`
- `$XDG_STATE_HOME/lightflow/runs/<run_id>/outputs/`
- `$XDG_STATE_HOME/lightflow/runs/<run_id>/trace.jsonl`

`manifest.json` is the stable LightFlow-owned summary of workflow structure and CortexFS correlation paths. It includes planned request, response, error, fingerprint, and route metadata paths for each inbox-backed CortexFS step. CortexFS structured jobs use `home/<uid>/job/<id>/{spec,req,out.json,status}`, hooks use `home/<uid>/hook/<id>/{trigger,spec,req,out.json,status,last,log.jsonl}`, and runtime channels use `/ctx/chan/<id>`; LightFlow exposes those path records without duplicating CortexFS provider configuration.

`request.json` preserves the create-run request. `resolved_workflow.json` preserves the parsed workflow definition used for planning. `events.jsonl` is the append-only user-facing run event stream. `trace.jsonl` is the append-only technical trace for CortexFS file commits and related plumbing.

Creating a run reserves the whole `$XDG_STATE_HOME/lightflow/runs/<run_id>/` directory. A later create with the same run id is rejected even if `manifest.json` is missing, so a partial or crashed run directory is not silently overwritten. Listing runs only returns directories with readable manifests and ignores incomplete run directories.

`POST /runs/preview` and `lightflow run preview` use the same planner and template renderer as create/submit, but they do not create this directory or write any run state.

Committed examples belong under `lightflow/runs/examples/` or docs. Do not commit real user outputs, logs, secrets, or bulky generated artifacts.

## Asset File Shape

Every project-owned model, node, composition, and workflow asset is a Rust file with the same basic shape:

```rust
use lightflow::asset::*;

pub const META: AssetMeta = AssetMeta {
    id: "example.asset",
    title: "Example Asset",
    kind: AssetKind::Workflow,
    description: "Human-readable purpose.",
    stability: Stability::Draft,
};

pub fn define() -> WorkflowDef {
    workflow("example.asset")
        .input_schema("schemas/example.input.json")
        .output_schema("schemas/example.output.json")
        .required_model("llm.planner")
        .api_step("draft", "node.llm_prompt", "openai.chat")
        .openai_chat_input("llm.planner", "prompt")
}
```

Exact helper APIs can evolve, but the invariant is stable: metadata and definition live in one `.rs` asset file.

The current discovery implementation reads a top-level `pub const META: AssetMeta = AssetMeta { ... };` with literal string fields and direct enum variants. `AssetMeta` is const-friendly for Rust asset files; discovery converts it into an owned API record. Workflow planning also reads a simple `define()` builder chain and turns CortexFS-backed steps into run manifest entries. A step may declare a request template such as `openai_chat_input("llm.planner", "prompt")`; LightFlow then renders the CortexFS request from run inputs when no explicit submit body is supplied. Keep metadata and definitions explicit so agents and reviewers can inspect an asset without executing code.

The backend may build a generated index under `$XDG_CACHE_HOME/lightflow/` for speed. Generated indexes are runtime artifacts and are not source of truth.

## API Mapping

The OpenAPI surface should read built-in and project assets through the asset loader and runtime state:

- `GET /ctx/abi` returns the CortexFS userspace/FUSE ABI and kernel policy for this LightFlow process.
- `GET /workflows` scans and validates `src/builtins/workflows/*.rs` and `lightflow/workflows/*.rs` through `src/workflows.rs`.
- `GET /nodes` scans and validates `src/builtins/nodes/*.rs` and `lightflow/nodes/*.rs` through `src/nodes.rs`.
- `GET /compositions` scans and validates `src/builtins/compositions/*.rs` and `lightflow/compositions/*.rs` through `src/compositions.rs`.
- `GET /models` scans and validates `lightflow/models/*.rs` through `src/models.rs`.
- `POST /runs/preview` resolves the workflow, validates references, plans CortexFS paths, and renders request templates without writing state.
- `GET /runs` lists readable manifests under `$XDG_STATE_HOME/lightflow/runs/`, sorted by run id.
- `POST /runs` creates `$XDG_STATE_HOME/lightflow/runs/<run_id>/` and rejects existing run directories with `409 Conflict`.
- `GET /runs/{run_id}` reads `$XDG_STATE_HOME/lightflow/runs/<run_id>/`.
- `GET /runs/{run_id}/status` returns a derived lifecycle summary and step counts from the manifest.
- `GET /runs/{run_id}/request` reads the original create-run request stored in `request.json`.
- `GET /runs/{run_id}/workflow` reads the resolved workflow definition stored in `resolved_workflow.json`.
- `POST /runs/{run_id}/steps/{step_id}/submit` writes a CortexFS request only for planned steps; duplicate submitted, succeeded, or failed steps return `409 Conflict`.
- `POST /runs/{run_id}/refresh` refreshes a run manifest from CortexFS outboxes and returns `404 Not Found` for missing run manifests.
- `GET /runs/{run_id}/events` and `GET /runs/{run_id}/trace` read JSONL streams only for existing run manifests.

The API should expose inspectable project state without requiring clients to understand Rust internals. Clients call the backend; the backend owns Rust asset parsing, validation, and execution.

The current Rust API service is framework-independent. It lists assets through the discovery modules, parses workflow definitions, validates referenced model/node/composition assets before writing run records, plans CortexFS-backed run steps, submits step request bodies through CortexFS atomic file semantics, refreshes run state from CortexFS outboxes, and creates or reads run manifests through the XDG run store. CLI, HTTP, MCP, and stream adapters should call that service rather than reimplementing asset or run file access.

Workflow execution uses CortexFS under `/ctx` for provider/model/tool/thread/audit operations. LightFlow's run store records workflow structure and CortexFS correlation fields; CortexFS remains the authoritative Linux execution and audit substrate.

## Commit Policy

Commit:

- Rust source definitions
- self-contained Rust asset files
- small fixtures
- schema examples
- docs

Do not commit:

- XDG config/state/cache/runtime directories
- model weights
- real run outputs
- local credentials
- provider API keys
- generated caches
