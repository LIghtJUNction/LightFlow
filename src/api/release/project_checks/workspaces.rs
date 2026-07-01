use super::{ApiService, ReleaseCheck, ReleaseCheckKind, ReleaseCheckStatus};
use crate::api::{ProjectWorkspaceCatalog, ProjectWorkspaceOptions, loop_check};
use std::path::PathBuf;

pub(in crate::api::release) fn project_workspace_review_check(
    service: &ApiService,
    project: Option<&str>,
) -> ReleaseCheck {
    let root = service.repo_root();
    if !root.join("projects").exists() {
        if let Some(project) = project {
            return ReleaseCheck {
                id: "release.review.project_workspaces",
                kind: ReleaseCheckKind::Review,
                status: ReleaseCheckStatus::Failed,
                message: format!(
                    "project workspace catalog is unavailable for requested project filter: {project}"
                ),
                details: vec![
                    format!("project filter: {project}"),
                    "projects/ directory does not exist".to_owned(),
                ],
                count: Some(0),
                command: None,
                path: Some(PathBuf::from("projects")),
                exit_code: None,
                stdout_tail: None,
                stderr_tail: None,
            };
        }
        return ReleaseCheck {
            id: "release.review.project_workspaces",
            kind: ReleaseCheckKind::Review,
            status: ReleaseCheckStatus::Passed,
            message: "projects/ sibling workspace catalog is optional outside the core multi-repo workspace".to_owned(),
            details: Vec::new(),
            count: Some(0),
            command: None,
            path: Some(PathBuf::from("projects")),
            exit_code: None,
            stdout_tail: None,
            stderr_tail: None,
        };
    }
    let report = service.project_workspaces_with_options(ProjectWorkspaceOptions {
        dirty_only: false,
        project: project.map(ToOwned::to_owned),
    });
    match report {
        Ok(report) if report.valid => valid_project_workspace_check(&report, project),
        Ok(report) => invalid_project_workspace_check(report, project),
        Err(error) => ReleaseCheck {
            id: "release.review.project_workspaces",
            kind: ReleaseCheckKind::Review,
            status: ReleaseCheckStatus::Failed,
            message: format!("project workspace catalog could not be inspected: {error}"),
            details: vec![error.to_string()],
            count: None,
            command: None,
            path: Some(PathBuf::from("projects")),
            exit_code: None,
            stdout_tail: None,
            stderr_tail: None,
        },
    }
}

fn valid_project_workspace_check(
    report: &ProjectWorkspaceCatalog,
    project: Option<&str>,
) -> ReleaseCheck {
    let git_issues = loop_check::project_git_status_issues(report);
    if git_issues.is_empty() {
        ReleaseCheck {
            id: "release.review.project_workspaces",
            kind: ReleaseCheckKind::Review,
            status: ReleaseCheckStatus::Passed,
            message: format!(
                "project workspace catalog passed with {} present workspace(s) and {} workflow crate(s)",
                report.linked_count, report.workflow_crate_count
            ),
            details: project_workspace_filter_details(project),
            count: Some(report.linked_count),
            command: None,
            path: Some(PathBuf::from("projects")),
            exit_code: None,
            stdout_tail: None,
            stderr_tail: None,
        }
    } else {
        ReleaseCheck {
            id: "release.review.project_workspaces",
            kind: ReleaseCheckKind::Review,
            status: ReleaseCheckStatus::Warning,
            message: format!(
                "project workspace catalog passed, but {} workspace git state issue(s) need review before updating parent gitlinks",
                git_issues.len()
            ),
            details: project_workspace_review_details(report, &git_issues, project),
            count: Some(report.linked_count),
            command: None,
            path: Some(PathBuf::from("projects")),
            exit_code: None,
            stdout_tail: None,
            stderr_tail: None,
        }
    }
}

fn invalid_project_workspace_check(
    report: ProjectWorkspaceCatalog,
    project: Option<&str>,
) -> ReleaseCheck {
    let count = report.issues.len();
    let status = if !report.project_config_valid || project.is_some() {
        ReleaseCheckStatus::Failed
    } else {
        ReleaseCheckStatus::Warning
    };
    ReleaseCheck {
        id: "release.review.project_workspaces",
        kind: ReleaseCheckKind::Review,
        status,
        message: format!(
            "project workspace catalog is incomplete: {}",
            report.issues.join("; ")
        ),
        details: project_workspace_catalog_issue_details(report.issues, project),
        count: Some(count),
        command: None,
        path: Some(PathBuf::from("projects")),
        exit_code: None,
        stdout_tail: None,
        stderr_tail: None,
    }
}

fn project_workspace_catalog_issue_details(
    issues: Vec<String>,
    project: Option<&str>,
) -> Vec<String> {
    let mut details = project_workspace_filter_details(project);
    details.extend(issues);
    details
}

fn project_workspace_review_details(
    report: &ProjectWorkspaceCatalog,
    git_issues: &[String],
    project: Option<&str>,
) -> Vec<String> {
    let mut details = project_workspace_filter_details(project);
    details.extend(git_issues.iter().cloned());
    for workspace in report.workspaces.iter().filter(|workspace| {
        workspace.git_dirty == Some(true)
            || workspace.parent_gitlink_changed == Some(true)
            || workspace.git_status_error.is_some()
    }) {
        if let Some(command) = &workspace.git_status_command {
            details.push(format!(
                "{} inspect command: {}",
                workspace.label,
                command.join(" ")
            ));
        }
        if workspace.git_dirty == Some(true) {
            if let Some(command) = &workspace.git_stage_command {
                details.push(format!(
                    "{} child stage command: {}",
                    workspace.label,
                    command.join(" ")
                ));
            }
            if let Some(command) = &workspace.git_commit_command {
                details.push(format!(
                    "{} child commit command: {}",
                    workspace.label,
                    command.join(" ")
                ));
            }
            if let Some(command) = &workspace.git_push_command {
                details.push(format!(
                    "{} child push command: {}",
                    workspace.label,
                    command.join(" ")
                ));
            }
        }
        if workspace.parent_gitlink_changed == Some(true)
            && let Some(command) = &workspace.parent_gitlink_stage_command
        {
            details.push(format!(
                "{} parent gitlink stage command: {}",
                workspace.label,
                command.join(" ")
            ));
        }
    }
    details
}

fn project_workspace_filter_details(project: Option<&str>) -> Vec<String> {
    project
        .map(|project| vec![format!("project filter: {project}")])
        .unwrap_or_default()
}
