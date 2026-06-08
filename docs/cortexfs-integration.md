# CortexFS Integration

LightFlow deeply integrates CortexFS. CortexFS is not an optional backend in this project.

LightFlow owns pipeline authorship: project assets, built-in assets, workflow composition, run planning, and the HTTP/OpenAPI surface.

CortexFS owns the Linux execution substrate: AI API formats, providers, model visibility, tool/MCP invocation, policy, audit, thread state, memory surfaces, and filesystem-native control paths.

## Repository Integration

CortexFS is vendored as a Git submodule:

```text
cortexfs/
```

The submodule is the source for the CortexFS ABI and implementation used by LightFlow. Do not duplicate CortexFS path semantics in LightFlow docs or code when they can be referenced from the submodule.

## Global Mount Point

The default global CortexFS mount point is:

```text
/ctx
```

LightFlow assumes a Linux host can expose CortexFS at `/ctx`. The current user entry is:

```bash
export CTX_HOME="/ctx/home/$(id -u)"
```

LightFlow should treat `/ctx` as the canonical local execution surface for provider/model/tool/thread/audit access.

## Responsibility Split

LightFlow:

- scans and validates self-contained Rust assets
- resolves workflows, nodes, compositions, presets, and project model aliases
- plans workflow runs
- exposes the HTTP/OpenAPI control surface
- writes LightFlow run records under `$XDG_STATE_HOME/lightflow/runs/`
- references CortexFS request ids, fingerprints, routes, and audit entries

CortexFS:

- exposes AI API formats such as `openai.chat`, `openai.responses`, `anthropic.messages`, and `google.generate_content`
- exposes provider and model views
- enforces user/space policy
- handles provider routing and secret resolution
- invokes tools and MCP tools
- records route-aware audit events
- owns thread/message projections
- provides Unix-native file submission semantics

## Runtime Mapping

LightFlow workflow steps map to CortexFS paths:

```text
AI model call:
  /ctx/home/<uid>/api/<format>/inbox/<step_id>.req.json
  /ctx/home/<uid>/api/<format>/outbox/<step_id>.resp.json
  /ctx/home/<uid>/api/<format>/outbox/<step_id>.error

Tool call:
  /ctx/tool/<tool_id>/invoke/inbox/<step_id>.req.json
  /ctx/tool/<tool_id>/invoke/outbox/<step_id>.resp.json

Thread message:
  /ctx/home/<uid>/thread/<thread_id>/inbox/<step_id>.req.json

Audit:
  /ctx/audit/events.jsonl
```

Submission must use CortexFS atomic file semantics:

1. Write a temporary request file.
2. Rename it into `inbox/*.req.json`.
3. Read `outbox/*.resp.json` or `outbox/*.error`.
4. Store the CortexFS fingerprint and route metadata in the LightFlow run record.

Plain writes must not trigger provider execution.

`src/cortex.rs` provides the minimal file helper for this contract: it writes `*.tmp`, syncs the file, renames it to `*.req.json`, and then reads optional response, error, fingerprint, and route metadata files. It does not call providers directly.

The CLI exposes that path through:

```bash
lightflow run submit <run_id> <step_id>
lightflow run submit <run_id> <step_id> '<json_request_body>'
lightflow run refresh <run_id>
```

`run submit` commits the request into CortexFS. If the workflow step declares a request template, the body can be omitted and LightFlow renders CortexFS-native JSON from the original run inputs. If a body is supplied, LightFlow commits it as-is. `run refresh` reads CortexFS outboxes and updates the LightFlow manifest status, fingerprint, route fields, and response/error paths.

Each submit and refresh also appends plain JSON lines under the run directory:

```text
$XDG_STATE_HOME/lightflow/runs/<run_id>/events.jsonl
$XDG_STATE_HOME/lightflow/runs/<run_id>/trace.jsonl
```

These files are intentionally ordinary text streams for `tail`, `jq`, and shell tooling.

## Model Resolution

LightFlow model assets define project aliases such as:

```text
llm.planner
image.base
embedding.text
```

At runtime those aliases resolve through CortexFS user-visible model and route views, not through hardcoded local model paths.

Example lookup surfaces:

```text
/ctx/home/<uid>/model/
/ctx/home/<uid>/route/<format>/
/ctx/provider/<provider_id>/model/
/ctx/model/
```

The user-visible CortexFS model view is authoritative for what the current Linux user can actually use.

## Run Records

LightFlow still owns its run record because it knows workflow structure. A run record should include:

- workflow asset id
- node/composition step ids
- CortexFS format
- CortexFS request id
- provider id
- model id
- route decision
- fingerprint
- response or error path
- output artifact paths
- audit correlation fields

The run record belongs under:

```text
$XDG_STATE_HOME/lightflow/runs/<run_id>/
```

The initial manifest file is:

```text
$XDG_STATE_HOME/lightflow/runs/<run_id>/manifest.json
```

LightFlow writes the manifest through a temporary file in the run directory and renames it into place. The run store also creates:

```text
$XDG_STATE_HOME/lightflow/runs/<run_id>/outputs/
```

CortexFS audit remains the authoritative cross-system audit stream.

## HTTP/OpenAPI Role

HTTP/OpenAPI is the network control surface for LightFlow. It does not replace CortexFS.

Local Linux execution should use `/ctx` for model/tool/provider/thread/audit operations. Non-Linux clients can call the LightFlow server over HTTP/OpenAPI, and the server then performs Linux-local execution through CortexFS.

## Design Rule

If a concept already exists in CortexFS, LightFlow should integrate with it instead of creating a parallel implementation.

Create LightFlow-specific state only when it represents workflow authorship or workflow run structure that CortexFS does not own.
