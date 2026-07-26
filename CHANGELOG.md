# Changelog

## Unreleased

### Workflow DSL

- Expand `workflow!` so composite graphs can declare `name`, `description`,
  `category`, nested `node id: "workflow_id"`, optional version pins with
  `@ "x.y.z"`, and `edge from.port -> to.port` in one block.
- Register nested workflow dependencies automatically from `node` / `if_node`
  declarations; omit an explicit version so the nested package version follows
  the installed catalog. Use `@ "x.y.z"` or `.node_version(...)` only when
  pinning is required.
- Add `.wire("from.port", "to.port")` for dotted endpoint edges on the builder
  chain. Legacy `.depends_on(...)` / `.node(...)` / `.edge(...)` remain
  supported and merge dependency entries by workflow id.

### Breaking

- Remove the separate `lightflow-macros` crate. Declarative `workflow!` helpers
  now live in `src/macros.rs` inside the main `lightflow` package. The typed
  attribute macros (`#[node]`, `#[typed_workflow]`, `#[subworkflow]`,
  `#[trace_node]`, `#[retry]`, `#[timeout]`) and `WorkflowInput` /
  `WorkflowOutput` derives are removed; use `run_node` / `Runnable` directly.

### Runtime

- Add the explicit `lightflow.command.run` / `process.command.v1` executor for
  versioned JSON process integrations. The runner is invoked directly without
  a shell, with bounded output, strict output/artifact validation, descendant
  termination on timeout, and structured replay fingerprints.

### Workflows

- Make `projects/lightflow-auto-editing` executable end to end: deterministic
  media planning, real FFmpeg rendering, audio preservation, silence removal,
  visual scene detection, bounded segment pacing, atomic output, source-media
  replay hashes, and colocated agent skills for planning, rendering, and
  one-shot automatic edits.

### Fixed

- Forward only a selected branch's declared inputs to `if_node` child
  workflows. The condition port and the other branch's ports are no longer
  passed through, so strict child input validation accepts conditional runs.
- Pin `LC_ALL=C` on machine-parsed git invocations in the local workflow loop
  so project workspace checks behave the same under non-English locales.
- Keep the ComfyUI contract's `workflow_path` guidance when strict input
  validation rejects the reserved input before execution.
- Run package runners without a synced `lfw.lock`: unsynced model
  requirements stay unbound and the runner decides whether it can execute,
  so preview workflows work on fresh checkouts while model-backed runners
  still fail closed. Synced lock entries remain strictly verified.
- Accept `null` for declared runner outputs in the `runner.v1` response
  contract; required outputs are still enforced by workflow-level port
  validation. `lightflow.model_lock_check` now reports an unlocked
  requirement's `path` as null instead of an invalid empty path string.
- Chain only the declared inputs of the next stage in `lfw run a | b`
  pipelines so strict input validation accepts piped stages; explicit
  `--input` values still pass through unchanged.
- Drain runner output beyond the stdout/stderr caps so an over-limit child
  process exits and fails fast with the limit error instead of blocking on a
  full pipe until the execution timeout kills it.

### Quality

- Add code CI for source-shape, Rust formatting/tests/Clippy, real FFmpeg
  integration tests, workflow crate checks across the rig and flux projects,
  and all automatic-edit node contracts.

## 0.1.4 - 2026-07-11

### CLI

- Flatten project, global, imported, and linked workflow collections to
  `workflows/<crate>` and remove the required `lfw new --category` argument.
- Add `lfw migrate [path]` with collision preflight, precise Cargo workspace
  member updates, rollback on failure, and idempotent legacy-layout migration.
- Validate run ids used by `lfw trace`, `lfw runs get`, `lfw runs rm`,
  `lfw replay`, and `lfw batch resume` as single path segments.
- Validate `lfw batch run --run-id` before writing batch state.
- Keep reusable graph patch registry names constrained to a single file name.
- Add `lfw plan <workflow_id>` and `lfw workflows plan <workflow_id>` to inspect
  selected executor, data-policy, atom, and model plans without running.

### API

- Persist optional workflow categories as DSL metadata so category filtering is
  independent from the on-disk crate layout.
- Reject workflow execution when recursive dependency validation reports missing
  workflows, dependency cycles, or version mismatches.
- Return structured HTTP error objects with `error`, `code`, `message`, and
  `status` fields.
- Verify OpenAPI path parity and live endpoint response required fields against
  the OpenAPI component schemas in server tests.
