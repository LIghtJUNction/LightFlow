use super::{CliError, CliResult, ensure_no_extra_args};
use crate::api::{ApiService, ProjectWorkspaceOptions};
use std::path::Path;

pub(super) fn manage_loop(service: &ApiService, args: &[String]) -> CliResult<serde_json::Value> {
    let action = match args.first().map(String::as_str) {
        Some("-h" | "--help" | "help") | None => {
            return Err(CliError::Usage(loop_usage()));
        }
        Some(action) => action,
    };
    match action {
        "check" => {
            let (workflow_id, require_selected_replay) = parse_loop_check_args(args)?;
            let value = if require_selected_replay {
                serde_json::to_value(
                    service.local_loop_check_with_options(workflow_id, require_selected_replay)?,
                )?
            } else {
                loop_check_report(service, workflow_id)?
            };
            let valid = value
                .get("valid")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false);
            if valid {
                Ok(value)
            } else {
                Err(CliError::Usage(value.to_string()))
            }
        }
        "changes" => {
            if args
                .get(1)
                .is_some_and(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
            {
                return Err(CliError::Usage(loop_usage()));
            }
            ensure_no_extra_args(args, 1, "loop changes")?;
            let report = service.local_loop_changes()?;
            let valid = report.valid;
            let value = serde_json::to_value(report)?;
            if valid {
                Ok(value)
            } else {
                Err(CliError::Usage(value.to_string()))
            }
        }
        "projects" => {
            let options = parse_loop_projects_args(args)?;
            let report = service.project_workspaces_with_options(ProjectWorkspaceOptions {
                dirty_only: options.dirty_only,
                project: options.project,
            })?;
            let valid = report.valid;
            let value = serde_json::to_value(report)?;
            if valid {
                Ok(value)
            } else {
                Err(CliError::Usage(value.to_string()))
            }
        }
        _ => Err(CliError::Usage(format!(
            "loop action must be check, changes, or projects\n{}",
            loop_usage()
        ))),
    }
}

#[derive(Debug, Default, Clone, Eq, PartialEq)]
struct LoopProjectsOptions {
    dirty_only: bool,
    project: Option<String>,
}

fn parse_loop_projects_args(args: &[String]) -> CliResult<LoopProjectsOptions> {
    let mut options = LoopProjectsOptions::default();
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--dirty" | "--changed" => {
                options.dirty_only = true;
                index += 1;
            }
            "--project" => {
                options.project = Some(required_loop_project_value(args, index)?.to_owned());
                index += 2;
            }
            "-h" | "--help" | "help" => return Err(CliError::Usage(loop_usage())),
            value if value.starts_with('-') => {
                return Err(CliError::Usage(loop_usage()));
            }
            value => {
                return Err(CliError::Usage(format!(
                    "unexpected argument for loop projects: {value}"
                )));
            }
        }
    }
    Ok(options)
}

fn required_loop_project_value(args: &[String], index: usize) -> CliResult<&str> {
    let Some(value) = args.get(index + 1).map(String::as_str) else {
        return Err(CliError::Usage(loop_usage()));
    };
    if value.starts_with('-') {
        return Err(CliError::Usage(loop_usage()));
    }
    Ok(value)
}

fn parse_loop_check_args(args: &[String]) -> CliResult<(Option<&str>, bool)> {
    let mut workflow_id = None;
    let mut require_selected_replay = false;
    for arg in args.iter().skip(1) {
        match arg.as_str() {
            "-h" | "--help" | "help" => return Err(CliError::Usage(loop_usage())),
            "--require-replay" | "--require-selected-replay" => {
                require_selected_replay = true;
            }
            value if value.starts_with('-') => {
                return Err(CliError::Usage(loop_usage()));
            }
            value if workflow_id.is_none() => {
                workflow_id = Some(value);
            }
            value => {
                return Err(CliError::Usage(format!(
                    "unexpected argument for loop check: {value}"
                )));
            }
        }
    }
    if require_selected_replay && workflow_id.is_none() {
        return Err(CliError::Usage(loop_usage()));
    }
    Ok((workflow_id, require_selected_replay))
}

fn loop_usage() -> String {
    [
        "usage:",
        "  lfw loop check [workflow_id] [--require-replay]",
        "  lfw loop changes",
        "  lfw loop projects [--dirty] [--project <name>]",
        "",
        "Reports local workflow-loop readiness without mutating project files.",
        "--dirty narrows project workspace output to dirty child repos or changed parent gitlinks.",
        "--project accepts full names, paths, labels, or lightflow-* short aliases such as std, flux, rig, or custom-tools.",
        "--require-replay requires a selected workflow id and completed-run replay evidence.",
    ]
    .join("\n")
}

pub(crate) fn loop_check_report(
    service: &ApiService,
    workflow_id: Option<&str>,
) -> CliResult<serde_json::Value> {
    serde_json::to_value(service.local_loop_check(workflow_id)?).map_err(CliError::from)
}

pub(crate) fn ensure_loop_changes_valid(root: &Path) -> CliResult<()> {
    let report = ApiService::new(root.to_path_buf()).local_loop_changes()?;
    if report.valid {
        return Ok(());
    }
    Err(CliError::Usage(serde_json::to_value(report)?.to_string()))
}
