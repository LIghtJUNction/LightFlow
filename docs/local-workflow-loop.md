# Local Workflow Loop

This document turns the long-term goal into one near-term product loop:

> Make LightFlow the best local, source-controlled workflow environment for AI
> pipelines that agents can modify safely and humans can run, inspect, replay,
> and publish.

The loop is intentionally local-first and source-first. A workflow should start
as ordinary repository files, remain reviewable as diffs, and be runnable
through the same backend contract exposed to CLI, HTTP, MCP, and editor clients.

## Target User Flow

1. Create or import a workflow project.
2. Add standard, FLUX, or RIG workflow dependencies in editable mode.
3. Write or agent-edit workflow crates and their colocated agent skills.
4. Validate workflow contracts and node metadata.
5. Sync required model resources into `lfw.lock`.
6. Run the workflow from CLI, HTTP, MCP, or the editor.
7. Inspect the plan, selected executor, models, artifacts, and trace.
8. Patch or replay the run without mutating the source workflow.
9. Publish or import the workflow project as normal Cargo-backed source.

This loop should feel coherent before LightFlow invests in richer graph editing.
The editor should be a client of the loop, not a replacement source format.

## Product Requirements

- **Source of truth:** workflow definitions live in Rust crates with
  `pub fn define() -> WorkflowSpec`; generated or edited workflow source must
  remain reviewable in git.
- **Agent safety:** every workflow and plugin project has a colocated
  `.agent/skills/<skill>/SKILL.md`, and contract changes update the skill in
  the same patch.
- **Dependency ergonomics:** `lfw add --editable` and `lfw import` support
  local iteration across `lightflow-std`, `lightflow-flux`, `lightflow-rig`,
  and user workflow repositories without vendoring them into core. In the core
  checkout, `lfw loop check` verifies that the `projects/` workspaces for
  those workflow repositories are present and resolve.
- **Runtime transparency:** `lfw info`, `lfw plan`, `/executors`, `/nodes`, and
  `/models` expose which executor will run, why it is available, which models
  are required, and whether a path is preview, mock, external, native, or
  reserved.
- **Reproducible resources:** model requirements are declared in workflow
  source, locked by `lfw sync`, and used as paths or artifact handles during
  execution.
- **Inspectable execution:** all run surfaces write `.lightflow/runs` records
  with manifest, execution, events, selected runtime metadata, artifacts, and
  failure details. The editor run browser surfaces run provenance from the same
  records, including adapter surface, duration, stage count, and workflow ids.
- **Non-destructive iteration:** patches can disable, enable, retry, time-limit,
  replace, or fallback graph nodes for one run or a saved local patch without
  editing workflow source.
- **Publishing path:** workflow projects remain normal Cargo workspaces or
  crates so publication, import, update, and upgrade stay aligned with Cargo.

## Verification Gates

The loop is release-ready only when these gates pass from a clean checkout or a
documented local fixture:

```bash
scripts/check.sh
scripts/check.sh --list --full --project lightflow-std
scripts/check.sh --full
scripts/check.sh --full --project lightflow-std
scripts/check.sh --full --workflow lightflow.text_plan
scripts/check.sh --full --project lightflow-std --workflow lightflow.text_plan
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test
cargo test --test standard_nodes repository_workflow_crates_have_agent_skills
cargo test publish_endpoint_can_filter_project_workspaces
cargo test mcp_exposes_backend_tools
cargo test --features rig --test llm_rig
cargo check --features flux-native
lfw loop check
lfw loop check <workflow_id>
lfw loop changes
lfw loop projects
lfw loop projects --dirty
lfw loop projects --project lightflow-std
lfw loop projects --dirty --project lightflow-std
lfw release check
```

Workflow-library gates:

```bash
lfw import projects/lightflow-std
lfw import projects/lightflow-flux
lfw import projects/lightflow-rig
lfw loop check <workflow_id>
lfw list
lfw node test <workflow_id>
lfw plan <workflow_id>
lfw sync <workflow_id> --auto-model --apply
lfw run <workflow_id> --inputs @input.json
lfw trace last
lfw replay last
lfw publish <workflow_id>
```

