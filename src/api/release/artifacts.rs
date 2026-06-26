use std::path::{Path, PathBuf};

use super::{ReleaseCheck, ReleaseCheckKind, ReleaseCheckStatus};
use crate::api::{ApiError, ApiResult};

pub(super) fn release_artifacts() -> Vec<(&'static str, &'static str)> {
    vec![
        ("release.artifact.changelog", "CHANGELOG.md"),
        ("release.artifact.v0_2_checklist", "docs/v0.2-checklist.md"),
        (
            "release.artifact.runtime_verification",
            "docs/runtime-verification.md",
        ),
        (
            "release.artifact.local_workflow_loop",
            "docs/local-workflow-loop.md",
        ),
    ]
}

pub(super) fn release_document_checks()
-> Vec<(&'static str, &'static str, &'static str, &'static str)> {
    vec![
        (
            "release.document.changelog_cli",
            "CHANGELOG.md",
            "### CLI",
            "CHANGELOG.md records CLI changes",
        ),
        (
            "release.document.changelog_api",
            "CHANGELOG.md",
            "### API",
            "CHANGELOG.md records API changes",
        ),
        (
            "release.document.changelog_workflows",
            "CHANGELOG.md",
            "### Workflows",
            "CHANGELOG.md records workflow changes",
        ),
        (
            "release.document.changelog_runtime",
            "CHANGELOG.md",
            "### Runtime",
            "CHANGELOG.md records runtime changes",
        ),
        (
            "release.document.changelog_known_limitations",
            "CHANGELOG.md",
            "### Known Limitations",
            "CHANGELOG.md documents known limitations",
        ),
        (
            "release.document.changelog_migration_notes",
            "CHANGELOG.md",
            "### Migration Notes",
            "CHANGELOG.md documents migration notes",
        ),
        (
            "release.document.local_workflow_loop",
            "docs/local-workflow-loop.md",
            "## Verification Gates",
            "docs/local-workflow-loop.md records local workflow loop verification gates",
        ),
    ]
}

pub(super) fn artifact_check(
    root: &Path,
    id: &'static str,
    relative_path: &'static str,
) -> ReleaseCheck {
    let path = PathBuf::from(relative_path);
    if root.join(&path).exists() {
        ReleaseCheck {
            id,
            kind: ReleaseCheckKind::Artifact,
            status: ReleaseCheckStatus::Passed,
            message: format!("required release artifact exists: {relative_path}"),
            details: Vec::new(),
            count: Some(1),
            command: None,
            path: Some(path),
            exit_code: None,
            stdout_tail: None,
            stderr_tail: None,
        }
    } else {
        ReleaseCheck {
            id,
            kind: ReleaseCheckKind::Artifact,
            status: ReleaseCheckStatus::Failed,
            message: format!("required release artifact is missing: {relative_path}"),
            details: Vec::new(),
            count: Some(1),
            command: None,
            path: Some(path),
            exit_code: None,
            stdout_tail: None,
            stderr_tail: None,
        }
    }
}

pub(super) fn document_check(
    root: &Path,
    id: &'static str,
    relative_path: &'static str,
    needle: &'static str,
    description: &'static str,
) -> ApiResult<ReleaseCheck> {
    let path = PathBuf::from(relative_path);
    let source = match std::fs::read_to_string(root.join(&path)) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(ReleaseCheck {
                id,
                kind: ReleaseCheckKind::Document,
                status: ReleaseCheckStatus::Failed,
                message: format!("required release document is missing: {relative_path}"),
                details: Vec::new(),
                count: Some(1),
                command: None,
                path: Some(path),
                exit_code: None,
                stdout_tail: None,
                stderr_tail: None,
            });
        }
        Err(error) => return Err(ApiError::from(error)),
    };
    if source.contains(needle) {
        Ok(ReleaseCheck {
            id,
            kind: ReleaseCheckKind::Document,
            status: ReleaseCheckStatus::Passed,
            message: description.to_owned(),
            details: Vec::new(),
            count: Some(1),
            command: None,
            path: Some(path),
            exit_code: None,
            stdout_tail: None,
            stderr_tail: None,
        })
    } else {
        Ok(ReleaseCheck {
            id,
            kind: ReleaseCheckKind::Document,
            status: ReleaseCheckStatus::Failed,
            message: format!("{relative_path} is missing required section {needle}"),
            details: Vec::new(),
            count: Some(1),
            command: None,
            path: Some(path),
            exit_code: None,
            stdout_tail: None,
            stderr_tail: None,
        })
    }
}
