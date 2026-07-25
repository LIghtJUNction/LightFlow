# LightFlow Long-Term Goals

This document describes the product direction beyond LightFlow's current
workflow foundation. It is intentionally strategic: release checklists,
near-term implementation plans, and claims about shipped capabilities belong
in separate documents.

For the tactical loop supported by the current backend, see
[Local Workflow Loop](local-workflow-loop.md).

## North Star

LightFlow should become an open-source, AI-driven video content production and
operations platform:

> Let organizations manage video content as systematically as data, while AI
> automates the path from raw media to content distribution and continuous
> improvement.

The aim is not merely to build another automatic video editor. Editing is the
entry point. The larger opportunity is to turn video from a collection of
one-off files into content assets that are searchable, generative,
distributable, and optimizable.

In short:

- near term: an AI video editing platform;
- medium term: a video content production platform;
- long term: video content automation infrastructure.

## Current Foundation

This direction is a roadmap, not a description of features that have already
shipped. LightFlow today is a backend-first, source-controlled workflow
environment for human-directed AI pipelines. Its current foundation includes
workflow crates, CLI/HTTP/MCP surfaces, explicit runtime and model contracts,
artifact handling, run history, tracing, and replay.

That foundation remains useful for the video-focused product:

- Rust workflow crates provide durable, reviewable workflow definitions.
- Backend contracts give CLI, API, MCP, and editor clients one source of truth.
- Explicit executors isolate Python, local-model, provider, and remote-runtime
  integrations from workflow graphs.
- Model locks, artifact handles, traces, and replay support reproducible media
  pipelines.
- Colocated agent skills and plugin projects make workflows discoverable and
  extensible.

The roadmap below must be delivered incrementally and verified before any
future capability is presented as available.

## Why Video Content Needs A Workflow Platform

Video production commonly requires a long manual chain:

```text
capture
  -> organize media
  -> find useful moments
  -> edit
  -> add subtitles
  -> translate
  -> dub
  -> create covers
  -> publish
  -> analyze results
  -> optimize
```

LightFlow's intended loop is:

```text
raw media
  -> AI understanding
  -> automated content generation
  -> multi-platform distribution
  -> performance feedback
  -> continuous optimization
```

The goal is to move video production from repeated manual assembly toward an
inspectable, human-directed intelligent pipeline.

## Product Roadmap

The time horizons below express sequence and product maturity, not release
commitments.

### Phase 1: AI Video Editing Platform (0-1 Year)

The first phase should establish a strong open-source AI video editing product.

#### Media Asset Management

Answer the basic question: "Where is the video I need?"

Planned capabilities include:

- a video asset library;
- AI-generated tags;
- semantic search;
- person recognition;
- scene recognition.

For example, a user should eventually be able to search for "all clips where a
customer says the price is too high" and receive the relevant source segments,
not merely matching filenames.

#### AI-Assisted Automatic Editing

Turn hours of source media into multiple useful short videos through:

- highlight and key-moment detection;
- filler and low-value segment removal;
- automatic pacing;
- story construction from selected moments.

Outputs must remain traceable to their source material so people can review and
correct AI decisions.

#### Intelligent Subtitles

Support the full subtitle production path:

- automatic transcription;
- multilingual subtitles;
- dynamic subtitle presentation;
- translation;
- reusable subtitle templates.

### Phase 2: Video Content Production Platform (1-3 Years)

The product should expand from "editing video" to "producing content" through
reusable content templates.

Templates describe the intended content structure and let AI find and assemble
matching material. Example verticals include:

- **Automotive:** find the customer problem, sales response, vehicle
  demonstration, and purchase outcome to generate a customer-case video.
- **Education:** extract knowledge points from course material and turn them
  into focused short videos.
- **E-commerce:** identify product selling points and source evidence to
  generate advertisements.

Templates should be inspectable workflows rather than opaque prompts, and
organizations should be able to adapt them without forking the platform.

### Phase 3: AI Content Workflow Platform (3-5 Years)

LightFlow should let organizations define complete content operations
workflows, combining the collaboration model of source control, the composable
graphs of node-based AI tools, and the automation reach of integration
platforms. In product terms, the intended combination is similar to
GitHub + ComfyUI + Zapier for video content operations.

An automotive content team, for example, could define:

```text
upload sales media
  -> identify customer pain points
  -> generate 30 short videos
  -> create Chinese and English versions
  -> publish to 10 platforms
  -> analyze performance
```

This phase requires user-defined workflows, multilingual generation,
distribution integrations, scheduling, policy-aware human approval, and
analytics feedback. Platform-specific behavior should live in plugins and
workflow packages instead of becoming implicit core behavior.

