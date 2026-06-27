# Workflow Development Guide

This guide explains how to create workflow projects, add workflows to a
project, write workflow definitions, and call one workflow from another.

## Core Concepts

A LightFlow workflow is a reusable Rust library crate that exposes:

```rust
pub fn define() -> WorkflowSpec
```

The workflow source lives in `src/lib.rs`. LightFlow statically parses the Rust
builder DSL from that file; it does not compile or execute workflow source when
discovering workflow metadata.

There are two common shapes:

- Leaf workflow: declares input/output ports and optionally a runtime
  capability, but has no graph nodes.
- Composite workflow: declares graph nodes that reference other workflows and
  connects their ports with edges.

Every workflow or plugin project should also include an agent skill under
`.agent/skills/<skill-name>/SKILL.md`. Update the skill whenever inputs,
outputs, runtime behavior, model requirements, or common commands change. Keep
the skill concrete: include at least one `lfw run` example and one HTTP
`POST /workflows/{workflow_id}/run` example using the shared run body.

## Create A New Workflow Project

Use a workflow collection when the repository should contain one or more
workflow crates:

```bash
lfw init --workflow
```

This creates a project layout like:

```text
Cargo.toml
workflows/
  <category>/
    <short-name>/
      Cargo.toml
      src/
        lib.rs
      .agent/
        skills/
          <skill-name>/
            SKILL.md
```

Then create a workflow crate inside the collection:

```bash
lfw new image_prompt --category image --name "Image Prompt"
```

Use a runtime-aware template when the workflow should execute through a known
runtime capability:

```bash
lfw new image_generate --category image --runtime lightflow.image.generate
```

Runtime-aware templates include Node Schema metadata, a starter runtime
declaration, an agent skill with CLI/API examples, and a contract test scaffold
where applicable.

Use a plugin project when a repository should be a single Cargo crate that can
expose one workflow from `src/lib.rs`:

```bash
lfw init --plugin
```

Use the global workflow home when a workflow should be available to many local
projects without adding it to each project repository:

```bash
lfw new my_global_flow --category std --global
```

## Add Workflows To A Project

Project-local workflows are discovered automatically from the current working
directory's `workflows/` tree.

To add an external workflow crate as a dependency, use `lfw add`:

```bash
lfw add lightflow-std --version 0.1.1
lfw add lightflow-std --path projects/lightflow-std/workflows/std/std --editable
lfw add lightflow-std --git https://github.com/lightjunction/lightflow-std --package lightflow-std
```

Use `--editable` for local development. It records a Cargo path dependency and
keeps edits live.
Use an external checkout path such as `../lightflow-std/workflows/std/std`
only when `lightflow-std` is not checked out under `projects/`.

To import a repository that contains many workflow crates, use `lfw import`:

```bash
lfw import --global projects/lightflow-flux
lfw import --global https://github.com/lightjunction/lightflow-flux.git
```

Use `add` when the dependency target is one known Cargo package. Use
`import` when the target is a workflow repository or collection and LightFlow
should discover all workflow crates under `workflows/<category>/<crate>/`.

In practice:

- `lfw add lightflow-text-template --path projects/lightflow-std/workflows/std/text_template`
  adds one workflow crate.
- `lfw import projects/lightflow-std` scans the repository and adds every workflow
  crate it finds under `workflows/std/*`.

Global installs are written into the default LightFlow home, usually
`~/.lightflow`, or another directory listed in `LFW_PATH`.
That home is a normal Cargo workspace, not a custom package database. Global
install commands edit the home's `Cargo.toml`; Cargo still owns dependency
resolution and `Cargo.lock`.

Refresh dependency resolution with Cargo-backed commands:

```bash
lfw update
lfw upgrade
lfw update --global
lfw upgrade --global
```

Check what LightFlow can discover:

```bash
lfw list
lfw ls --detail
lfw info
lfw help <workflow_id>
```

## Write A Leaf Workflow

A minimal leaf workflow declares metadata and ports:

