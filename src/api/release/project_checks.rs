use super::utils::summarize_messages;
use super::{ApiResult, ApiService, ReleaseCheck, ReleaseCheckKind, ReleaseCheckStatus};
use crate::api::{ApiError, WorkflowPublishOptions};

mod workspaces;
pub(super) use workspaces::project_workspace_review_check;

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
