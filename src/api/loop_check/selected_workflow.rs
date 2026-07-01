use super::check_messages::{patch_validation_summary, summarize_messages};
use super::loop_report::run_includes_workflow;
use super::selected_publish::push_selected_publish_check;
use super::{ApiService, LocalLoopCheck, LocalLoopStatus};
use crate::api::WorkflowPlan;
use std::collections::BTreeSet;

pub(super) fn push_selected_workflow_checks(
    service: &ApiService,
    workflow_id: &str,
    checks: &mut Vec<LocalLoopCheck>,
) {
    let workflow = match service.get_workflow(workflow_id) {
        Ok(workflow) => {
            checks.push(LocalLoopCheck::passed(
                "loop.selected.exists",
                format!("workflow {workflow_id} is discoverable"),
            ));
            workflow
        }
        Err(error) => {
            checks.push(LocalLoopCheck::failed(
                "loop.selected.exists",
                format!("workflow {workflow_id} is not discoverable: {error}"),
            ));
            return;
        }
    };

    let validation = service.validate_workflow(&workflow);
    if validation.valid {
        checks.push(LocalLoopCheck::passed(
            "loop.selected.validation",
            "selected workflow validates",
        ));
    } else {
        checks.push(LocalLoopCheck::failed(
            "loop.selected.validation",
            format!(
                "selected workflow validation failed: {}",
                validation.issues.join("; ")
            ),
        ));
    }

    match service.workflow_dependencies(workflow_id) {
        Ok(report) if report.complete => checks.push(
            LocalLoopCheck::passed(
                "loop.selected.dependencies",
                "selected workflow dependencies are complete",
            )
            .count(report.workflow_order.len()),
        ),
        Ok(report) => checks.push(LocalLoopCheck::failed(
            "loop.selected.dependencies",
            format!(
                "selected workflow dependencies are incomplete: missing={}, mismatches={}, cycles={}",
                report.missing_workflows.len(),
                report.version_mismatches.len(),
                report.cycles.len()
            ),
        )),
        Err(error) => checks.push(LocalLoopCheck::failed(
            "loop.selected.dependencies",
            format!("selected workflow dependencies failed: {error}"),
        )),
    }

    match service.plan_workflow(workflow_id) {
        Ok(plan) => {
            checks.push(LocalLoopCheck::passed(
                "loop.selected.plan",
                "selected workflow execution plan can be built",
            ));
            let unavailable = unavailable_plan_executors(&plan);
            if unavailable.is_empty() {
                checks.push(LocalLoopCheck::passed(
                    "loop.selected.executors",
                    "selected workflow planned executors are available",
                ));
            } else {
                checks.push(LocalLoopCheck::failed(
                    "loop.selected.executors",
                    format!(
                        "selected workflow has unavailable planned executors: {}",
                        unavailable.join(", ")
                    ),
                ));
            }
        }
        Err(error) => checks.push(LocalLoopCheck::failed(
            "loop.selected.plan",
            format!("selected workflow execution plan failed: {error}"),
        )),
    }

    push_selected_run_history_checks(service, workflow_id, checks);
    push_selected_model_readiness_check(service, workflow_id, checks);
    push_selected_patch_registry_check(service, workflow_id, checks);
    push_selected_publish_check(service, workflow_id, checks);
}

fn push_selected_patch_registry_check(
    service: &ApiService,
    workflow_id: &str,
    checks: &mut Vec<LocalLoopCheck>,
) {
    match service.list_patches() {
        Ok(catalog) if catalog.patches.is_empty() => checks.push(LocalLoopCheck::passed(
            "loop.selected.patches",
            "no saved patches require selected workflow preflight",
        )),
        Ok(catalog) => {
            let mut incompatible = Vec::new();
            for patch in &catalog.patches {
                match service.get_patch(&patch.name) {
                    Ok(registered) => {
                        let validation =
                            service.validate_patch_for_workflow(workflow_id, registered.patch);
                        if !validation.valid {
                            incompatible
                                .push(patch_validation_summary(&patch.name, &validation.issues));
                        }
                    }
                    Err(error) => incompatible.push(format!("{} ({error})", patch.name)),
                }
            }
            if incompatible.is_empty() {
                checks.push(
                    LocalLoopCheck::passed(
                        "loop.selected.patches",
                        format!("saved patches are compatible with {workflow_id}"),
                    )
                    .count(catalog.patches.len()),
                );
            } else {
                checks.push(
                    LocalLoopCheck::warning(
                        "loop.selected.patches",
                        format!(
                            "{} saved patches are not compatible with {workflow_id}: {}",
                            incompatible.len(),
                            summarize_messages(&incompatible, 3)
                        ),
                    )
                    .count(incompatible.len()),
                );
            }
        }
        Err(error) => checks.push(LocalLoopCheck::warning(
            "loop.selected.patches",
            format!("could not inspect saved patches for {workflow_id}: {error}"),
        )),
    }
}

