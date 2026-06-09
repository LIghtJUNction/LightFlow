# LightFlow

Lightweight AI Pipelines. Built by Agent, Directed by You.

LightFlow is a backend-first alternative to ComfyUI. Instead of asking users to build AI workflows by dragging boxes around a canvas, LightFlow treats workflows as code that agents can read, write, review, and evolve.

ComfyUI proved the value of direct, visual feedback. Humans can look at an output, move a slider, change a seed, adjust a prompt, and immediately tell whether the result improved. That loop is fast, natural, and still hard for agents to match.

But the same human-first canvas becomes expensive when the task is building the workflow itself. Complex node graphs and long chains of links often translate into ordinary control flow, typed inputs, function calls, and reusable modules. What looks visually complicated can be much simpler as code.

LightFlow is built for that split:

- agents handle the setup work: reading docs, installing models, wiring nodes, fixing shape mismatches, and assembling workflows as Rust code
- humans keep the high-value feedback loop: judging outputs, changing intent, tuning parameters, and directing revisions

You should not need to buy an expensive workflow or search through layers of menus just to get started. Give an agent the docs, skills, and requirements; let it build the backend workflow; then use the generated API surface to run, inspect, and refine it.

## Positioning

LightFlow is for AI pipeline authorship where:

- a node is a Rust file
- a composition is a Rust file
- a workflow is a Rust file
- pipeline files are project assets, not engine source files
- each asset file is self-contained: metadata and definition live together
- agents generate and modify pipeline code directly
- humans direct intent, constraints, and review
- the backend exposes an OpenAPI-compatible surface

LightFlow is Linux-first. The primary target is a Linux workstation or Linux server with local filesystem conventions, Unix sockets, process isolation, and server deployment as first-class assumptions. Other systems can use a LightFlow server over HTTP/OpenAPI; they do not shape the core runtime design.

LightFlow deeply integrates [CortexFS](cortexfs/) as its Linux execution substrate. CortexFS is vendored as a submodule, and `/ctx` is the default global CortexFS mount point for model, provider, tool, thread, policy, and audit access.

LightFlow is not a Linux kernel subsystem candidate. CortexFS is a userspace/FUSE boundary; kernel work must be a small generic primitive, not LightFlow or `/ctx`. See [docs/kernel-policy.md](docs/kernel-policy.md).

The frontend is intentionally out of scope for now.

## Scope

Current scope:

- Rust backend project foundation
- node / composition / workflow asset layout
- framework-independent API service plus CLI and HTTP adapters
- OpenAPI-first backend contract for asset discovery and run records
- Linux-first runtime layout and deployment assumptions
- CortexFS submodule integration and `/ctx` mount-point convention
- runtime stream discovery and FlatBuffers snapshot framing

Not in scope yet:

- embedded agent loop or visual workflow editor
- node runtime implementation beyond CortexFS request planning
- workflow scheduler
- model provider integrations
- persistence layer
- auth / permissions
- non-Linux local runtime support

## Project Shape

```text
src/
  api.rs           # OpenAPI-facing backend boundary
  cortex.rs        # CortexFS path planning and request exchange records
  runs.rs          # XDG-backed LightFlow run manifests
  asset.rs         # Self-contained Rust asset metadata discovery
  models.rs        # Engine support for model assets
  nodes.rs         # Engine support for node assets
  compositions.rs  # Engine support for composition assets
  workflows.rs     # Engine support for workflow assets
  builtins/        # Built-in self-contained node/composition/workflow assets
lightflow/
  models/          # Self-contained Rust model assets, not heavyweight weights
  nodes/           # Self-contained Rust node assets
  compositions/    # Self-contained Rust reusable composition assets
  workflows/       # Self-contained Rust executable workflow assets
  presets/         # Named parameter sets
  policies/        # Routing, sandbox, resource, and approval policy data
  runs/            # Committed schemas/examples only
cortexfs/          # CortexFS submodule, Linux execution substrate
docs/
  architecture.md  # Product and architecture intent
  data-layout.md   # Project data and runtime state rules
  cortexfs-integration.md
openapi/
  lightflow.yaml   # API contract for runs and CortexFS exchange records
```

