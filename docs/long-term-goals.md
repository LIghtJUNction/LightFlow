# LightFlow Long-Term Goals

This document describes the long-term direction for LightFlow after the v0.2
backend foundation. It is intentionally strategic: release checklists and
short-term task lists should stay in separate documents.

## North Star

LightFlow should become a code-first, agent-editable workflow runtime for
human-directed AI pipelines.

The core promise is:

- humans keep ownership of workflow intent, review, and execution decisions;
- agents can safely inspect, modify, test, and explain workflows because they
  are normal source-controlled Rust crates;
- CLI, HTTP, MCP, and future UI surfaces all project the same backend contract;
- model-backed runtimes stay behind explicit executor boundaries instead of
  leaking model files, tensors, or provider details into workflow graphs.

## Product Shape

LightFlow should remain backend-first. The backend contract, workflow crate
layout, runtime registry, model sync behavior, and run history model should
lead the product shape. UI work should start as a client of those contracts,
not as a separate workflow source of truth.

The long-term product should support three user modes:

- CLI-first development for engineers and agents.
- API/MCP integration for other tools and automation.
- A read/write editor for inspecting, running, tracing, and eventually
  composing workflows without replacing the Rust workflow source model.

## Architecture Principles

- Keep `workflow` as the primary domain concept until there is strong evidence
  for another top-level concept.
- Treat workflow crates as the durable package format.
- Keep standard workflows small, neutral, and reusable.
- Keep executor selection explicit through runtime capabilities and engine
  metadata.
- Keep large artifacts, model weights, and tensor payloads out of workflow JSON.
- Keep run history immutable enough for debugging, replay, and editor timeline
  views.
- Prefer contract tests over duplicated documentation when API behavior matters.

## Long-Term Tracks

### 1. Workflow Ecosystem

Build a useful catalog of reusable workflow crates, each with:

- Node Schema metadata for inputs, outputs, models, and artifacts.
- A colocated agent skill.
- `lfw node test` coverage.
- Clear examples for CLI and API use.

The ecosystem should make common image, text, LLM, model-management, and
control-flow tasks available without users writing custom Rust for every node.

### 2. Runtime Backends

Make runtime backends production-grade behind stable capability contracts:

- native FLUX and image runtimes for local model execution;
- RIG-backed LLM generation for provider-backed text execution;
- external runner contracts for experiments and non-Rust backends;
- reserved executor paths for command, Python, ONNX, and Candle only when their
  contracts are clear enough to test.

Runtime maturity should be visible through `lfw info`, `/nodes`, `/models`, and
documentation.

### 3. Model And Resource Management

LightFlow should make model requirements inspectable, syncable, and reproducible:

- declared model requirements live with workflow definitions;
- `lfw sync` writes locked choices and hashes;
- runtime execution uses locked paths directly;
- model approval, missing files, and incompatible formats fail with actionable
  errors.

The long-term goal is reproducible workflow execution across machines without
copying model weights into project directories.

### 4. Observability And Replay

Run history should become the shared debugging surface for CLI, API, MCP, and
editor clients:

- run manifests capture stable replay contracts;
- execution files capture outputs, artifacts, node attempts, durations, and
  selected runtimes;
- event streams support timeline views;
- failed runs are first-class records, not only terminal output.

Replay should stay deterministic where runtimes allow it and explicit about
runtime/model changes where they do not.

### 5. Editor Surface

The editor should grow in stages:

1. Read-only node catalog, node detail, model catalog, and run history.
2. Run forms generated from Node Schema metadata.
3. Trace and artifact inspection.
4. Patch authoring for temporary run modifications.
5. Graph composition only after the backend graph contract is stable enough to
   round-trip safely.

The editor must not become a separate hidden workflow format.

### 6. Agent Collaboration

LightFlow should be comfortable for coding agents:

- repository docs explain the domain model and file layout;
- every workflow/plugin includes an agent skill;
- generated projects include useful TODOs but also runnable tests;
- patches, traces, and node contracts are serializable and reviewable;
- agents can propose workflow changes as normal diffs.

The goal is not an autonomous built-in planning loop. The goal is a codebase
that agents can modify safely under human direction.

### 7. Release Discipline

Every meaningful release should have:

- a clear checklist;
- format, clippy, default tests, and feature-specific runtime checks;
- changelog entries for CLI, API, workflow, and runtime changes;
- documented known limitations;
- migration notes when data layout or API contracts change.

## Non-Goals

LightFlow should avoid:

- making a visual editor the workflow source of truth;
- embedding a general-purpose agent loop into the runtime;
- hiding provider/model behavior behind implicit routing;
- copying large model weights into projects;
- adding top-level concepts that duplicate `workflow` without solving a real
  user problem.

## Success Signals

The long-term direction is working when:

- a new workflow can be created, tested, synced, run, traced, and documented
  without leaving the LightFlow conventions;
- editor clients can be built from the HTTP/OpenAPI contract without private
  backend knowledge;
- agents can update workflow crates and their skills in one reviewable change;
- model-backed runs are reproducible enough to debug on another machine;
- users can choose between preview/mock, external, and native runtime paths with
  clear tradeoffs.
