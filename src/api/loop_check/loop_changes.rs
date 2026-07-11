use super::git_worktree::git_changed_paths;
use super::{
    ApiResult, LoopChangeStatus, LoopChangesReport, WorkflowChangeAccumulator, WorkflowChangeKind,
    WorkflowChangeSummary, discover_present_project_workspaces,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

pub(super) fn loop_changes_across_project_set(root: &Path) -> ApiResult<LoopChangesReport> {
    let mut changed = BTreeMap::<String, WorkflowChangeAccumulator>::new();
    let mut issues = Vec::new();

    for workspace in loop_change_workspaces(root) {
        let paths = match git_changed_paths(&workspace.root) {
            Ok(paths) => paths,
            Err(issue) => {
                issues.push(format!("{}: {issue}", workspace.label));
                continue;
            }
        };
        for path in paths {
            let display_path = workspace.display_path(&path);
            let Some((mut workflow_key, kind)) = classify_workflow_change(&workspace.root, &path)
            else {
                continue;
            };
            if let Some(prefix) = &workspace.workflow_key_prefix {
                workflow_key = format!("{prefix}:{workflow_key}");
            }
            let entry = changed.entry(workflow_key).or_default();
            match kind {
                WorkflowChangeKind::Workflow => entry.workflow_paths.push(display_path),
                WorkflowChangeKind::Skill => entry.skill_paths.push(display_path),
                WorkflowChangeKind::Patch => entry.patch_paths.push(display_path),
            }
        }
    }

    let changed_workflows = changed
        .into_iter()
        .map(|(workflow_key, mut entry)| {
            entry.workflow_paths.sort();
            entry.skill_paths.sort();
            entry.patch_paths.sort();
            let workflow_changed = !entry.workflow_paths.is_empty();
            let skill_changed = !entry.skill_paths.is_empty();
            let patch_changed = !entry.patch_paths.is_empty();
            let workflow_removed = workflow_crate_removed(root, &entry.workflow_paths);
            let (status, message) = match (workflow_changed, skill_changed, patch_changed) {
                (true, true, false) => (
                    LoopChangeStatus::Passed,
                    "workflow files and colocated agent skill changed together".to_owned(),
                ),
                (true, true, true) => (
                    LoopChangeStatus::Warning,
                    "workflow files, colocated agent skill, and saved patches changed together"
                        .to_owned(),
                ),
                (true, false, _) if workflow_removed => (
                    LoopChangeStatus::Passed,
                    "workflow crate removed; colocated agent skill is removed with the crate"
                        .to_owned(),
                ),
                (true, false, _) => (
                    LoopChangeStatus::Failed,
                    "workflow files changed without a colocated agent skill update".to_owned(),
                ),
                (false, true, false) => (
                    LoopChangeStatus::Passed,
                    "agent skill changed without workflow file changes".to_owned(),
                ),
                (false, true, true) => (
                    LoopChangeStatus::Warning,
                    "agent skill and saved patches changed without workflow file changes"
                        .to_owned(),
                ),
                (false, false, true) => (
                    LoopChangeStatus::Warning,
                    "saved patches changed; validate affected workflows before handoff".to_owned(),
                ),
                (false, false, false) => (
                    LoopChangeStatus::Passed,
                    "no workflow or skill changes".to_owned(),
                ),
            };
            WorkflowChangeSummary {
                workflow_key,
                status,
                message,
                workflow_changed,
                skill_changed,
                patch_changed,
                workflow_paths: entry.workflow_paths,
                skill_paths: entry.skill_paths,
                patch_paths: entry.patch_paths,
            }
        })
        .collect::<Vec<_>>();
    let valid = issues.is_empty()
        && !changed_workflows
            .iter()
            .any(|change| change.status == LoopChangeStatus::Failed);
    let passed = changed_workflows
        .iter()
        .filter(|change| change.status == LoopChangeStatus::Passed)
        .count();
    let warnings = changed_workflows
        .iter()
        .filter(|change| change.status == LoopChangeStatus::Warning)
        .count();
    let failed = changed_workflows
        .iter()
        .filter(|change| change.status == LoopChangeStatus::Failed)
        .count();
    let blockers = changed_workflows
        .iter()
        .filter(|change| change.status == LoopChangeStatus::Failed)
        .map(|change| format!("{}: {}", change.workflow_key, change.message))
        .collect::<Vec<_>>();
    let warning_messages = changed_workflows
        .iter()
        .filter(|change| change.status == LoopChangeStatus::Warning)
        .map(|change| format!("{}: {}", change.workflow_key, change.message))
        .collect::<Vec<_>>();

    Ok(LoopChangesReport {
        valid,
        project_root: root.to_path_buf(),
        issues,
        blockers,
        warning_messages,
        passed,
        warnings,
        failed,
        changed_workflows,
    })
}

pub(super) fn workflow_crate_removed(root: &Path, workflow_paths: &[PathBuf]) -> bool {
    !workflow_paths.is_empty()
        && workflow_paths
            .iter()
            .any(|path| path.file_name().and_then(|name| name.to_str()) == Some("Cargo.toml"))
        && workflow_paths.iter().all(|path| !root.join(path).exists())
}

#[derive(Debug)]
struct LoopChangeWorkspace {
    root: PathBuf,
    label: String,
    display_prefix: Option<PathBuf>,
    workflow_key_prefix: Option<String>,
}

impl LoopChangeWorkspace {
    fn display_path(&self, path: &Path) -> PathBuf {
        self.display_prefix
            .as_ref()
            .map(|prefix| prefix.join(path))
            .unwrap_or_else(|| path.to_path_buf())
    }
}

fn loop_change_workspaces(root: &Path) -> Vec<LoopChangeWorkspace> {
    let mut workspaces = vec![LoopChangeWorkspace {
        root: root.to_path_buf(),
        label: "root".to_owned(),
        display_prefix: None,
        workflow_key_prefix: None,
    }];
    if let Ok(project_workspaces) = discover_present_project_workspaces(root) {
        for workspace in project_workspaces {
            workspaces.push(LoopChangeWorkspace {
                root: workspace.root,
                label: format!("projects/{}", workspace.name),
                display_prefix: Some(workspace.display_prefix),
                workflow_key_prefix: Some(workspace.name),
            });
        }
    }
    workspaces
}

pub(super) fn classify_workflow_change(
    root: &Path,
    path: &Path,
) -> Option<(String, WorkflowChangeKind)> {
    let parts = path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect::<Vec<_>>();
    if parts.first() == Some(&".lightflow")
        && parts.get(1) == Some(&"patches")
        && path.extension().and_then(|extension| extension.to_str()) == Some("json")
    {
        let name = path.file_stem()?.to_str()?;
        return Some((format!("patch:{name}"), WorkflowChangeKind::Patch));
    }
    let (workflows_index, collection) = match parts.as_slice() {
        ["workflows", ..] => (0, root.join("workflows")),
        [".lightflow", "workflows", ..] => (1, root.join(".lightflow/workflows")),
        _ => return None,
    };
    let first = parts.get(workflows_index + 1)?;
    let relative_start = workflows_index + 2;
    let relative = &parts[relative_start..];
    let crate_dir = collection.join(first);
    let complete_crate =
        crate_dir.join("Cargo.toml").is_file() && crate_dir.join("src/lib.rs").is_file();
    let crate_deletion_marker = matches!(relative, ["Cargo.toml"] | ["src", "lib.rs"]);
    if !complete_crate && !crate_deletion_marker {
        return None;
    }
    let workflow_key = (*first).to_owned();
    if relative.first() == Some(&".agent")
        && relative.get(1) == Some(&"skills")
        && relative.last() == Some(&"SKILL.md")
    {
        return Some((workflow_key, WorkflowChangeKind::Skill));
    }
    if relative.is_empty() {
        return None;
    }
    Some((workflow_key, WorkflowChangeKind::Workflow))
}