## Data Ownership

LightFlow keeps engine source, built-in assets, and project assets separate:

- engine/runtime source lives in `src/`
- built-in nodes, compositions, and workflows may live under `src/builtins/` when they ship with LightFlow itself
- model assets live in `lightflow/models/*.rs`
- node assets live in `lightflow/nodes/*.rs`
- composition assets live in `lightflow/compositions/*.rs`
- workflow assets live in `lightflow/workflows/*.rs`
- every built-in or project asset file contains its own metadata and definition; metadata is not split into sidecar JSON
- CortexFS is the required Linux execution substrate, exposed at `/ctx`
- the API boundary is a framework-independent service first; CLI, HTTP, MCP, and stream adapters call that service instead of owning behavior
- real run output, caches, traces, sockets, locks, and local state use XDG Base Directory paths and are not committed

See [docs/data-layout.md](docs/data-layout.md) for the full layout and commit policy.
See [docs/cortexfs-integration.md](docs/cortexfs-integration.md) for the CortexFS integration contract.

## CLI

The binary is a small Linux-friendly JSON command surface:

```bash
cargo run -- assets workflows
cargo run -- run preview workflow.text_plan --id run-001 --inputs '{"prompt":"Write a migration plan"}'
cargo run -- run create workflow.text_plan --id run-001 --inputs '{"prompt":"Write a migration plan"}'
cargo run -- run list
cargo run -- run get run-001
cargo run -- run status run-001
cargo run -- run request run-001
cargo run -- run workflow run-001
cargo run -- run submit run-001 draft
cargo run -- run submit run-001 draft '{"model":"demo","messages":[]}'
cargo run -- run submit run-001 draft @request.json
jq -n '{model:"demo",messages:[]}' | cargo run -- run submit run-001 draft -
cargo run -- run refresh run-001
cargo run -- run events run-001
cargo run -- run trace run-001
cargo run -- ctx abi
cargo run -- ctx api openai.chat step-001
cargo run -- ctx chan fengying
cargo run -- ctx job translate.zh
cargo run -- ctx hook daily-translate
cargo run -- serve --port 5174
cargo run -- stream info
cargo run -- stream snapshot run-001 > run-001.fb
```

It does not start a background service. It lists Rust assets, parses workflow definitions, writes XDG run manifests, commits CortexFS requests, exposes CortexFS channel/job/hook path records, and exposes event/trace JSONL for shell pipelines. If a workflow step declares a request template, `run submit <run> <step>` renders the CortexFS request from the stored run inputs; passing explicit JSON keeps full caller control.

The CLI fails fast on unexpected extra arguments, unknown flags, duplicate flags, and missing flag values so scripts do not accidentally run with ignored input.

For tests or sandboxes, `LIGHTFLOW_CTX_MOUNT` and `LIGHTFLOW_CTX_UID` override the default `/ctx/home/<current uid>` CortexFS path.

## HTTP Gateway

`cargo run -- serve --port 5174` starts an Axum gateway for the OpenAPI run surface, MCP endpoint, runtime stream discovery, and static `LightFlowUI/dist` files when present.

Examples:

```bash
curl http://127.0.0.1:5174/workflows
curl http://127.0.0.1:5174/ctx/abi
curl -X POST http://127.0.0.1:5174/runs/preview \
  -H 'content-type: application/json' \
  -d '{"workflow_asset_id":"workflow.text_plan","run_id":"run-001","inputs":{"prompt":"Write a migration plan"}}'
curl -X POST http://127.0.0.1:5174/runs/run-001/steps/draft/submit
curl http://127.0.0.1:5174/runs/run-001/events
```

## Design Principle

LightFlow should make the natural authoring path for AI agents be the same path engineers use: write Rust files, compose typed building blocks, and expose inspectable backend contracts.
