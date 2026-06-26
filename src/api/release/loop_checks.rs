use super::{ReleaseCheck, ReleaseCheckKind, ReleaseCheckStatus};
use crate::api::{ApiResult, ApiService, LocalLoopCheck, LocalLoopStatus};

pub(super) fn local_workflow_loop_review_check(
    service: &ApiService,
    project: Option<&str>,
) -> ApiResult<ReleaseCheck> {
    let report = service.local_loop_check(None)?;
    let relevant_checks = report
        .checks
        .iter()
        .filter(|check| release_local_loop_review_includes(check, project))
        .collect::<Vec<_>>();
    let failed = relevant_checks
        .iter()
        .copied()
        .filter(|check| check.status == LocalLoopStatus::Failed)
        .collect::<Vec<_>>();
    if !failed.is_empty() {
        let summaries = local_loop_check_summaries(&failed);
        let details = local_loop_check_details(&failed);
        return Ok(ReleaseCheck {
            id: "release.review.local_workflow_loop",
            kind: ReleaseCheckKind::Review,
            status: ReleaseCheckStatus::Failed,
            message: format!(
                "local workflow loop has {} failed check(s): {}",
                failed.len(),
                summaries.join("; ")
            ),
            details,
            count: Some(release_local_loop_review_count(&failed)),
            command: None,
            path: None,
            exit_code: None,
            stdout_tail: None,
            stderr_tail: None,
        });
    }

    let warnings = relevant_checks
        .iter()
        .copied()
        .filter(|check| check.status == LocalLoopStatus::Warning)
        .collect::<Vec<_>>();
    if !warnings.is_empty() {
        let summaries = local_loop_check_summaries(&warnings);
        let details = local_loop_check_details(&warnings);
        return Ok(ReleaseCheck {
            id: "release.review.local_workflow_loop",
            kind: ReleaseCheckKind::Review,
            status: ReleaseCheckStatus::Warning,
            message: format!(
                "local workflow loop has {} warning check(s): {}",
                warnings.len(),
                summaries.join("; ")
            ),
            details,
            count: Some(release_local_loop_review_count(&warnings)),
            command: None,
            path: None,
            exit_code: None,
            stdout_tail: None,
            stderr_tail: None,
        });
    }

    Ok(ReleaseCheck {
        id: "release.review.local_workflow_loop",
        kind: ReleaseCheckKind::Review,
        status: ReleaseCheckStatus::Passed,
        message: format!(
            "local workflow loop readiness passed for {} reviewed check(s)",
            relevant_checks.len()
        ),
        details: Vec::new(),
        count: Some(relevant_checks.len()),
        command: None,
        path: None,
        exit_code: None,
        stdout_tail: None,
        stderr_tail: None,
    })
}

pub(super) fn release_local_loop_review_includes(
    check: &LocalLoopCheck,
    project: Option<&str>,
) -> bool {
    if project.is_some() && check.id == "loop.projects.git_status" {
        return false;
    }
    !matches!(
        check.id,
        "loop.document.local_workflow_loop"
            | "loop.projects.sibling_workspaces"
            | "loop.source_changes.safety"
            | "loop.publish.workflow_crates"
            | "loop.publish.readiness"
    )
}

pub(super) fn release_local_loop_review_count(checks: &[&LocalLoopCheck]) -> usize {
    checks
        .iter()
        .map(|check| check.count.unwrap_or(1))
        .sum::<usize>()
}

pub(super) fn selected_workflow_loop_review_check(
    service: &ApiService,
    workflow_id: &str,
) -> ApiResult<ReleaseCheck> {
    let report = service.local_loop_check_with_options(Some(workflow_id), true)?;
    let relevant_checks = report
        .checks
        .iter()
        .filter(|check| release_selected_loop_review_includes(check))
        .collect::<Vec<_>>();
    let failed = relevant_checks
        .iter()
        .copied()
        .filter(|check| check.status == LocalLoopStatus::Failed)
        .collect::<Vec<_>>();
    if !failed.is_empty() {
        let summaries = local_loop_check_summaries(&failed);
        let details = local_loop_check_details(&failed);
        return Ok(ReleaseCheck {
            id: "release.review.selected_workflow_loop",
            kind: ReleaseCheckKind::Review,
            status: ReleaseCheckStatus::Failed,
            message: format!(
                "selected workflow {workflow_id} loop has {} failed check(s): {}",
                failed.len(),
                summaries.join("; ")
            ),
            details,
            count: Some(release_local_loop_review_count(&failed)),
            command: None,
            path: None,
            exit_code: None,
            stdout_tail: None,
            stderr_tail: None,
        });
    }

    let warnings = relevant_checks
        .iter()
        .copied()
        .filter(|check| check.status == LocalLoopStatus::Warning)
        .collect::<Vec<_>>();
    if !warnings.is_empty() {
        let summaries = local_loop_check_summaries(&warnings);
        let details = local_loop_check_details(&warnings);
        return Ok(ReleaseCheck {
            id: "release.review.selected_workflow_loop",
            kind: ReleaseCheckKind::Review,
            status: ReleaseCheckStatus::Warning,
            message: format!(
                "selected workflow {workflow_id} loop has {} warning check(s): {}",
                warnings.len(),
                summaries.join("; ")
            ),
            details,
            count: Some(release_local_loop_review_count(&warnings)),
            command: None,
            path: None,
            exit_code: None,
            stdout_tail: None,
            stderr_tail: None,
        });
    }

    Ok(ReleaseCheck {
        id: "release.review.selected_workflow_loop",
        kind: ReleaseCheckKind::Review,
        status: ReleaseCheckStatus::Passed,
        message: format!(
            "selected workflow {workflow_id} loop readiness passed for {} reviewed check(s)",
            relevant_checks.len()
        ),
        details: Vec::new(),
        count: Some(relevant_checks.len()),
        command: None,
        path: None,
        exit_code: None,
        stdout_tail: None,
        stderr_tail: None,
    })
}

pub(super) fn release_selected_loop_review_includes(check: &LocalLoopCheck) -> bool {
    check.id.starts_with("loop.selected.")
}

pub(super) fn local_loop_check_summaries(checks: &[&LocalLoopCheck]) -> Vec<String> {
    checks
        .iter()
        .map(|check| format!("{}: {}", check.id, check.message))
        .collect()
}

pub(super) fn local_loop_check_details(checks: &[&LocalLoopCheck]) -> Vec<String> {
    checks
        .iter()
        .flat_map(|check| {
            if check.details.is_empty() {
                vec![format!("{}: {}", check.id, check.message)]
            } else {
                check
                    .details
                    .iter()
                    .map(|detail| format!("{}: {detail}", check.id))
                    .collect::<Vec<_>>()
            }
        })
        .collect()
}