fn push_selected_model_readiness_check(
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

fn push_selected_run_history_checks(
    service: &ApiService,
    workflow_id: &str,
    checks: &mut Vec<LocalLoopCheck>,
) {
    match service.list_runs() {
        Ok(catalog) => {
            if !catalog.issues.is_empty() {
                checks.push(
                    LocalLoopCheck::warning(
                        "loop.selected.history.catalog",
                        format!(
                            "run history has {} non-fatal issue(s): {}",
                            catalog.issues.len(),
                            catalog.issues.join("; ")
                        ),
                    )
                    .count(catalog.issues.len()),
                );
            }
            let runs = catalog
                .runs
                .iter()
                .filter(|run| run_includes_workflow(run, workflow_id))
                .collect::<Vec<_>>();
            if runs.is_empty() {
                checks.push(LocalLoopCheck::warning(
                    "loop.selected.history",
                    format!("no recorded runs found for {workflow_id}; run it before trace/replay"),
                ));
                checks.push(LocalLoopCheck::warning(
                    "loop.selected.replay",
                    format!(
                        "no completed recorded run found for {workflow_id}; replay is not ready yet"
                    ),
                ));
                return;
            }

            checks.push(
                LocalLoopCheck::passed(
                    "loop.selected.history",
                    format!("recorded runs exist for {workflow_id}"),
                )
                .count(runs.len()),
            );

            if runs.iter().any(|run| run.status == "completed") {
                checks.push(LocalLoopCheck::passed(
                    "loop.selected.replay",
                    format!("a completed recorded run exists for {workflow_id}"),
                ));
            } else {
                checks.push(LocalLoopCheck::warning(
                    "loop.selected.replay",
                    format!("recorded runs exist for {workflow_id}, but none are completed"),
                ));
            }
        }
        Err(error) => checks.push(LocalLoopCheck::warning(
            "loop.selected.history",
            format!("could not inspect run history for {workflow_id}: {error}"),
        )),
    }
}

pub(super) fn push_selected_replay_required_check(
    workflow_id: &str,
    checks: &mut Vec<LocalLoopCheck>,
) {
    let replay_ready = checks
        .iter()
        .any(|check| check.id == "loop.selected.replay" && check.status == LocalLoopStatus::Passed);
    if replay_ready {
        checks.push(LocalLoopCheck::passed(
            "loop.selected.replay.required",
            format!("selected workflow {workflow_id} has replay evidence for release readiness"),
        ));
    } else {
        checks.push(LocalLoopCheck::failed(
            "loop.selected.replay.required",
            format!(
                "selected workflow {workflow_id} needs a completed recorded run before release readiness can pass"
            ),
        ));
    }
}

fn unavailable_plan_executors(plan: &WorkflowPlan) -> Vec<String> {
    let mut unavailable = Vec::new();
    if let Some(runtime) = &plan.runtime
        && !runtime.executor_available
    {
        unavailable.push(format!(
            "{} ({})",
            runtime.executor_id, runtime.executor_status_reason
        ));
    }
    for node in &plan.nodes {
        if let Some(runtime) = &node.runtime
            && !runtime.executor_available
        {
            unavailable.push(format!(
                "{}:{} ({})",
                node.node_id, runtime.executor_id, runtime.executor_status_reason
            ));
        }
    }
    unavailable
}
