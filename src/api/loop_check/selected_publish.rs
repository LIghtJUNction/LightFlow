use super::check_messages::summarize_messages;
use super::{ApiError, ApiService, LocalLoopCheck};

pub(super) fn push_selected_publish_check(
    service: &ApiService,
    workflow_id: &str,
    checks: &mut Vec<LocalLoopCheck>,
) {
    let workflow_ids = match service.workflow_dependencies(workflow_id) {
        Ok(report) => report.workflow_order,
        Err(error) => {
            checks.push(LocalLoopCheck::warning(
                "loop.selected.publish",
                format!(
                    "selected workflow publish dependency scope could not be inspected: {error}"
                ),
            ));
            return;
        }
    };

    let mut checked = Vec::new();
    let mut blockers = Vec::new();
    let mut skipped = Vec::new();
    let mut publish_plan_count = 0;
    for dependency_workflow_id in &workflow_ids {
        match service.workflow_publish_check(dependency_workflow_id) {
            Ok(check) => {
                publish_plan_count += 1;
                if !check.publishable {
                    let issues = if check.issues.is_empty() {
                        "publish dry-run reported the crate is not publishable".to_owned()
                    } else {
                        check.issues.join("; ")
                    };
                    blockers.push(format!("{}: {issues}", check.workflow_id));
                }
                checked.push(check);
            }
            Err(ApiError::NotFound(_)) => skipped.push(dependency_workflow_id.clone()),
            Err(error) => {
                publish_plan_count += 1;
                blockers.push(format!("{dependency_workflow_id}: {error}"));
            }
        }
    }

    if checked.is_empty() && blockers.is_empty() {
        checks.push(LocalLoopCheck::passed(
            "loop.selected.publish",
            format!(
                "selected workflow {workflow_id} dependency graph has no local workflow crates to publish"
            ),
        ));
        return;
    }

    if blockers.is_empty() {
        let path = checked
            .iter()
            .find(|check| check.workflow_id == workflow_id)
            .map(|check| check.manifest.clone());
        let mut check = LocalLoopCheck::passed(
            "loop.selected.publish",
            format!(
                "selected workflow {workflow_id} dependency graph has publishable local crates"
            ),
        )
        .count(checked.len());
        check.path = path;
        checks.push(check);
    } else {
        let path = checked
            .iter()
            .find(|check| check.workflow_id == workflow_id)
            .map(|check| check.manifest.clone());
        let mut check = LocalLoopCheck::warning(
            "loop.selected.publish",
            format!(
                "{} of {} selected workflow publish plan(s) blocked: {}",
                blockers.len(),
                publish_plan_count,
                summarize_messages(&blockers, 3)
            ),
        )
        .count(blockers.len());
        check.path = path;
        checks.push(check);
    }
}

pub(super) fn selected_local_publish_plan_count(
    service: &ApiService,
    workflow_id: &str,
) -> Option<usize> {
    let report = service.workflow_dependencies(workflow_id).ok()?;
    Some(
        report
            .workflow_order
            .iter()
            .filter(|dependency_workflow_id| {
                service
                    .workflow_publish_check(dependency_workflow_id)
                    .is_ok()
            })
            .count(),
    )
}
