# Agent Instructions

## Development Documentation

Start with these docs when changing LightFlow:

- `README.md` for the user-facing CLI and workflow model.
- `docs/architecture.md` for backend and resolver architecture.
- `docs/data-layout.md` for project, `LFW_PATH`, workflow, lockfile, and agent skill layout.

## Workflow And Plugin Skills

Every workflow or plugin project must include an agent skill that explains how to use it.

- Workflow crates should place skills under `<workflow-crate>/.agent/skills/<skill-name>/SKILL.md`.
- Plugin projects should place skills under `.agent/skills/<skill-name>/SKILL.md`.
- Keep `SKILL.md` valid for agent skill loaders: YAML frontmatter with `name`, `description`, and `version`, followed by concise workflow usage guidance.
- When adding or changing a workflow's inputs, outputs, runtime behavior, models, or common commands, update the corresponding skill in the same change.
- `lfw sync --apply` discovers these skills and can install them into project or global agent skill directories using symlinks, with choices recorded in `lfw.lock`.

## Source Shape Standards

- Before changing LightFlow source, read `README.md`, `docs/architecture.md`, and `docs/data-layout.md`.
- Run `scripts/check-source-shape.sh` before sharing code changes that touch `src/`.
- Source-shape rules for `src/`:
  - Keep files below 500 lines where practical; when a file approaches this boundary, split it by semantic module boundaries.
  - Avoid meaningless numeric filenames such as `001_*`, `002_*`, `003_*`, or any three-digit numeric-prefix filename.
- Refactors should preserve public APIs, serialization formats, and error semantics unless behavior changes are explicitly requested.
- Mandatory post-change checks for Rust surface changes:
  - `cargo fmt --check`
  - `cargo test --lib`
  - `cargo clippy --all-targets -- -D warnings`
  - `git diff --check`
- Use deterministic git test helpers for tests that create temporary repositories and commits.
