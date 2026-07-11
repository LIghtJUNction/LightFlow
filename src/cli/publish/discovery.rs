use crate::api::project_filter_matches;
use crate::cli::{CliError, CliResult};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

#[derive(Debug)]
pub(super) struct WorkflowManifestRef {
    pub(super) path: PathBuf,
    pub(super) workspace: String,
    pub(super) workspace_root: PathBuf,
    pub(super) project_name: Option<String>,
}

pub(super) fn discover_workflow_manifest_refs(
    root: &Path,
    project: Option<&str>,
) -> CliResult<Vec<WorkflowManifestRef>> {
    if let Some(project) = project {
        let mut manifests = Vec::new();
        let mut matched = false;
        for workspace in discover_present_project_workspaces(root)? {
            if project_filter_matches(project, &workspace.name, &workspace.label, &workspace.root) {
                matched = true;
                manifests.extend(discover_workflow_manifests(
                    &workspace.root,
                    &workspace.label,
                    &workspace.root,
                    Some(workspace.name.clone()),
                )?);
            }
        }
        if !matched {
            return Err(CliError::Usage(format!(
                "project workspace filter matched no workspace: {project}"
            )));
        }
        return Ok(manifests);
    }

    let mut manifests = discover_workflow_manifests(root, "root", root, None)?;
    for workspace in discover_present_project_workspaces(root)? {
        manifests.extend(discover_workflow_manifests(
            &workspace.root,
            &workspace.label,
            &workspace.root,
            Some(workspace.name.clone()),
        )?);
    }
    Ok(manifests)
}

fn discover_workflow_manifests(
    root: &Path,
    workspace: &str,
    workspace_root: &Path,
    project_name: Option<String>,
) -> CliResult<Vec<WorkflowManifestRef>> {
    let project_workflows = root.join(".lightflow").join("workflows");
    let workflows = root.join("workflows");
    let legacy_workflows = root.join("lightflow").join("workflows");
    let source_root = if project_workflows.exists() {
        project_workflows
    } else if workflows.exists() {
        workflows
    } else {
        legacy_workflows
    };
    if !source_root.exists() {
        return Ok(Vec::new());
    }

    let mut manifests = Vec::new();
    for entry in sorted_dir_entries(&source_root)? {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if is_workflow_crate_dir(&path) {
            manifests.push(WorkflowManifestRef {
                path: path.join("Cargo.toml"),
                workspace: workspace.to_owned(),
                workspace_root: workspace_root.to_path_buf(),
                project_name: project_name.clone(),
            });
        }
    }
    manifests.sort_by(|left, right| left.path.cmp(&right.path));
    Ok(manifests)
}

#[derive(Debug)]
struct PublishProjectWorkspace {
    root: PathBuf,
    name: String,
    label: String,
}

fn discover_present_project_workspaces(root: &Path) -> CliResult<Vec<PublishProjectWorkspace>> {
    let projects = root.join("projects");
    let Ok(entries) = fs::read_dir(projects) else {
        return Ok(Vec::new());
    };
    let mut workspaces = Vec::new();
    for entry in entries {
        let entry = entry?;
        let metadata = fs::symlink_metadata(entry.path())?;
        if !metadata.file_type().is_symlink() && !metadata.file_type().is_dir() {
            continue;
        }
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let Some(name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
            continue;
        };
        workspaces.push(PublishProjectWorkspace {
            root: path,
            name: name.clone(),
            label: format!("projects/{name}"),
        });
    }
    workspaces.sort_by(|left, right| left.label.cmp(&right.label));
    Ok(workspaces)
}

fn sorted_dir_entries(path: &Path) -> CliResult<Vec<fs::DirEntry>> {
    let mut entries = fs::read_dir(path)?.collect::<Result<Vec<_>, io::Error>>()?;
    entries.sort_by_key(|entry| entry.path());
    Ok(entries)
}

fn is_workflow_crate_dir(path: &Path) -> bool {
    path.join("Cargo.toml").exists() && path.join("src").join("lib.rs").exists()
}