- Keep `/nodes`, `/executors`, `/models`, `/runs`, `/runs/{run_id}`,
  `/runs/{run_id}/events`, and `/artifacts` aligned with the editor-facing
  backend contract.
- Include selected runtime metadata in workflow execution and node execution
  records for trace, replay, HTTP, MCP, and editor clients.
- Include selected runtime metadata on completed node trace events so timeline
  clients can explain executor choice from `/runs/{run_id}/events`.
- Include replay runtime and model-lock comparison reports so clients can see
  whether selected runtime fingerprints or locked model choices changed during
  replay.
- Include executor status labels, availability reasons, data policies, and
  model-planning flags in `lfw info`, `/executors`, MCP executor tools, and
  node runtime cards.
- Add `GET /workflows/{workflow_id}/plan` and MCP
  `lightflow.workflow.plan` so API clients can inspect workflow runtime plans
  without creating run history.
- Add `GET /openapi.yaml` and MCP `lightflow://openapi` so clients can discover
  the HTTP contract from a running backend or MCP resource list.
- Add `DELETE /runs/{run_id}` and MCP `lightflow.run.rm` so HTTP, MCP, and
  CLI clients can manage the same project-local run history.
- Add `GET /release`, MCP `lightflow.release.check`, and
  `lightflow://release` so clients can inspect release readiness without
  executing gate commands.
- Include project workspace config diagnostics in project catalogs and
  dev/release reports with `project_config_valid`, `project_config_error`, and
  repair commands so CLI, HTTP, MCP, and editor clients can guide fixes without
  parsing fatal errors.
- Include a non-mutating source-change review gate in release readiness reports
  so unsafe workflow edits are visible before `--apply`.
- Add an explicit release gate for repository workflow agent skills with CLI
  and API usage examples.

### Editor

- Show source workflow graph nodes and edges from `/workflows/{workflow_id}` in
  LightFlowUI node detail without introducing a frontend graph format.
- Show `/workflows/{workflow_id}/plan` runtime details in LightFlowUI node
  detail so users can inspect executor, data-policy, atom, and model choices
  before running.
- Show node trace rows, runtime badges, artifact counts, and replay drift in
  LightFlowUI run detail without requiring users to read raw trace JSON.
- Let LightFlowUI delete recorded runs through the HTTP run-history contract.
- Expand LightFlowUI model catalog columns so lock status, variants, formats,
  hashes, local paths, and missing paths are visible from `/models`.
- Show project and selected-workflow `/release` gate planning in LightFlowUI
  alongside local loop, source-change, and publish readiness.

### Workflows

- Keep the standard workflow catalog as Rust library crates with colocated
  agent skills and Node Schema metadata.
- Keep preview image, mock LLM, text/JSON, image, mask, model, and control
  helpers runnable through builtin executor contracts.

### Runtime

- Document executor status labels for preview, mock, external, native, and
  reserved runtime paths.
- Add `docs/runtime-verification.md` with verified commands for preview/mock,
  RIG, external FLUX runner, and native FLUX build checks.
- Keep `LIGHTFLOW_FLUX_RUNNER` as the external FLUX handoff contract.
- Keep `--features rig` as the feature gate for provider-backed RIG execution,
  with deterministic mock-provider and local OpenAI-compatible coverage for
  verification.
- Record selected executor id, executor kind, capabilities, data policy, and
  declared runtime requirements in execution traces when a leaf runtime is
  selected.
- Validate FLUX locked model paths and expected formats before handing work to
  native or external runners, with `lfw sync` remediation in runtime errors.

### Known Limitations

- Preview and mock executors remain deterministic plumbing checks; they do not
  prove production model quality.
- Native FLUX support is feature-gated and depends on local C/C++ build tools
  and platform libraries documented in `docs/runtime-verification.md`.
- Graph composition in the static editor is intentionally deferred until the
  backend graph contract can round-trip safely.

### Migration Notes

- HTTP, MCP, and CLI workflow runs now write project-local history under
  `.lightflow/runs`; existing projects can delete that directory if they do not
  want to keep local traces.
- Reusable patches live under `.lightflow/patches/<name>.json`; run manifests
  store expanded patch data, so replay does not depend on later registry
  edits.
- `lfw.lock` model entries now drive `/models` lock status. Projects without a
  lockfile report missing-lock status until `lfw sync --apply` writes locked
  choices.