```rust
use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("acme.text.echo")
        .version("0.1.0")
        .name("Text Echo")
        .description("Return the input text unchanged.")
        .input("text", "text")
        .input_description("text", "Text to echo.")
        .input_required("text", true)
        .input_widget("text", "textarea")
        .output("text", "text")
        .output_description("text", "Echoed text.")
        .build()
}
```

Port metadata follows Node Schema v1. Prefer adding it for user-facing
workflows:

- `input_description` / `output_description` for help and editor labels.
- `input_required` and `input_default_json` for validation and defaults.
- `input_range` for numeric sliders or steppers.
- `input_enum_json` for select controls.
- `input_widget` for editor rendering hints such as `textarea`, `prompt`,
  `image`, `file_save`, `json`, `toggle`, or `model_select`.
- `input_artifact_kind` / `output_artifact_kind` for artifacts such as `image`
  and `mask`.
- `input_model_requirement` / `output_model_requirement` to bind ports to a
  declared model requirement.

## Write A Runtime-Backed Workflow

A runtime-backed workflow declares a capability from the Executor Registry:

```rust
use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("acme.image.preview")
        .version("0.1.0")
        .name("Image Preview")
        .description("Generate a deterministic preview image.")
        .input("prompt", "text")
        .input_description("prompt", "Prompt text.")
        .input_required("prompt", true)
        .input_widget("prompt", "prompt")
        .input("output_path", "path")
        .input_description("output_path", "Optional PNG output path.")
        .input_required("output_path", false)
        .input_widget("output_path", "file_save")
        .input_artifact_kind("output_path", "image")
        .output("image", "artifact")
        .output_description("image", "Generated image artifact metadata.")
        .output_artifact_kind("image", "image")
        .output("image_path", "path")
        .output_description("image_path", "Path to the generated image.")
        .output_artifact_kind("image_path", "image")
        .builtin_runtime("image_runtime", "lightflow.image.generate", "builtin.preview.v1")
        .hf_model(
            "image_model",
            "preview-model",
            "text-to-image",
            "gguf",
            "owner/repo",
            "model.gguf",
        )
        .build()
}
```

Use `.runtime(id, capability)` when any available executor for that capability
may run the workflow. Use `.builtin_runtime(id, capability, engine)` when the
workflow requires a specific builtin engine, such as `builtin.preview.v1` or
`builtin.llm.mock.v1`.

Run conformance before publishing or installing:

```bash
lfw node test acme.image.preview
```

This checks workflow validation, `lfw help`, Node Schema metadata, model
bindings, runtime executor availability, and the workflow crate's agent skill.
Generated placeholder descriptions are reported as `node.placeholders`
warnings here, so agents can spot incomplete metadata before the stricter
publish gate fails.
Before publishing, replace generated `TODO` workflow, input, and output
descriptions; `lfw publish` reports those placeholders as publish blockers for
workflow crates.
Before handing off agent-authored changes, run `lfw loop changes` to confirm
workflow crate edits and colocated skill edits are paired in the same review.
Tools can read the same report from `/loop/changes` or
`lightflow.loop.changes`.
Use `lfw dev check` for the broader developer gate plan before handoff. It
reuses the release gate definitions, but presents them as a daily development
workflow: local loop readiness, source-change safety, sibling project
workspaces, publish readiness, formatting, clippy, tests, workflow skill
coverage, and feature-specific runtime checks.
Use `lfw dev check --project <name>` when the current change is scoped to one
linked project workspace. The report still plans the normal developer gates,
but the project workspace review and `lfw loop projects` commands are
filtered to that workspace. A project name that matches no linked workspace
fails the gate and reports the known workspace names and aliases.
When a workflow skill is missing required usage guidance, run
`lfw dev skill-template <workflow_id>` to generate a compliant starter
`SKILL.md` with frontmatter, workflow contract notes, a CLI run example, and
an HTTP run example. Add `--write` to create it under the workflow crate's
`.agent/skills/<skill-name>/SKILL.md`; existing files are not overwritten
unless `--force` is also passed.
Use `lfw dev project-config-template` to inspect a starter
`projects/lightflow-projects.toml`, and add `--write` to create it when a
project set should stop relying on built-in compatibility defaults. Existing
config files are not overwritten unless `--force` is passed. The command still
returns a repair template when the existing config is invalid, so
`--write --force` can replace a broken project-set config intentionally. The
same response includes `project_config_template_command`,
`project_config_write_command`, and `project_submodule_update_command` for
repair prompts and configured submodule initialization.
`lfw dev check` and `lfw release check` also expose `project_config_valid`,
`project_config_error`, and the same template/write commands, so editor and
agent clients can surface a repair action from the first gate report.
The development profile skips release-only artifact and changelog-section
checks; those remain part of `lfw release check`.
`lfw publish <workflow_id> --apply` and `lfw publish --workflows --apply` run
the same gate before invoking Cargo publish commands.