The exact workflow ids and inputs should come from the project being verified.
For local multi-repo development from this checkout, use `projects/` as the
workspace view over sibling repositories.
`lfw loop check` includes present linked workflow crates in its agent-skill
readiness check, so missing or unusable
`projects/<name>/workflows/.../.agent/skills/.../SKILL.md` coverage is reported
from the core checkout. A usable workflow skill has frontmatter, names the
workflow id, and includes CLI plus HTTP run examples.
`lfw loop changes` also inspects present linked workflow projects under
`projects/`, including extra project directories beyond the expected
workspaces declared in `projects/lightflow-projects.toml`. When that file is
absent, the expected set defaults to `lightflow-std`, `lightflow-flux`, and
`lightflow-rig` for compatibility.
`lfw loop projects`, `GET /loop/projects`, `lightflow.loop.projects`, and
`lightflow://loop/projects` expose the same sibling-workspace catalog with
expected workspace counts, optional workspace rows, resolved paths, directory,
symlink, submodule, and workflow-crate counts. When a workspace can be inspected with `git status`, the
catalog also reports `git_dirty`, `git_changed_count`, and `git_changed_paths`,
plus `git_branch`, `git_upstream`, `git_remote_url`, and `git_head`, making it
clear which linked repositories, remotes, branches, and commits need review
before the parent gitlink is updated.
When a workspace is tracked as a git submodule, `parent_gitlink_head` and
`parent_gitlink_changed` show whether the parent repository already points at
the child checkout's current commit. `git_status_command`,
`git_stage_command`, `git_commit_command`, `git_push_command`, and
`parent_gitlink_stage_command` provide copy-free next steps for inspecting,
committing, pushing the child repo, and staging parent gitlink updates.
`known_workspace_names` keeps
the unfiltered workspace name list available for clients even when `dirty` or
`project` filters are applied. The catalog also includes
`known_project_workspaces` and `known_project_aliases` as compatibility aliases
matching the release/dev report naming. Each workspace summary carries its own
`aliases` list so clients can render row-level selector hints without reverse
mapping the top-level alias table. `project_config_path` and
`project_config_present` identify whether the catalog used
`projects/lightflow-projects.toml` or built-in compatibility defaults.
`project_config_valid` and `project_config_error` let catalog clients surface
config repair actions even when the project-set config is invalid.
`project_config_template_command`, `project_config_write_command`, and
`project_submodule_update_command` expose the next commands for previewing or
creating that config and initializing configured project submodules.
`default_workflow_sources` names the project workspaces whose workflow crates
are loaded by default, so clients can explain catalog contents without parsing
the config file. `known_optional_workspace_names` names configured optional
workspaces before filters, while `optional_workspace_names` names optional
workspaces in the returned rows. Optional workspaces are recognized when
present but do not fail the catalog when absent.
`not_symlink_count` remains in the API as a deprecated compatibility alias for
`directory_count`; new clients should prefer `directory_count`, `symlink_count`,
and `submodule_count`.
`lfw loop check` summarizes dirty or uninspectable project workspace git state
as `loop.projects.git_status`, so release preparation does not rely on manually
reading the full project catalog. `lfw dev check` and `lfw release check`
include the same inspect and parent-gitlink stage commands in the project
workspace review details, so automation can surface the next action without
issuing a second project catalog request.
Use `lfw loop projects --dirty`, `GET /loop/projects?dirty=true`,
`lightflow.loop.projects` with `dirty: true`, or
`lightflow://loop/projects?dirty=true` when you only need the linked workspaces
that still need child-repository commits, gitlink updates, or git-status
cleanup. `dirty_filter` marks whether a returned catalog is the full view or
this review-only slice.
Use `--project <name>`, `GET /loop/projects?project=<name>`, MCP
`project: "<name>"`, or `lightflow://loop/projects?project=<name>` when you
only need one workspace. The filter accepts full workspace names, labels,
relative or absolute paths, and conventional `lightflow-*` short aliases, such
as `std` for `lightflow-std` or `custom-tools` for
`lightflow-custom-tools`. Relative path filters can use labels such as
`projects/lightflow-std` or shell-style forms such as
`./projects/lightflow-std`; MCP resource URI query values may be percent
encoded. A project filter that matches no known workspace is reported as a
catalog issue so typos do not look like a clean workspace, and the issue lists
the known workspace names and aliases. MCP clients can discover the
parameterized resources through
`resources/templates/list` as `lightflow://loop?workflow_id={workflow_id}`,
`lightflow://loop?workflow_id={workflow_id}&require_replay={require_replay}`,
`lightflow://loop/projects?project={project}`, and
`lightflow://loop/projects?project={project}&dirty={dirty}`.
`lfw loop check <workflow_id>` is non-mutating: before the first run it reports
trace/replay readiness as warnings, then turns those checks green after a
completed run is recorded. It also uses the normal publish dry-run planner to
report whether local crates in the selected workflow's dependency graph are
publishable.
`projects/lightflow-projects.toml` can also declare
`[workspaces].optional` for related project repositories that tools should
recognize when present without requiring them in every checkout, and
`[workflows].default_sources`, the project workspaces whose workflow crates are
loaded without `LFW_PATH`, `lfw import`, or an explicit search path. The current
repository keeps `lightflow-std` in `default_sources` as the baseline standard
node set, while domain-specific sibling projects stay opt-in unless they are
added to `default_sources`. A default source is also treated as an expected
project workspace, so missing default workflow sources fail `lfw loop projects`
instead of silently removing nodes from the active catalog.
Run `lfw dev project-config-template` to preview the effective project-set
configuration, or add `--write` to create `projects/lightflow-projects.toml`
from the current catalog defaults. The JSON response includes
`project_config_template_command`, `project_config_write_command`, and
`project_submodule_update_command` so setup tools can repair config and
initialize configured submodules without separately calling `lfw loop projects`.
Runnable dependency workflows discovered from external Cargo path dependencies
are not treated as local workflow publish blockers unless they resolve to a
local workflow crate publish plan.
Pipeline runs count as history for every recorded stage, not only the first
workflow in the pipeline.
Model-lock readiness is checked from the same `/models` catalog: missing
`lfw.lock`, missing entries, invalid locks, and missing local model paths are
reported as warnings at project scope and for the selected workflow.
For release gates, `lfw loop check <workflow_id> --require-replay` turns missing
completed-run replay evidence into a failed readiness check.
At project scope, `lfw loop check` summarizes the workspace publish preflight
so blocked crates are visible before a user reaches `--apply`. It also
summarizes source-change safety so unsafe workflow edits without colocated
skill updates fail the readiness report.
Model-lock readiness warnings point to `/models` or
`lfw models requirements [workflow_id] [--blocked]` for details and to
`lfw sync <workflow_id> --auto-model --apply` for the selected-workflow fix
path; `--locked --apply` verifies already locked cache entries. The generated
next-command chain lists `lfw models requirements <workflow_id> --blocked`
before sync so humans and agents can inspect the exact lock gaps first. CLI
callers can also use `--status all|available|blocked` when they need a
machine-filtered catalog.
Run-history catalog issues, such as malformed local run manifests, are reported
as warnings without hiding valid runs or replay evidence. Legacy run manifests
that predate explicit status fields are inferred from terminal trace events or
execution status first; any remaining unknown-status run summaries still warn,
and `/runs` exposes their ids in `unknown_run_ids` so local history remains
visible and inspectable.
When saved patches exist, the same check verifies they are readable, reference
available workflow nodes and replacement/fallback workflows, and are valid so
temporary graph edits remain reviewable and reusable. Patch readiness warnings
include the patch name and validation issue summary.
With a selected workflow, `lfw loop check <workflow_id>` also warns when saved
patches are not compatible with that workflow's node ids or
replacement/fallback port contracts; use `lfw patch validate <patch>
--workflow <workflow_id>` for a strict patch-specific preflight.
The static editor's patch tool uses the same selected-workflow validation path
by default, while still allowing project-catalog validation for reusable patch
drafts.
When a completed run exists for the selected workflow, the suggested `trace`
and `replay` commands use that concrete run id instead of a generic selector,
and selected workflow reports expose it as `replay_run_id` for clients that
link directly to the replay evidence.
For selected workflows whose dependency graph contains multiple local workflow
publish plans, suggested publish commands keep the direct workflow preflight
and also include `lfw publish --workflows`, so child workflow crate blockers
can be inspected in dependency order.
Agents and editor clients can fetch the same readiness data through HTTP
`GET /loop`, HTTP `GET /workflows/{workflow_id}/loop`, the MCP
`lightflow.loop.check` tool, or the `lightflow://loop` resource. For selected
workflow checks, MCP resource clients can read
`lightflow://loop?workflow_id=<id>`; for selected workflow release gates, HTTP
clients can pass `GET /workflows/{workflow_id}/loop?require_replay=true`, MCP
tool clients can set `require_replay: true` on `lightflow.loop.check`, and MCP
resource clients can read
`lightflow://loop?workflow_id=<id>&require_replay=true`. Loop readiness reports
include top-level passed, warning, and failed counts for compact client
summaries, plus `issues` for failed checks and `warning_messages` for
non-blocking warnings.
Agents should also run `lfw loop changes` before handing off a patch; it fails
when workflow crate files changed without a colocated agent skill update.
Complete workflow crate removals are treated as safe because the colocated skill
is removed with the crate; partial workflow source edits still require the
colocated skill update. Skill-only documentation edits remain visible as passed
rows. Saved patch registry edits under `.lightflow/patches/*.json` are reported
as warning-level patch changes so runtime behavior edits remain visible in
review.
The static editor's source-change safety table shows workflow, skill, and patch
change flags plus their paths, so saved patch edits are inspectable alongside
source and skill diffs.
Project-level `lfw loop check` surfaces warning-level patch/source-change rows
without turning the readiness report invalid.
From the core checkout, that check covers both the root repository and any
present `projects/lightflow-std`, `projects/lightflow-flux`, or
`projects/lightflow-rig` linked git worktree, prefixing sibling changes with
`projects/<name>/...` in the report.
If a present linked workspace cannot be inspected with `git status`, readiness
fails because source-change safety cannot be proven.
HTTP and MCP clients can inspect the same source-change safety report through
`GET /loop/changes`, `lightflow.loop.changes`, and
`lightflow://loop/changes`. The report includes top-level passed, warning, and
failed counts for clients that need a compact review summary. Inspection
problems remain in `issues`, while unsafe changed workflows are listed in
`blockers`.

