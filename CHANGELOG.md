# Changelog

## Unreleased

### CLI

- Validate run ids used by `lfw trace`, `lfw runs get`, `lfw runs rm`,
  `lfw replay`, and `lfw batch resume` as single path segments.
- Validate `lfw batch run --run-id` before writing batch state.
- Keep reusable graph patch registry names constrained to a single file name.

### API

- Reject workflow execution when recursive dependency validation reports missing
  workflows, dependency cycles, or version mismatches.
- Return structured HTTP error objects with `error`, `code`, `message`, and
  `status` fields.
- Verify OpenAPI path parity and live endpoint response required fields against
  the OpenAPI component schemas in server tests.
- Keep `/nodes`, `/models`, `/runs`, `/runs/{run_id}`,
  `/runs/{run_id}/events`, and `/artifacts` aligned with the editor-facing
  backend contract.

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
