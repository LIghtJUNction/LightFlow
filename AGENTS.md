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