## Milestone Slices

### Slice 1: Authoring Contract

- `lfw init --workflow` and `lfw new` produce a runnable workflow crate.
- Generated workflow crates include an agent skill with CLI and HTTP examples.
- `lfw node test` verifies workflow metadata, skill presence, model bindings,
  runtime availability, and help output.

### Slice 2: Editable Ecosystem

- `lightflow-std`, `lightflow-flux`, and `lightflow-rig` can be imported or
  added in editable mode from sibling repositories.
- The core repo documents the local multi-repo view under `projects/`.
- Version and dependency errors point to the exact workflow id, crate, and
  install hint that need attention.

### Slice 3: Run And Inspect

- A workflow can be run from CLI, HTTP, and MCP with equivalent input payloads.
- `/nodes`, `/models`, `/executors`, `/runs`, `/artifacts`, and OpenAPI expose
  enough data for a read-only editor to explain what happened; `/models`
  includes catalog-level model-lock readiness counts and issues.
- Failed runs are written as records with structured error data.

### Slice 4: Patch And Replay

- `lfw run --patch`, saved patch registry entries, and HTTP/MCP patch surfaces
  share one serializable patch format.
- `lfw replay` proves the stored manifest can be rerun and reports runtime or
  model-lock changes explicitly. The editor replay view renders both runtime
  and model-lock fingerprints so drift can be traced to a stage, workflow,
  requirement, path, and hash.