### Phase 4: Video Content Intelligence Infrastructure (5+ Years)

The durable asset at this stage is not a single model. It is the accumulated,
permission-aware knowledge of how content performs:

- which source moments are effective;
- which openings retain attention;
- which subtitle treatments improve completion;
- which titles improve click-through;
- which content patterns contribute to conversion.

With suitable consent, governance, provenance, and privacy controls, these
signals can support content intelligence that recommends and optimizes future
workflows. Analytics feedback must never silently rewrite production behavior;
people should be able to inspect, approve, and roll back optimization choices.

## Long-Term Product Shape

The eventual platform should contain coherent, interoperable product areas:

```text
LightFlow
├── Content Asset Management
├── AI Video Understanding
├── AI Editing Engine
├── Workflow Automation
├── Multi-language Generation
├── Distribution Management
├── Analytics
└── Plugin Ecosystem
```

These are long-term product areas, not a declaration that each one is a
separate top-level core domain type. The current `workflow` domain should remain
small until implementation evidence requires additional concepts.

## Product Differentiation

LightFlow should compete on the scale and inspectability of the whole content
pipeline:

- **Jianying/CapCut** primarily helps users edit a video; LightFlow should help
  teams produce and operate content at scale.
- **Adobe Premiere Pro** centers professional manual editing; LightFlow should
  center repeatable, AI-assisted production.
- **Canva** centers accessible design; LightFlow should center automated video
  content workflows.

These products may overlap with individual LightFlow capabilities. The
distinction is the open, source-controlled pipeline from content assets through
generation, distribution, feedback, and optimization.

## Technical Direction

### Rust Core

Rust should continue to own stable platform capabilities:

- projects and durable data contracts;
- timelines and media references;
- workflows and execution plans;
- plugin contracts;
- artifacts, traces, replay, and policy boundaries.

### Python AI Ecosystem

Python integrations should make it possible to adopt changing AI capabilities
without destabilizing the core:

- video understanding and multimodal models;
- automatic speech recognition;
- text-to-speech and dubbing;
- translation, ranking, and generation models.

These integrations should use explicit executor contracts. Model files,
tensors, provider details, and Python environment assumptions must not leak
into the durable workflow format.

### Plugin Ecosystem

Community and organization-specific functionality should be distributable as
normal plugin or workflow projects, for example:

```text
lightflow-plugin-youtube
lightflow-plugin-car-sales
lightflow-plugin-course
lightflow-plugin-ecommerce
```

Every workflow or plugin project should include an agent skill, testable usage
guidance, declared inputs and outputs, and clear runtime requirements.

## Architecture Guardrails

The video product direction extends the existing foundation rather than
discarding it:

- Keep the backend contract as the source of truth; UI clients must not own a
  hidden workflow format.
- Keep workflows source-controlled, agent-editable, and human-reviewed.
- Keep standard workflow building blocks small, neutral, reusable, and covered
  by contract tests.
- Keep large media artifacts, model weights, and tensor payloads out of
  workflow JSON.
- Keep runtime selection and provider behavior explicit.
- Keep run history and provenance strong enough to trace generated content back
  to source media, workflow versions, models, and human approvals.
- Add new top-level domain concepts only when a shipped product requirement
  cannot be represented cleanly by workflows and their contracts.
- Treat distribution credentials, biometric recognition, customer media, and
  performance data as governed resources with explicit permissions.
- Ship each roadmap slice with explicit verification, documented limitations,
  and migration guidance when contracts or layouts change.

LightFlow is not intended to become an autonomous built-in agent planner or a
closed, editor-owned automation format. AI may propose and execute bounded
workflow steps, but people retain ownership of intent, approval, and
publication decisions.

## Success Signals

The direction is working when each stage can be demonstrated with real,
end-to-end evidence:

- source media can be ingested, understood, searched, and traced;
- long recordings can produce reviewable short-video candidates;
- subtitle, translation, and dubbing outputs retain editable provenance;
- content templates can be reused across projects and verticals;
- one approved workflow can generate, localize, and distribute variants
  without hidden platform state;
- analytics can be attributed back to content, source material, and workflow
  choices;
- agents can modify workflows and their skills as ordinary reviewable diffs;
- organizations can choose local, remote, preview, and provider-backed runtimes
  with explicit tradeoffs.

The final positioning is therefore:

> LightFlow is an open-source AI video content workflow platform that turns
> video from a one-off file into a searchable, generative, distributable, and
> optimizable content asset.