## Call Other Workflows From A Workflow

Composite workflows use `.node()` to instantiate another workflow and `.edge()`
to connect output ports to input ports.

```rust
use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("acme.prompted_image")
        .version("0.1.0")
        .name("Prompted Image")
        .description("Render a prompt template, then generate an image.")
        .input("topic", "text")
        .input_description("topic", "Subject to render.")
        .input_required("topic", true)
        .input_widget("topic", "text")
        .output("image", "artifact")
        .output_description("image", "Generated image artifact.")
        .output_artifact_kind("image", "image")
        .output("image_path", "path")
        .output_description("image_path", "Generated PNG path.")
        .output_artifact_kind("image_path", "image")
        .depends_on("lightflow.text.template", "0.1.0")
        .depends_on("lightflow.text_to_image", "0.1.0")
        .node("template", "lightflow.text.template")
        .node("generate", "lightflow.text_to_image")
        .edge("template", "text", "generate", "prompt")
        .build()
}
```

Workflow inputs are automatically visible to child nodes when the child has an
input with the same name. Use `.edge(from_node, from_port, to_node, to_port)`
when a child output should feed another child input.

Declare dependencies with exact versions so `lfw deps` can verify the graph:

```bash
lfw deps acme.prompted_image
```

Use install hints when a dependency may not already be installed:

```rust
workflow("acme.prompted_image")
    .depends_on_path(
        "lightflow.text_to_image",
        "0.1.0",
        "lightflow-text-to-image",
        "projects/lightflow-std/workflows/std/text_to_image",
    )
    .depends_on_git(
        "lightflow.text.template",
        "0.1.0",
        "lightflow-text-template",
        "https://github.com/lightjunction/lightflow-std",
        "lightflow-text-template",
    )
```

Then let LightFlow add missing Cargo dependencies:

```bash
lfw sync acme.prompted_image --apply
```

## Call Workflows From The CLI

Run one workflow:

```bash
lfw run acme.text.echo -i text='"hello"'
```

Use JSON values for non-string inputs:

```bash
lfw run lightflow.control.merge \
  -i a='{"prompt":"cat"}' \
  -i b='{"seed":1}' \
  -i mode='"object"'
```

Pipe one workflow into another from the CLI:

```bash
lfw run lightflow.text_to_image \
  -i prompt='"a quiet lake"' \
  -i output_path='"out/lake.png"' \
  '|' lightflow.image.invert \
  -i output_path='"out/lake-inverted.png"'
```

Use `lfx` as a short alias for `lfw run`:

```bash
lfx lightflow.text_to_image --text "a quiet lake" --output out/lake.png
```

## Development Checklist

Before committing a workflow change:

1. Keep `src/lib.rs` as the source of truth for workflow metadata.
2. Add Node Schema metadata for user-facing ports.
3. Declare runtime capabilities and model requirements explicitly.
4. Add or update `.agent/skills/<skill-name>/SKILL.md`.
5. Run `lfw help <workflow_id>`.
6. Run `lfw node test <workflow_id>`.
7. Run `lfw deps <workflow_id>` for composite workflows.
8. Run representative `lfw run ...` commands for executable workflows.