- Patches stay data-only; Rust closure replacement remains a typed SDK concern.

### Slice 5: Publish And Reuse

- `lfw publish --workflows` handles local path dependency order, dedupes linked
  workspace duplicate workflow ids in favor of root workspace definitions in
  the default catalog, and reports top-level total, publishable, and blocked
  counts. Use
  `lfw publish --workflows --project <name>` to review one linked workflow
  project by full name, `projects/<name>` label, or short `lightflow-*` alias.
  Project-scoped publish views return that linked workspace's matching workflow
  crates even when the default catalog dedupes the same workflow id in favor of
  the root workspace.
  Path dependency order and crates.io blockers include dependencies inherited
  from `[workspace.dependencies]` as `workspace = true`; blockers are checked in
  normal, build, dev, and target-specific dependency sections.
  `lfw loop check` uses the same catalog for `loop.publish.workflow_crates` and
  `loop.publish.readiness`, so project-only workflow workspaces are treated as
  publishable workflow sources even when the root `workflows/` tree is empty.
- `/publish`, `lightflow.workflow.publish_list`, and `lightflow://publish`
  expose dependency-ordered publish readiness for every local workflow crate
  before upload, including present linked workflow project workspaces under
  `projects/`, per-crate workspace labels, a top-level dependency-ordered
  dry-run command list, and top-level total, publishable, and blocked counts.
  `GET /publish?project=<name>`, MCP `lightflow.workflow.publish_list` with
  `project`, and `lightflow://publish?project=<name>` use the same full-name,
  `projects/<name>` label, relative or absolute path, or short-alias filter as
  `lfw publish --workflows --project <name>`, and return the original filter,
  match flag, and canonical matched workspace. MCP clients can discover the
  parameterized resource through `resources/templates/list` as
  `lightflow://publish?project={project}`.
