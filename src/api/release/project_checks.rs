use super::utils::summarize_messages;
use super::{ApiResult, ApiService, ReleaseCheck, ReleaseCheckKind, ReleaseCheckStatus};
use crate::api::{
    ApiError, ProjectWorkspaceCatalog, ProjectWorkspaceOptions, WorkflowPublishOptions, loop_check,
};

pub(super) fn source_change_review_check(service: &ApiService) -> ApiResult<ReleaseCheck> {
    let report = service.local_loop_changes()?;
    if !report.issues.is_empty() {
        let count = report.issues.len();
        return Ok(ReleaseCheck {
            id: "release.review.workflow_change_skills",
            kind: ReleaseCheckKind::Review,
            status: ReleaseCheckStatus::Failed,
            message: format!(
                "source-change safety could not be inspected: {}",
                report.issues.join("; ")
            ),
            details: report.issues,
            count: Some(count),
            command: None,
            path: None,
            exit_code: None,
            stdout_tail: None,
            stderr_tail: None,
        });
    }
    if !report.blockers.is_empty() {
        let detail = summarize_messages(&report.blockers, 3);
        return Ok(ReleaseCheck {
            id: "release.review.workflow_change_skills",
            kind: ReleaseCheckKind::Review,
            status: ReleaseCheckStatus::Failed,
            message: format!(
                "workflow source changes need colocated agent skill updates: {}",
                detail
            ),
            details: report.blockers,
            count: Some(report.failed),
            command: None,
            path: None,
            exit_code: None,
            stdout_tail: None,
            stderr_tail: None,
        });
    }
    let (status, message) = if report.warnings > 0 {
        (
            ReleaseCheckStatus::Warning,
            format!(
                "source-change safety has {} warning workflow change(s) and no blockers: {}",
                report.warnings,
                summarize_messages(&report.warning_messages, 3)
            ),
        )
    } else {
        (
            ReleaseCheckStatus::Passed,
            format!(
                "source-change safety passed with {} passed, {} warning, and {} failed workflow change(s)",
                report.passed, report.warnings, report.failed
            ),
        )
    };
    Ok(ReleaseCheck {
        id: "release.review.workflow_change_skills",
        kind: ReleaseCheckKind::Review,
        status,
        message,
        command: None,
        details: if report.warnings > 0 {
            report.warning_messages
        } else {
            Vec::new()
        },
        count: Some(if report.warnings > 0 {
            report.warnings
        } else {
            report.passed
        }),
        path: None,
        exit_code: None,
        stdout_tail: None,
        stderr_tail: None,
    })
}

pub(super) fn project_workspace_review_check(
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
                path: Some(std::path::PathBuf::from("projects")),
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
            path: Some(std::path::PathBuf::from("projects")),
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
        Ok(report) if report.valid => {
            let git_issues = loop_check::project_git_status_issues(&report);
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
                    path: Some(std::path::PathBuf::from("projects")),
                    exit_code: None,
                    stdout_tail: None,
                    stderr_tail: None,
                }
            } else {
                let details = project_workspace_review_details(&report, &git_issues, project);
                ReleaseCheck {
                    id: "release.review.project_workspaces",
                    kind: ReleaseCheckKind::Review,
                    status: ReleaseCheckStatus::Warning,
                    message: format!(
                        "project workspace catalog passed, but {} workspace git state issue(s) need review before updating parent gitlinks",
                        git_issues.len()
                    ),
                    details,
                    count: Some(report.linked_count),
                    command: None,
                    path: Some(std::path::PathBuf::from("projects")),
                    exit_code: None,
                    stdout_tail: None,
                    stderr_tail: None,
                }
            }
        }
        Ok(report) => {
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
                path: Some(std::path::PathBuf::from("projects")),
                exit_code: None,
                stdout_tail: None,
                stderr_tail: None,
            }
        }
        Err(error) => ReleaseCheck {
            id: "release.review.project_workspaces",
            kind: ReleaseCheckKind::Review,
            status: ReleaseCheckStatus::Failed,
            message: format!("project workspace catalog could not be inspected: {error}"),
            details: vec![error.to_string()],
            count: None,
            command: None,
            path: Some(std::path::PathBuf::from("projects")),
            exit_code: None,
            stdout_tail: None,
            stderr_tail: None,
        },
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

pub(super) fn workflow_publish_review_check(
    service: &ApiService,
    project: Option<&str>,
) -> ApiResult<ReleaseCheck> {
    let report = service.workflow_publish_checks_with_options(&WorkflowPublishOptions {
        project: project.map(ToOwned::to_owned),
    })?;
    let details = if let Some(project) = project {
        let matched = report.matched_project_workspace.as_deref().ok_or_else(|| {
            ApiError::InvalidRequest(format!(
                "project workspace filter matched no workspace: {project}"
            ))
        })?;
        vec![
            format!("project filter: {project}"),
            format!("workspace: projects/{matched}"),
        ]
    } else {
        Vec::new()
    };
    let total = report.checks.len();
    let blocked = report
        .checks
        .iter()
        .filter(|check| !check.publishable)
        .collect::<Vec<_>>();
    if blocked.is_empty() && total > 0 {
        return Ok(ReleaseCheck {
            id: "release.review.workflow_publish_ready",
            kind: ReleaseCheckKind::Review,
            status: ReleaseCheckStatus::Passed,
            message: format!(
                "workflow publish readiness passed for {} workflow crate(s)",
                total
            ),
            details,
            count: Some(total),
            command: None,
            path: None,
            exit_code: None,
            stdout_tail: None,
            stderr_tail: None,
        });
    }

    let mut failed_details = details;
    failed_details.extend(blocked.iter().flat_map(|check| {
        check
            .issues
            .iter()
            .map(|issue| format!("{}: {issue}", check.workflow_id))
            .collect::<Vec<_>>()
    }));
    Ok(ReleaseCheck {
        id: "release.review.workflow_publish_ready",
        kind: ReleaseCheckKind::Review,
        status: ReleaseCheckStatus::Failed,
        message: format!(
            "{} of {} workflow crate(s) are not publishable yet: {}",
            blocked.len(),
            total,
            failed_details.join("; ")
        ),
        details: failed_details,
        count: Some(blocked.len()),
        command: None,
        path: None,
        exit_code: None,
        stdout_tail: None,
        stderr_tail: None,
    })
}
