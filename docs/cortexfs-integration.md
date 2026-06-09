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

## Kernel Boundary

CortexFS is a userspace/FUSE execution substrate for LightFlow. The `/ctx` tree is a CortexFS userspace ABI, not a Linux kernel ABI.

LightFlow should never require the Linux kernel tree to understand AI workflow graphs, provider routing, model aliases, MCP tools, HTTP/OpenAPI, WebTransport, FlatBuffers stream metadata, JSON request bodies, hooks, or LightFlow run records.

The boundary is queryable as structured data:

```bash
lightflow ctx abi
curl http://127.0.0.1:5174/ctx/abi
```

Kernel upstream work, if any, must be split out as a generic primitive with a reproducer, benchmark, tests, and maintainer-specific patch series. See [kernel-policy.md](kernel-policy.md).

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

Structured job:
  /ctx/home/<uid>/job/<job_id>/spec
  /ctx/home/<uid>/job/<job_id>/req
  /ctx/home/<uid>/job/<job_id>/out.json
  /ctx/home/<uid>/job/<job_id>/status

CortexFS hook:
  /ctx/home/<uid>/hook/<hook_id>/trigger
  /ctx/home/<uid>/hook/<hook_id>/spec
  /ctx/home/<uid>/hook/<hook_id>/req
  /ctx/home/<uid>/hook/<hook_id>/out.json
  /ctx/home/<uid>/hook/<hook_id>/status
  /ctx/home/<uid>/hook/<hook_id>/last
  /ctx/home/<uid>/hook/<hook_id>/log.jsonl

Runtime channel:
  /ctx/chan/<channel_id>/url
  /ctx/chan/<channel_id>/keyref
  /ctx/chan/<channel_id>/fmt
  /ctx/chan/<channel_id>/mod
  /ctx/chan/<channel_id>/enabled
  /ctx/chan/<channel_id>/status
  /ctx/chan/<channel_id>/localurl

Audit:
  /ctx/audit/events.jsonl
```

API, tool, and thread request submission must use CortexFS atomic file semantics:

1. Write a temporary request file.
2. Rename it into `inbox/*.req.json`.
3. Read `outbox/*.resp.json` or `outbox/*.error`.
4. Store the CortexFS fingerprint and route metadata in the LightFlow run record.

Plain writes must not trigger provider execution for inbox-backed requests. Structured jobs use the CortexFS job ABI instead: write `spec`, then write `req`, then read `out.json` and `status`. CortexFS hooks expose externally triggered work under `home/<uid>/hook/<id>`; schedulers such as systemd timers, cron, Git hooks, or CI own the trigger event and write `req`, while CortexFS owns the hook files, status, log, and audit surface. Runtime channels use the file channel ABI under `/ctx/chan/<id>` for local provider endpoint configuration; `keyref` stores a secret reference such as an environment-variable reference, not secret material.

`src/cortex.rs` provides the minimal file helper for these contracts: it writes `*.tmp`, syncs the file, renames it to `*.req.json`, reads optional response, error, fingerprint, and route metadata files, and exposes structured job, hook, plus runtime channel path records. It does not call providers directly.

The CLI exposes that path through:

```bash
lightflow run submit <run_id> <step_id>
lightflow run submit <run_id> <step_id> '<json_request_body>'
lightflow run refresh <run_id>
lightflow ctx chan <channel_id>
lightflow ctx job <job_id>
lightflow ctx hook <hook_id>
```

`run submit` commits the request into CortexFS. If the workflow step declares a request template, the body can be omitted and LightFlow renders CortexFS-native JSON from the original run inputs. If a body is supplied, LightFlow commits it as-is. `run refresh` reads CortexFS outboxes and updates the LightFlow manifest status, fingerprint, route fields, and response/error paths.

Each submit and refresh also appends plain JSON lines under the run directory:

```text
$XDG_STATE_HOME/lightflow/runs/<run_id>/events.jsonl
$XDG_STATE_HOME/lightflow/runs/<run_id>/trace.jsonl
```

Submit records `cortex.request.committed` in the technical trace. Refresh records
`cortex.response.observed` or `cortex.error.observed` when it first observes a
CortexFS outbox outcome for a step.

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
