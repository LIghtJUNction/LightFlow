use std::collections::BTreeMap;
use std::path::PathBuf;

use super::ApiService;
use crate::api::ProjectWorkspaceOptions;
use crate::api::project_config::{
    project_config_template_command, project_config_write_command, project_submodule_update_command,
};

pub(super) struct ProjectConfigReport {
    pub path: PathBuf,
    pub present: bool,
    pub valid: bool,
    pub error: Option<String>,
    pub template_command: Vec<String>,
    pub write_command: Vec<String>,
    pub submodule_update_command: Vec<String>,
    pub default_workflow_sources: Vec<String>,
    pub known_optional_workspace_names: Vec<String>,
}

pub(super) fn project_config_report(service: &ApiService) -> ProjectConfigReport {
    let path = service.project_workspace_config_path();
    let present = path.exists();
    let (
        valid,
        error,
        default_workflow_sources,
        known_optional_workspace_names,
        submodule_update_command,
    ) = service
        .project_workspaces()
        .map(|catalog| {
            (
                catalog.project_config_valid,
                catalog.project_config_error,
                catalog.default_workflow_sources,
                catalog.known_optional_workspace_names,
                catalog.project_submodule_update_command,
            )
        })
        .unwrap_or_else(|error| {
            let (expected, optional, default_sources) = service.default_project_config_values();
            let submodule_update_command = project_submodule_update_command(
                expected
                    .iter()
                    .chain(default_sources.iter())
                    .chain(optional.iter())
                    .map(String::as_str),
            );
            (
                false,
                Some(error.to_string()),
                default_sources,
                optional,
                submodule_update_command,
            )
        });
    ProjectConfigReport {
        path,
        present,
        valid,
        error,
        template_command: project_config_template_command(),
        write_command: project_config_write_command(),
        submodule_update_command,
        default_workflow_sources,
        known_optional_workspace_names,
    }
}

pub(super) fn matched_project_workspace(
    service: &ApiService,
    project: Option<&str>,
) -> Option<String> {
    let project = project?;
    service
        .project_workspaces_with_options(ProjectWorkspaceOptions {
            dirty_only: false,
            project: Some(project.to_owned()),
        })
        .ok()
        .and_then(|catalog| catalog.matched_project_workspace)
}

pub(super) fn known_project_aliases(service: &ApiService) -> BTreeMap<String, String> {
    service
        .project_workspaces()
        .map(|catalog| catalog.known_workspace_aliases)
        .unwrap_or_default()
}

pub(super) fn project_filter_matched(service: &ApiService, project: Option<&str>) -> Option<bool> {
    let project = project?;
    Some(
        service
            .project_workspaces_with_options(ProjectWorkspaceOptions {
                dirty_only: false,
                project: Some(project.to_owned()),
            })
            .map(|catalog| !catalog.workspaces.is_empty())
            .unwrap_or(false),
    )
}

pub(super) fn known_project_workspaces(service: &ApiService) -> Vec<String> {
    service
        .project_workspaces()
        .map(|catalog| catalog.known_workspace_names)
        .unwrap_or_default()
}
