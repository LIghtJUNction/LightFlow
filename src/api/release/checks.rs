use super::{ReleaseCheck, ReleaseCheckKind, ReleaseCheckStatus};
use crate::api::ApiError;

pub(super) fn review_error_check(
    id: &'static str,
    message: &'static str,
    error: ApiError,
    path: Option<std::path::PathBuf>,
) -> ReleaseCheck {
    ReleaseCheck {
        id,
        kind: ReleaseCheckKind::Review,
        status: ReleaseCheckStatus::Failed,
        message: format!("{message}: {error}"),
        details: vec![error.to_string()],
        count: None,
        command: None,
        path,
        exit_code: None,
        stdout_tail: None,
        stderr_tail: None,
    }
}

pub(super) fn release_issues(checks: &[ReleaseCheck]) -> Vec<String> {
    checks
        .iter()
        .filter(|check| check.status == ReleaseCheckStatus::Failed)
        .map(|check| format!("{}: {}", check.id, check.message))
        .collect()
}

pub(super) fn release_warnings(checks: &[ReleaseCheck]) -> Vec<String> {
    checks
        .iter()
        .filter(|check| check.status == ReleaseCheckStatus::Warning)
        .map(|check| format!("{}: {}", check.id, check.message))
        .collect()
}

pub(super) fn release_check_count(checks: &[ReleaseCheck], status: ReleaseCheckStatus) -> usize {
    checks.iter().filter(|check| check.status == status).count()
}
