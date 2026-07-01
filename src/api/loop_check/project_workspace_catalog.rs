use super::{ProjectWorkspaceCatalog, ProjectWorkspaceSummary};
use std::collections::BTreeMap;

pub(super) fn recompute_project_workspace_counts(catalog: &mut ProjectWorkspaceCatalog) {
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

pub(super) fn recompute_project_workspace_issues(catalog: &mut ProjectWorkspaceCatalog) {
    let mut issues = project_workspace_config_issue(&catalog.project_config_error);
    issues.extend(workspace_issue_list(&catalog.workspaces));
    catalog.issues = issues;
}

pub(super) fn project_workspace_config_issue(project_config_error: &Option<String>) -> Vec<String> {
    project_config_error
        .as_ref()
        .map(|error| vec![format!("project config invalid: {error}")])
        .unwrap_or_default()
}

pub(super) fn workspace_issue_list(workspaces: &[ProjectWorkspaceSummary]) -> Vec<String> {
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

pub(super) fn project_workspace_alias_map(
    workspaces: &[ProjectWorkspaceSummary],
) -> BTreeMap<String, String> {
    let mut aliases = BTreeMap::new();
    for workspace in workspaces {
        for alias in project_workspace_aliases(&workspace.name) {
            aliases.insert(alias, workspace.name.clone());
        }
    }
    aliases
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
