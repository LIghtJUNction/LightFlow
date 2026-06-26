use super::check_messages::{patch_validation_summary, summarize_messages};
use super::{ApiService, LocalLoopCheck};
use crate::api::RunCatalog;
use std::path::PathBuf;

pub(super) fn push_workflow_discovery_check(
    service: &ApiService,
    checks: &mut Vec<LocalLoopCheck>,
) {
    match service.list_workflows() {
        Ok(workflows) if workflows.workflows.is_empty() => checks.push(LocalLoopCheck::failed(
            "loop.workflow.discovery",
            "no workflows are discoverable from this project",
        )),
        Ok(workflows) => checks.push(
            LocalLoopCheck::passed(
                "loop.workflow.discovery",
                "workflows are discoverable from this project",
            )
            .count(workflows.workflows.len()),
        ),
        Err(error) => checks.push(LocalLoopCheck::failed(
            "loop.workflow.discovery",
            format!("workflow discovery failed: {error}"),
        )),
    }
}

pub(super) fn push_executor_check(service: &ApiService, checks: &mut Vec<LocalLoopCheck>) {
    let executors = service.list_executors().executors;
    let has_passthrough = executors
        .iter()
        .any(|executor| executor.id == "passthrough" && executor.available);
    if executors.is_empty() {
        checks.push(LocalLoopCheck::failed(
            "loop.executor.catalog",
            "executor catalog is empty",
        ));
    } else if has_passthrough {
        checks.push(
            LocalLoopCheck::passed(
                "loop.executor.catalog",
                "executor catalog is available and includes passthrough",
            )
            .count(executors.len()),
        );
    } else {
        checks.push(
            LocalLoopCheck::warning(
                "loop.executor.catalog",
                "executor catalog is available but passthrough is not available",
            )
            .count(executors.len()),
        );
    }
}

pub(super) fn push_model_readiness_check(service: &ApiService, checks: &mut Vec<LocalLoopCheck>) {
    match service.list_models() {
        Ok(catalog) if catalog.total == 0 => checks.push(LocalLoopCheck::passed(
            "loop.models.readiness",
            "no model requirements are declared by discovered workflows",
        )),
        Ok(catalog) if catalog.blocked_count == 0 => {
            checks.push(
                LocalLoopCheck::passed(
                    "loop.models.readiness",
                    "all declared model requirements have available locked paths",
                )
                .count(catalog.total),
            );
        }
        Ok(catalog) => {
            checks.push(
                LocalLoopCheck::warning(
                    "loop.models.readiness",
                    format!(
                        "{} of {} model requirement(s) not ready: {}",
                        catalog.blocked_count,
                        catalog.total,
                        summarize_messages(&catalog.issues, 3)
                    ) + "; run `lfw sync <workflow_id> --auto-model --apply` for a selected workflow or inspect `/models` / `lfw models requirements --blocked` for lock details",
                )
                .count(catalog.blocked_count),
            );
        }
        Err(error) => checks.push(LocalLoopCheck::warning(
            "loop.models.readiness",
            format!("model resource readiness could not be inspected: {error}"),
        )),
    }
}

pub(super) fn push_run_history_check(service: &ApiService, checks: &mut Vec<LocalLoopCheck>) {
    let root = service.repo_root();
    let path = PathBuf::from(".lightflow/runs");
    if !root.join(&path).is_dir() {
        checks.push(
            LocalLoopCheck::warning(
                "loop.history.runs",
                "run history directory is created after the first recorded run",
            )
            .path(path),
        );
        return;
    }

    match service.list_runs() {
        Ok(catalog) if catalog.issues.is_empty() && catalog.unknown_count == 0 => checks.push(
            LocalLoopCheck::passed("loop.history.runs", "run history directory exists")
                .path(path)
                .count(catalog.runs.len()),
        ),
        Ok(catalog) if catalog.issues.is_empty() => checks.push(
            LocalLoopCheck::warning(
                "loop.history.runs",
                format!(
                    "run history has {} unknown-status run(s): {}",
                    catalog.unknown_count,
                    unknown_status_run_sample(&catalog)
                ),
            )
            .path(path)
            .count(catalog.unknown_count),
        ),
        Ok(catalog) => checks.push(
            LocalLoopCheck::warning(
                "loop.history.runs",
                format!(
                    "run history has {} non-fatal issue(s): {}",
                    catalog.issues.len(),
                    catalog.issues.join("; ")
                ),
            )
            .path(path)
            .count(catalog.issues.len()),
        ),
        Err(error) => checks.push(
            LocalLoopCheck::failed(
                "loop.history.runs",
                format!("run history could not be inspected: {error}"),
            )
            .path(path),
        ),
    }
}

fn unknown_status_run_sample(catalog: &RunCatalog) -> String {
    let sample = catalog
        .runs
        .iter()
        .filter(|run| run.status != "completed" && run.status != "failed")
        .take(3)
        .map(|run| run.run_id.as_str())
        .collect::<Vec<_>>();
    if catalog.unknown_count > sample.len() {
        format!(
            "{} and {} more",
            sample.join(", "),
            catalog.unknown_count - sample.len()
        )
    } else {
        sample.join(", ")
    }
}

pub(super) fn push_patch_registry_check(service: &ApiService, checks: &mut Vec<LocalLoopCheck>) {
    let root = service.repo_root();
    let path = PathBuf::from(".lightflow/patches");
    if !root.join(&path).is_dir() {
        checks.push(
            LocalLoopCheck::passed(
                "loop.patches.registry",
                "patch registry directory is created when a patch is saved",
            )
            .path(path),
        );
        return;
    }

    match service.list_patches() {
        Ok(catalog) if catalog.patches.is_empty() => checks.push(
            LocalLoopCheck::passed(
                "loop.patches.registry",
                "patch registry exists but has no saved patches",
            )
            .path(path),
        ),
        Ok(catalog) => {
            let mut invalid = Vec::new();
            for patch in &catalog.patches {
                match service.get_patch(&patch.name) {
                    Ok(registered) => {
                        let validation = service.validate_patch(registered.patch);
                        if !validation.valid {
                            invalid.push(patch_validation_summary(&patch.name, &validation.issues));
                        }
                    }
                    Err(error) => invalid.push(format!("{} ({error})", patch.name)),
                }
            }
            if invalid.is_empty() {
                checks.push(
                    LocalLoopCheck::passed(
                        "loop.patches.registry",
                        "saved patches are readable and valid",
                    )
                    .path(path)
                    .count(catalog.patches.len()),
                );
            } else {
                checks.push(
                    LocalLoopCheck::failed(
                        "loop.patches.registry",
                        format!(
                            "saved patches are invalid: {}",
                            summarize_messages(&invalid, 3)
                        ),
                    )
                    .path(path)
                    .count(invalid.len()),
                );
            }
        }
        Err(error) => checks.push(
            LocalLoopCheck::failed(
                "loop.patches.registry",
                format!("patch registry could not be inspected: {error}"),
            )
            .path(path),
        ),
    }
}
