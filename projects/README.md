# LightFlow Project Set

This folder groups related LightFlow workflow projects for local iteration:

- `lightflow-flux`
- `lightflow-std`
- `lightflow-rig`
- `lightflow-auto-editing`

`lightflow-projects.toml` declares the required project workspaces for local
review:

```toml
[workspaces]
expected = ["lightflow-flux", "lightflow-std", "lightflow-rig"]
optional = ["lightflow-auto-editing"]

[workflows]
default_sources = ["lightflow-std"]
```

Entries are project directory names under `projects/`, not filesystem paths.
Use `lightflow-std`, not `projects/lightflow-std` or `../lightflow-std`.

When this file is absent, LightFlow falls back to the same three expected
workspaces and uses `lightflow-std` as the default project workflow source for
compatibility. Extra `projects/lightflow-*` directories are still discovered
and can be filtered by their short alias. Use `[workspaces].optional` for
related submodules that should be recognized when present but should not fail
the core catalog when absent. Required status wins if a workspace appears in
both optional and expected/default-source lists. Add a project to
`[workflows].default_sources` only when its workflows should be visible in the
core catalog without `LFW_PATH`, `lfw import`, or an explicit search path. A
default source is also treated as a required workspace by `lfw loop projects`,
because missing default workflow sources would otherwise silently remove nodes
from the active catalog.

Each entry is a git submodule managed inside this folder. The projects remain
independent git repositories; edit, commit, and push changes from each project
directory, then update the parent LightFlow gitlink when you want the core repo
to point at the new submodule commit.

After cloning LightFlow without `--recurse-submodules`, initialize this project
set before running the default workflow catalog:

```bash
lfw dev project-config-template
git submodule update --init --recursive projects/lightflow-auto-editing projects/lightflow-flux projects/lightflow-rig projects/lightflow-std
```

The `project_submodule_update_command` field in the template response is the
authoritative command for the configured project set. Use plain
`git submodule update --init --recursive` when you also want every optional
checkout, including the static UI.

Use this folder when you want one workspace view across the core LightFlow repo
and the workflow projects that are commonly iterated together.

The core backend discovers workflow crates from `[workflows].default_sources`.
This repository keeps `projects/lightflow-std` there because it is the baseline
standard node library. Other sibling projects are intentionally opt-in workflow
sources; add them through `LFW_PATH`, `lfw import`, an explicit workflow search
path, or the default source list when you want their nodes in the active
catalog.

## Adding A Project Workspace

Use a git submodule for each shared workflow/plugin project:

```bash
git submodule add https://github.com/lightjunction/lightflow-example.git projects/lightflow-example
```

Then update `projects/lightflow-projects.toml`:

- Add always-required projects to `[workspaces].expected`.
- Add related but non-blocking projects to `[workspaces].optional`.
- Add a project to `[workflows].default_sources` only when its workflow crates
  should load without `LFW_PATH`, `lfw import`, or an explicit search path.

Check the result before handing off:

```bash
lfw dev project-config-template
lfw loop projects --project lightflow-example
lfw dev check --project lightflow-example
```

Keep core SDK crates such as `lightflow-macros` in the root Cargo workspace,
not in `projects/`.

Inspect the current linked workspace set with:

```bash
lfw loop projects
lfw loop projects --dirty
lfw loop projects --project lightflow-std
```

The same read-only catalog is exposed through `GET /loop/projects`,
`lightflow.loop.projects`, and `lightflow://loop/projects`. Git workspaces
include `git_dirty`, `git_changed_count`, and `git_changed_paths` fields so you
can see which submodules and files need commits before updating parent
gitlinks. `git_branch`, `git_upstream`, `git_remote_url`, and `git_head`
identify the child checkout currently being reviewed. `parent_gitlink_head` and
`parent_gitlink_changed` show whether the parent repository already records
that child checkout.
`git_status_command`, `git_stage_command`, `git_commit_command`,
`git_push_command`, and `parent_gitlink_stage_command` expose the next child
repo and parent git commands for tools and scripts. `project_config_path` and
`project_config_present` identify whether the catalog came from
`lightflow-projects.toml` or built-in compatibility defaults, while
`project_config_valid` and `project_config_error` expose parse or validation
problems without requiring clients to parse CLI stderr.
`project_config_template_command`, `project_config_write_command`, and
`project_submodule_update_command` expose the next commands for previewing or
creating that config and initializing the configured project submodules.
`directory_count`, `symlink_count`, and `submodule_count` describe the local
checkout shape directly. `not_symlink_count` remains as a deprecated
compatibility alias for `directory_count`.
`known_workspace_names` preserves the unfiltered name list for clients, and
`known_optional_workspace_names` preserves the unfiltered optional project list
for clients, while `optional_workspace_names` identifies optional workspaces in
the returned rows.
`default_workflow_sources` reports the configured project workflow sources
loaded by default. `lfw loop check` and release review report dirty or
uninspectable project workspace git state as warnings. Use
`lfw loop projects --dirty`, `GET /loop/projects?dirty=true`, or
`lightflow.loop.projects` with `dirty: true` for the shortened list of
workspaces that need git-status or parent-gitlink review.
Use `--project <name>`, `GET /loop/projects?project=<name>`, or MCP
`project: "<name>"` to inspect one project workspace. Filters accept full
workspace names, labels, paths, and conventional `lightflow-*` short aliases,
such as `std` for `lightflow-std` or `custom-tools` for
`lightflow-custom-tools`. Unknown project filters are reported as catalog
issues instead of returning a silent empty result, and include the known
workspace names and aliases.
