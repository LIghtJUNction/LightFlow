use crate::api::ApiService;
use crate::api::loop_check::LocalLoopCheck;
use crate::api::loop_check::check_messages::summarize_messages;
use std::collections::BTreeSet;

pub(super) fn push_selected_model_readiness_check(
    service: &ApiService,
    workflow_id: &str,
    checks: &mut Vec<LocalLoopCheck>,
) {
    let workflow_ids = match service.workflow_dependencies(workflow_id) {
        Ok(report) => report.workflow_order.into_iter().collect::<BTreeSet<_>>(),
        Err(error) => {
            checks.push(LocalLoopCheck::warning(
                "loop.selected.models",
                format!("selected workflow model dependency scope could not be inspected: {error}"),
            ));
            return;
        }
    };

    match service.list_models() {
        Ok(catalog) => {
            let selected = catalog
                .models
                .iter()
                .filter(|model| workflow_ids.contains(&model.workflow_id))
                .collect::<Vec<_>>();
            if selected.is_empty() {
                checks.push(LocalLoopCheck::passed(
                    "loop.selected.models",
                    format!(
                        "selected workflow {workflow_id} dependency graph declares no model requirements"
                    ),
                ));
                return;
            }

            let issues = selected
                .iter()
                .filter(|model| model.lock.status.as_str() != "available")
                .map(|model| {
                    format!(
                        "{}: model lock is {}",
                        model.lock.key,
                        model.lock.status.as_str()
                    )
                })
                .collect::<Vec<_>>();
            if issues.is_empty() {
                checks.push(
                    LocalLoopCheck::passed(
                        "loop.selected.models",
                        format!(
                            "selected workflow {workflow_id} dependency graph model locks are ready"
                        ),
                    )
                    .count(selected.len()),
                );
            } else {
                checks.push(
                    LocalLoopCheck::warning(
                        "loop.selected.models",
                        format!(
                            "{} of {} selected model requirement(s) not ready: {}",
                            issues.len(),
                            selected.len(),
                            summarize_messages(&issues, 3)
                        ) + &format!(
                            "; inspect `lfw models requirements {workflow_id} --blocked`, then run `lfw sync {workflow_id} --auto-model --apply` or `lfw sync {workflow_id} --locked --apply`"
                        ),
                    )
                    .count(issues.len()),
                );
            }
        }
        Err(error) => checks.push(LocalLoopCheck::warning(
            "loop.selected.models",
            format!("selected workflow model readiness could not be inspected: {error}"),
        )),
    }
}
