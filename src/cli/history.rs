use super::{CliError, CliResult};
use crate::api::{ApiService, RunListOptions};

pub(super) fn trace_run(service: &ApiService, args: &[String]) -> CliResult<serde_json::Value> {
    let selector = optional_run_selector(args.first().map(String::as_str))?;
    if matches!(selector, "-h" | "--help" | "help") {
        return Err(CliError::Usage(runs_usage()));
    }
    if args.len() > 1 {
        return Err(CliError::Usage(format!(
            "unexpected argument for trace: {}",
            args[1]
        )));
    }
    Ok(serde_json::to_value(service.get_run(selector)?)?)
}

pub(super) fn manage_runs(service: &ApiService, args: &[String]) -> CliResult<serde_json::Value> {
    let action = args.first().map(String::as_str).unwrap_or("list");
    match action {
        "list" | "ls" => {
            let options = parse_run_list_options(args)?;
            Ok(serde_json::to_value(
                service.list_runs_with_options(&options)?,
            )?)
        }
        "get" | "show" | "trace" => {
            let run_id = optional_run_selector(args.get(1).map(String::as_str))?;
            ensure_no_history_extra_args(args, 2, "runs get")?;
            trace_run(service, &[run_id.to_owned()])
        }
        "replay" => {
            let run_id = optional_run_selector(args.get(1).map(String::as_str))?;
            if matches!(run_id, "-h" | "--help" | "help") {
                return Err(CliError::Usage(runs_usage()));
            }
            ensure_no_history_extra_args(args, 2, "runs replay")?;
            Ok(service.replay_run_with_surface(run_id, "cli")?)
        }
        "rm" | "remove" | "delete" => {
            let run_id = required_run_selector(args.get(1).map(String::as_str))?;
            if matches!(run_id, "-h" | "--help" | "help") {
                return Err(CliError::Usage(runs_usage()));
            }
            ensure_no_history_extra_args(args, 2, "runs rm")?;
            Ok(serde_json::to_value(service.remove_run(run_id)?)?)
        }
        "-h" | "--help" | "help" => Err(CliError::Usage(runs_usage())),
        _ => Err(CliError::Usage(runs_usage())),
    }
}

pub(super) fn parse_replay_run_id(args: &[String]) -> CliResult<&str> {
    let run_id = optional_run_selector(args.first().map(String::as_str))?;
    if matches!(run_id, "-h" | "--help" | "help") {
        return Err(CliError::Usage(runs_usage()));
    }
    if let Some(extra) = args.get(1) {
        return Err(CliError::Usage(format!(
            "unexpected argument for replay: {extra}"
        )));
    }
    Ok(run_id)
}

fn optional_run_selector(value: Option<&str>) -> CliResult<&str> {
    let value = value.unwrap_or("last");
    if value.starts_with('-') && !matches!(value, "-h" | "--help") {
        return Err(CliError::Usage(runs_usage()));
    }
    if value == "|" {
        return Err(CliError::Usage(runs_usage()));
    }
    Ok(value)
}

fn required_run_selector(value: Option<&str>) -> CliResult<&str> {
    let Some(value) = value else {
        return Err(CliError::Usage(runs_usage()));
    };
    optional_run_selector(Some(value))
}

fn ensure_no_history_extra_args(args: &[String], max_len: usize, command: &str) -> CliResult<()> {
    if let Some(extra) = args.get(max_len) {
        return Err(CliError::Usage(format!(
            "unexpected argument for {command}: {extra}"
        )));
    }
    Ok(())
}

fn parse_run_list_options(args: &[String]) -> CliResult<RunListOptions> {
    let mut options = RunListOptions::default();
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--limit" => {
                let value = required_runs_list_flag_value(args, index)?;
                let limit = value
                    .parse::<usize>()
                    .map_err(|_| CliError::Usage(runs_usage()))?;
                options.limit = Some(limit);
                index += 2;
            }
            "--workflow" | "--workflow-id" => {
                options.workflow_id = Some(required_runs_list_flag_value(args, index)?.to_owned());
                index += 2;
            }
            "--status" => {
                options.status = Some(required_runs_list_flag_value(args, index)?.to_owned());
                index += 2;
            }
            "-h" | "--help" => return Err(CliError::Usage(runs_usage())),
            extra if extra.starts_with('-') => return Err(CliError::Usage(runs_usage())),
            extra => {
                return Err(CliError::Usage(format!(
                    "unexpected argument for runs list: {extra}"
                )));
            }
        }
    }
    Ok(options)
}

fn required_runs_list_flag_value(args: &[String], index: usize) -> CliResult<&str> {
    let Some(value) = args.get(index + 1).map(String::as_str) else {
        return Err(CliError::Usage(runs_usage()));
    };
    if value.starts_with("--") {
        return Err(CliError::Usage(runs_usage()));
    }
    Ok(value)
}

fn runs_usage() -> String {
    [
        "usage:",
        "  lfw runs list [--limit <n>] [--workflow <workflow_id>] [--status <status>]",
        "  lfw runs get [last|run_id]",
        "  lfw runs replay [last|run_id]",
        "  lfw runs rm <last|run_id>",
        "  lfw trace [last|run_id]",
        "  lfw replay [last|run_id]",
        "",
        "Inspects and replays recorded workflow runs under .lightflow/runs/.",
        "Use trace to read the stored manifest, execution, artifacts, and events.",
        "Use replay to rerun the stored stage definitions; both default to last.",
    ]
    .join("\n")
}
