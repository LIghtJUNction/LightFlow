use super::project_workspaces::{project_workspace_aliases, project_workspaces};
use super::workflow_crates::discover_local_workflow_crates;
use super::{
    ApiError, ApiResult, ProjectWorkspaceSummary, git_changed_paths, git_current_branch,
    git_current_upstream, git_full_head, git_origin_remote_url, git_short_head,
    parent_gitlink_full_head, short_commit,
};
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn inspect_project_workspace(
    root: &Path,
    name: &str,
    expected: bool,
    optional: bool,
) -> ApiResult<ProjectWorkspaceSummary> {
    let relative_path = PathBuf::from("projects").join(name);
    let path = root.join(&relative_path);
    let label = format!("projects/{name}");
    let mut issues = Vec::new();
    let mut exists = path.exists();
    let mut is_symlink = false;
    let mut broken = false;
    let mut target = None;
    let mut resolved_path = None;
    let mut workflow_crate_count = 0;
    let mut git_dirty = None;
    let mut git_changed_count = None;
    let mut git_changed_path_list = None;
    let mut git_branch = None;
    let mut git_upstream = None;
    let mut git_remote_url = None;
    let mut git_head = None;
    let mut parent_gitlink_head = None;
    let mut parent_gitlink_changed = None;
    let mut git_status_command = None;
    let mut git_stage_command = None;
    let mut git_commit_command = None;
    let mut git_push_command = None;
    let mut parent_gitlink_stage_command = None;
    let mut git_status_error = None;

    match fs::symlink_metadata(&path) {
        Ok(metadata) => {
            is_symlink = metadata.file_type().is_symlink();
            if is_symlink {
                target = Some(fs::read_link(&path)?);
            }
            if exists {
                resolved_path = fs::canonicalize(&path).ok();
                workflow_crate_count = discover_local_workflow_crates(&path)?.len();
                git_status_command = Some(vec![
                    "git".to_owned(),
                    "-C".to_owned(),
                    relative_path.display().to_string(),
                    "status".to_owned(),
                    "--short".to_owned(),
                ]);
                git_stage_command = Some(vec![
                    "git".to_owned(),
                    "-C".to_owned(),
                    relative_path.display().to_string(),
                    "add".to_owned(),
                    ".".to_owned(),
                ]);
                git_commit_command = Some(vec![
                    "git".to_owned(),
                    "-C".to_owned(),
                    relative_path.display().to_string(),
                    "commit".to_owned(),
                    "-m".to_owned(),
                    "<message>".to_owned(),
                ]);
                git_push_command = Some(vec![
                    "git".to_owned(),
                    "-C".to_owned(),
                    relative_path.display().to_string(),
                    "push".to_owned(),
                ]);
                match git_changed_paths(&path) {
                    Ok(paths) => {
                        git_changed_count = Some(paths.len());
                        git_dirty = Some(!paths.is_empty());
                        if !paths.is_empty() {
                            git_changed_path_list = Some(paths);
                        }
                        git_branch = git_current_branch(&path).ok();
                        git_upstream = git_current_upstream(&path).ok();
                        git_remote_url = git_origin_remote_url(&path).ok();
                        git_head = git_short_head(&path).ok();
                        if let Ok(Some(parent_head)) =
                            parent_gitlink_full_head(root, &relative_path)
                        {
                            parent_gitlink_head = Some(short_commit(&parent_head));
                            parent_gitlink_stage_command = Some(vec![
                                "git".to_owned(),
                                "add".to_owned(),
                                relative_path.display().to_string(),
                            ]);
                            if let Ok(child_head) = git_full_head(&path) {
                                parent_gitlink_changed = Some(parent_head != child_head);
                            }
                        }
                    }
                    Err(error) => {
                        git_status_error = Some(error);
                    }
                }
            } else {
                broken = true;
                issues.push("symlink target does not exist".to_owned());
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            exists = false;
            if expected {
                issues.push("missing expected project workspace checkout".to_owned());
            }
        }
        Err(error) => return Err(ApiError::Io(error)),
    }

    Ok(ProjectWorkspaceSummary {
        name: name.to_owned(),
        label,
        aliases: project_workspace_aliases(name),
        expected,
        optional,
        path: relative_path,
        target,
        resolved_path,
        exists,
        is_symlink,
        broken,
        workflow_crate_count,
        git_dirty,
        git_changed_count,
        git_changed_paths: git_changed_path_list,
        git_branch,
        git_upstream,
        git_remote_url,
        git_head,
        parent_gitlink_head,
        parent_gitlink_changed,
        git_status_command,
        git_stage_command,
        git_commit_command,
        git_push_command,
        parent_gitlink_stage_command,
        git_status_error,
        issues,
    })
}

#[derive(Debug)]
pub(super) struct PresentProjectWorkspace {
    pub(super) name: String,
    pub(super) root: PathBuf,
    pub(super) display_prefix: PathBuf,
}

pub(super) fn discover_present_project_workspaces(
    root: &Path,
) -> ApiResult<Vec<PresentProjectWorkspace>> {
    let catalog = project_workspaces(root)?;
    let mut workspaces = Vec::new();
    for workspace in catalog
        .workspaces
        .into_iter()
        .filter(|workspace| workspace.exists && !workspace.broken)
    {
        let workspace_root = root.join(&workspace.path);
        if !workspace_root.is_dir() {
            continue;
        }
        workspaces.push(PresentProjectWorkspace {
            name: workspace.name,
            root: workspace_root,
            display_prefix: workspace.path,
        });
    }
    workspaces.sort_by(|left, right| left.display_prefix.cmp(&right.display_prefix));
    Ok(workspaces)
}
