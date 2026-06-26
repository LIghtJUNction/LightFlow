# Changelog

## Unreleased

### CLI

- Validate run ids used by `lfw trace`, `lfw runs get`, `lfw runs rm`,
  `lfw replay`, and `lfw batch resume` as single path segments.
- Validate `lfw batch run --run-id` before writing batch state.
- Keep reusable graph patch registry names constrained to a single file name.
- Add `lfw plan <workflow_id>` and `lfw workflows plan <workflow_id>` to inspect
  selected executor, data-policy, atom, and model plans without running.

### API

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
