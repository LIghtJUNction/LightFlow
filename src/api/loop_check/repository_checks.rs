use super::check_messages::summarize_messages;
use super::loop_changes::loop_changes_across_project_set;
use super::project_workspaces::{project_git_status_issues, project_workspaces};
use super::{ApiError, ApiResult, LocalLoopCheck, ProjectWorkspaceCatalog};
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn push_document_checks(root: &Path, checks: &mut Vec<LocalLoopCheck>) -> ApiResult<()> {
    let path = root.join("docs").join("local-workflow-loop.md");
    let relative_path = PathBuf::from("docs/local-workflow-loop.md");
    match fs::read_to_string(&path) {
        Ok(source) => {
            checks.push(
                LocalLoopCheck::passed(
                    "loop.document.local_workflow_loop",
                    "local workflow loop document exists",
                )
                .path(&relative_path),
            );
            if source.contains("## Verification Gates") {
                checks.push(
                    LocalLoopCheck::passed(
                        "loop.document.verification_gates",
                        "local workflow loop records verification gates",
                    )
                    .path(relative_path),
                );
            } else {
                checks.push(
                    LocalLoopCheck::failed(
                        "loop.document.verification_gates",
                        "docs/local-workflow-loop.md is missing ## Verification Gates",
                    )
                    .path(relative_path),
                );
            }
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            checks.push(
                LocalLoopCheck::warning(
                    "loop.document.local_workflow_loop",
                    "docs/local-workflow-loop.md is optional outside the LightFlow core repository",
                )
                .path(relative_path),
            );
        }
        Err(error) => return Err(ApiError::Io(error)),
    }
    Ok(())
}

pub(super) fn push_project_set_check(root: &Path, checks: &mut Vec<LocalLoopCheck>) {
    let Ok(catalog) = project_workspaces(root) else {
        checks.push(
            LocalLoopCheck::warning(
                "loop.projects.sibling_workspaces",
                "projects/ could not be inspected",
            )
            .path(PathBuf::from("projects")),
        );
        return;
    };
    let projects = catalog.projects_dir.clone();
    let relative_path = PathBuf::from("projects");
    if !projects.exists() {
        checks.push(
            LocalLoopCheck::warning(
                "loop.projects.sibling_workspaces",
                "projects/ is optional outside the core multi-repo workspace",
            )
            .path(relative_path),
        );
        return;
    }

    if catalog.valid {
        checks.push(
            LocalLoopCheck::passed(
                "loop.projects.sibling_workspaces",
                "projects/ contains the flux, std, and rig workflow workspaces",
            )
            .path(relative_path)
            .count(catalog.linked_count),
        );
    } else {
        checks.push(
            LocalLoopCheck::warning(
                "loop.projects.sibling_workspaces",
                format!("projects/ is incomplete: {}", catalog.issues.join("; ")),
            )
            .path(relative_path),
        );
    }

    push_project_git_status_check(&catalog, checks);
}

fn push_project_git_status_check(
    catalog: &ProjectWorkspaceCatalog,
    checks: &mut Vec<LocalLoopCheck>,
) {
    let issues = project_git_status_issues(catalog);
    if issues.is_empty() {
        let inspected = catalog
            .workspaces
            .iter()
            .filter(|workspace| workspace.git_dirty.is_some())
            .count();
        checks.push(
            LocalLoopCheck::passed(
                "loop.projects.git_status",
                "project workspace git status is clean",
            )
            .path(PathBuf::from("projects"))
            .count(inspected),
        );
    } else {
        checks.push(
            LocalLoopCheck::warning(
                "loop.projects.git_status",
                format!(
                    "project workspaces need git review before updating parent gitlinks: {}",
                    summarize_messages(&issues, 4)
                ),
            )
            .path(PathBuf::from("projects"))
            .count(issues.len())
            .details(issues),
        );
    }
}

pub(super) fn push_source_change_safety_check(
    root: &Path,
    checks: &mut Vec<LocalLoopCheck>,
) -> ApiResult<()> {
    let report = loop_changes_across_project_set(root)?;
    if !report.issues.is_empty() {
        let message = format!(
            "source-change safety could not be inspected: {}",
            report.issues.join("; ")
        );
        let check = if source_change_inspection_failure_is_fatal(root, &report.issues) {
            LocalLoopCheck::failed("loop.source_changes.safety", message)
        } else {
            LocalLoopCheck::warning("loop.source_changes.safety", message)
        };
        checks.push(check);
        return Ok(());
    }

    if report.failed > 0 {
        let detail = summarize_messages(&report.blockers, 3);
        checks.push(
            LocalLoopCheck::failed(
                "loop.source_changes.safety",
                format!(
                    "{} changed workflow(s) are missing colocated agent skill updates: {}",
                    report.failed, detail
                ),
            )
            .count(report.failed),
        );
    } else if report.warnings > 0 {
        let detail = summarize_messages(&report.warning_messages, 3);
        checks.push(
            LocalLoopCheck::warning(
                "loop.source_changes.safety",
                format!(
                    "source-change safety has {} warning workflow change(s) and no blockers: {}",
                    report.warnings, detail
                ),
            )
            .count(report.warnings),
        );
    } else {
        checks.push(
            LocalLoopCheck::passed(
                "loop.source_changes.safety",
                "workflow source changes are paired with colocated agent skill updates",
            )
            .count(report.changed_workflows.len()),
        );
    }
    Ok(())
}

fn source_change_inspection_failure_is_fatal(root: &Path, issues: &[String]) -> bool {
    if issues.iter().any(|issue| issue.starts_with("projects/")) {
        return true;
    }
    if root.join(".git").exists() {
        return true;
    }
    false
}
