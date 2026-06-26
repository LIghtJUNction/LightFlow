use super::project_workspace_inspection::inspect_project_workspace;
use super::{
    ApiError, ApiResult, ProjectWorkspaceCatalog, ProjectWorkspaceSummary,
    default_expected_project_workspace_names, default_optional_project_workspace_names,
    default_project_workflow_source_names, default_project_workflow_sources,
    expected_project_workspace_names, optional_project_workspace_names,
    project_config_template_command, project_config_write_command,
    project_submodule_update_command, project_workspace_config_path,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;

pub(in crate::api) fn project_git_status_issues(catalog: &ProjectWorkspaceCatalog) -> Vec<String> {
    catalog
        .workspaces
        .iter()
        .filter_map(|workspace| {
            if workspace.parent_gitlink_changed == Some(true) {
                return Some(format!(
                    "{} parent gitlink {} differs from child HEAD {}",
                    workspace.label,
                    workspace
                        .parent_gitlink_head
                        .as_deref()
                        .unwrap_or("unknown"),
                    workspace.git_head.as_deref().unwrap_or("unknown")
                ));
            }
            if workspace.git_dirty == Some(true) {
                return Some(format!(
                    "{} has {} changed path(s)",
                    workspace.label,
                    workspace.git_changed_count.unwrap_or(0)
                ));
            }
            workspace
                .git_status_error
                .as_ref()
                .map(|error| format!("{} git status unavailable: {error}", workspace.label))
        })
        .collect()
}

pub(super) fn filter_dirty_project_workspaces(catalog: &mut ProjectWorkspaceCatalog) {
    catalog
        .workspaces
        .retain(project_workspace_needs_git_review);
    recompute_project_workspace_issues(catalog);
    recompute_project_workspace_counts(catalog);
}

pub(super) fn filter_project_workspaces(
    catalog: &mut ProjectWorkspaceCatalog,
    project: &str,
) -> bool {
    let matched = catalog
        .workspaces
        .iter()
        .any(|workspace| project_workspace_matches(workspace, project));
    catalog
        .workspaces
        .retain(|workspace| project_workspace_matches(workspace, project));
    recompute_project_workspace_issues(catalog);
    recompute_project_workspace_counts(catalog);
    matched
}

pub(super) fn matched_project_workspace(
    catalog: &ProjectWorkspaceCatalog,
    project: &str,
) -> Option<String> {
    catalog
        .workspaces
        .iter()
        .find(|workspace| project_workspace_matches(workspace, project))
        .map(|workspace| workspace.name.clone())
}

pub(super) fn project_workspace_filter_choices(catalog: &ProjectWorkspaceCatalog) -> String {
    if catalog.known_workspace_names.is_empty() {
        "none".to_owned()
    } else {
        catalog.known_workspace_names.join(", ")
    }
}

pub(super) fn project_workspace_filter_alias_choices(catalog: &ProjectWorkspaceCatalog) -> String {
    if catalog.known_workspace_aliases.is_empty() {
        "none".to_owned()
    } else {
        catalog
            .known_workspace_aliases
            .iter()
            .map(|(alias, workspace)| format!("{alias}={workspace}"))
            .collect::<Vec<_>>()
            .join(", ")
    }
}

fn project_workspace_matches(workspace: &ProjectWorkspaceSummary, project: &str) -> bool {
    workspace.name == project
        || workspace.label == project
        || workspace.path.display().to_string() == project
        || project_workspace_aliases(&workspace.name)
            .iter()
            .any(|alias| alias == project)
}

pub(super) fn project_workspace_aliases(name: &str) -> Vec<String> {
    if let Some(alias) = name.strip_prefix("lightflow-")
        && !alias.is_empty()
        && alias != name
    {
        return vec![alias.to_owned()];
    }
    Vec::new()
}

fn project_workspace_needs_git_review(workspace: &ProjectWorkspaceSummary) -> bool {
    workspace.git_dirty == Some(true)
        || workspace.parent_gitlink_changed == Some(true)
        || workspace.git_status_error.is_some()
}

fn recompute_project_workspace_counts(catalog: &mut ProjectWorkspaceCatalog) {
    catalog.expected_count = catalog
        .workspaces
        .iter()
        .filter(|workspace| workspace.expected)
        .count();
    catalog.optional_count = catalog
        .workspaces
        .iter()
        .filter(|workspace| workspace.optional)
        .count();
    catalog.optional_workspace_names = catalog
        .workspaces
        .iter()
        .filter(|workspace| workspace.optional)
        .map(|workspace| workspace.name.clone())
        .collect();
    catalog.present_count = catalog
        .workspaces
        .iter()
        .filter(|workspace| workspace.exists)
        .count();
    catalog.linked_count = catalog
        .workspaces
        .iter()
        .filter(|workspace| workspace.exists && !workspace.broken)
        .count();
    catalog.missing_count = catalog
        .workspaces
        .iter()
        .filter(|workspace| workspace.expected && !workspace.exists)
        .count();
    catalog.directory_count = catalog
        .workspaces
        .iter()
        .filter(|workspace| workspace.exists && !workspace.is_symlink)
        .count();
    catalog.symlink_count = catalog
        .workspaces
        .iter()
        .filter(|workspace| workspace.exists && workspace.is_symlink)
        .count();
    catalog.submodule_count = catalog
        .workspaces
        .iter()
        .filter(|workspace| workspace.parent_gitlink_head.is_some())
        .count();
    catalog.not_symlink_count = catalog
        .workspaces
        .iter()
        .filter(|workspace| workspace.exists && !workspace.is_symlink)
        .count();
    catalog.broken_count = catalog
        .workspaces
        .iter()
        .filter(|workspace| workspace.broken)
        .count();
    catalog.workflow_crate_count = catalog
        .workspaces
        .iter()
        .map(|workspace| workspace.workflow_crate_count)
        .sum();
    catalog.valid = catalog.project_config_valid
        && catalog.issues.is_empty()
        && catalog.missing_count == 0
        && catalog.broken_count == 0;
}

fn recompute_project_workspace_issues(catalog: &mut ProjectWorkspaceCatalog) {
    let mut issues = Vec::new();
    if let Some(error) = &catalog.project_config_error {
        issues.push(format!("project config invalid: {error}"));
    }
    issues.extend(catalog.workspaces.iter().flat_map(|workspace| {
        workspace
            .issues
            .iter()
            .map(|issue| format!("{}: {issue}", workspace.label))
            .collect::<Vec<_>>()
    }));
    catalog.issues = issues;
}

fn project_workspace_config_values(
    root: &Path,
) -> (bool, Option<String>, Vec<String>, Vec<String>, Vec<String>) {
    match (|| -> ApiResult<(Vec<String>, Vec<String>, Vec<String>)> {
        Ok((
            expected_project_workspace_names(root)?,
            optional_project_workspace_names(root)?,
            default_project_workflow_sources(root)?,
        ))
    })() {
        Ok((expected, optional, default_sources)) => {
            (true, None, expected, optional, default_sources)
        }
        Err(error) => (
            false,
            Some(error.to_string()),
            default_expected_project_workspace_names(),
            default_optional_project_workspace_names(),
            default_project_workflow_source_names(),
        ),
    }
}

fn project_workspace_config_issue(project_config_error: &Option<String>) -> Vec<String> {
    project_config_error
        .as_ref()
        .map(|error| vec![format!("project config invalid: {error}")])
        .unwrap_or_default()
}

fn workspace_issue_list(workspaces: &[ProjectWorkspaceSummary]) -> Vec<String> {
    workspaces
        .iter()
        .flat_map(|workspace| {
            workspace
                .issues
                .iter()
                .map(|issue| format!("{}: {issue}", workspace.label))
                .collect::<Vec<_>>()
        })
        .collect()
}

pub(super) fn project_workspaces(root: &Path) -> ApiResult<ProjectWorkspaceCatalog> {
    let projects_dir = root.join("projects");
    let project_config_path = project_workspace_config_path(root);
    let project_config_present = project_config_path.exists();
    let (
        project_config_valid,
        project_config_error,
        expected_names,
        optional_names,
        default_workflow_sources,
    ) = project_workspace_config_values(root);
    let mut names: BTreeMap<String, ProjectWorkspaceRole> = BTreeMap::new();
    let mut configured_workspace_names = BTreeSet::new();
    for name in expected_names.iter().chain(default_workflow_sources.iter()) {
        configured_workspace_names.insert(name.clone());
        names.entry(name.to_owned()).or_default().expected = true;
    }
    for name in &optional_names {
        configured_workspace_names.insert(name.clone());
        names.entry(name.to_owned()).or_default().optional = true;
    }
    let project_submodule_update_command =
        project_submodule_update_command(configured_workspace_names.iter().map(String::as_str));
    let mut catalog_issues = project_workspace_config_issue(&project_config_error);
    match fs::read_dir(&projects_dir) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry?;
                let metadata = fs::symlink_metadata(entry.path())?;
                if !metadata.file_type().is_symlink() && !metadata.file_type().is_dir() {
                    continue;
                }
                let Some(name) = entry.file_name().to_str().map(ToOwned::to_owned) else {
                    continue;
                };
                names.entry(name).or_default();
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            catalog_issues.push("projects/ does not exist".to_owned());
        }
        Err(error) => return Err(ApiError::Io(error)),
    }

    let expected_count = names.iter().filter(|(_, role)| role.expected).count();
    let mut workspaces = Vec::new();
    for (name, role) in names {
        workspaces.push(inspect_project_workspace(
            root,
            &name,
            role.expected,
            role.optional && !role.expected,
        )?);
    }

    let present_count = workspaces
        .iter()
        .filter(|workspace| workspace.exists)
        .count();
    let linked_count = workspaces
        .iter()
        .filter(|workspace| workspace.exists && !workspace.broken)
        .count();
    let missing_count = workspaces
        .iter()
        .filter(|workspace| workspace.expected && !workspace.exists && !workspace.broken)
        .count();
    let directory_count = workspaces
        .iter()
        .filter(|workspace| workspace.exists && !workspace.is_symlink)
        .count();
    let symlink_count = workspaces
        .iter()
        .filter(|workspace| workspace.exists && workspace.is_symlink)
        .count();
    let submodule_count = workspaces
        .iter()
        .filter(|workspace| workspace.parent_gitlink_head.is_some())
        .count();
    let optional_count = workspaces
        .iter()
        .filter(|workspace| workspace.optional)
        .count();
    let not_symlink_count = directory_count;
    let broken_count = workspaces
        .iter()
        .filter(|workspace| workspace.broken)
        .count();
    let workflow_crate_count = workspaces
        .iter()
        .map(|workspace| workspace.workflow_crate_count)
        .sum();
    let known_workspace_names: Vec<String> = workspaces
        .iter()
        .map(|workspace| workspace.name.clone())
        .collect();
    let known_workspace_aliases = project_workspace_alias_map(&workspaces);
    let optional_workspace_names: Vec<String> = workspaces
        .iter()
        .filter(|workspace| workspace.optional)
        .map(|workspace| workspace.name.clone())
        .collect();
    catalog_issues.extend(workspace_issue_list(&workspaces));
    let valid = project_config_valid
        && missing_count == 0
        && broken_count == 0
        && linked_count >= expected_count;
    Ok(ProjectWorkspaceCatalog {
        valid,
        project_root: root.to_path_buf(),
        projects_dir,
        project_config_path,
        project_config_present,
        project_config_valid,
        project_config_error,
        project_config_template_command: project_config_template_command(),
        project_config_write_command: project_config_write_command(),
        project_submodule_update_command,
        project_filter: None,
        project_filter_matched: None,
        matched_project_workspace: None,
        dirty_filter: false,
        expected_count,
        optional_count,
        present_count,
        linked_count,
        missing_count,
        directory_count,
        symlink_count,
        submodule_count,
        not_symlink_count,
        broken_count,
        workflow_crate_count,
        known_project_workspaces: known_workspace_names.clone(),
        known_project_aliases: known_workspace_aliases.clone(),
        known_optional_workspace_names: optional_workspace_names.clone(),
        optional_workspace_names,
        default_workflow_sources,
        known_workspace_names,
        known_workspace_aliases,
        issues: catalog_issues,
        workspaces,
    })
}

fn project_workspace_alias_map(workspaces: &[ProjectWorkspaceSummary]) -> BTreeMap<String, String> {
    let mut aliases = BTreeMap::new();
    for workspace in workspaces {
        for alias in project_workspace_aliases(&workspace.name) {
            aliases.insert(alias, workspace.name.clone());
        }
    }
    aliases
}

#[derive(Debug, Default, Clone, Copy)]
struct ProjectWorkspaceRole {
    expected: bool,
    optional: bool,
}