- `GET /release`, `lightflow.release.check`, and `lightflow://release` expose
  the same non-mutating release gate plan as `lfw release check`, including
  local-loop warnings, selected workflow loop readiness, source-change safety,
  sibling workspace, and workflow publish-readiness review gates plus the
  project catalog inspection commands. The
  `project` parameter, CLI `--project <name>`, or
  `lightflow://release?workflow_id=<id>&project=<name>` narrows the sibling
  workspace review and project catalog commands to one workspace while leaving
  the selected workflow gate controlled by `workflow_id`. MCP clients can
  discover release resource templates through `resources/templates/list`.
  Unknown project
  filters fail the review and list the known workspace names and aliases when
  available;
  if `projects/` is absent, an explicit project filter fails instead of using
  the optional-projects fallback. Release/dev reports expose
  `known_project_workspaces`, `known_project_aliases`, and
  `project_filter_matched` as structured data so clients can build project
  pickers and detect typos without parsing warning text. When a filter matches,
  `matched_project_workspace` carries the canonical workspace name. They also
  include `project_config_path`, `project_config_present`,
  `project_config_valid`, `project_config_error`, `default_workflow_sources`,
  `project_config_template_command`, `project_config_write_command`, and
  `project_submodule_update_command`, plus `known_optional_workspace_names`, so
  clients can show or repair the project-set config from the first dev/release
  check response;
  CLI `--apply` remains the only surface that executes release commands.
  Release reports include top-level passed, warning, failed, planned, and
  skipped counts for compact client summaries, plus optional per-check counts
  for review rows and planned commands. Warning release checks remain
  non-blocking but visible to HTTP, MCP, CLI, and editor clients.
- `lfw import` discovers workflow repositories and records Cargo path
  dependencies in the target project or global home.
- Changelog and migration notes describe any workflow format, API, or data
  layout changes.

## Design Boundaries

- Do not add a visual-editor-owned workflow format.
- Do not add a built-in autonomous agent loop.
- Do not hide provider/model selection behind implicit routing.
- Do not copy large model weights or tensors into project workflow JSON.
- Do not introduce a new top-level concept unless `workflow` cannot express the
  user problem cleanly.

## Success Signals

- A user can clone LightFlow plus workflow libraries, import editable
  dependencies, run a real workflow, inspect the trace, patch/replay the run,
  and publish changes without leaving source control.
- An agent can make a workflow behavior change, update the colocated skill, run
  focused validation, and produce a normal reviewable diff.
- A UI or external tool can build its workflow, node, model, run, and artifact
  views entirely from documented HTTP/OpenAPI or MCP contracts.
