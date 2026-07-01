use super::{CliError, CliResult, ensure_no_extra_args};
use crate::api::ApiService;

pub(super) fn manage_nodes(service: &ApiService, args: &[String]) -> CliResult<serde_json::Value> {
    let action = match args.first().map(String::as_str) {
        Some("-h" | "--help" | "help") | None => {
            return Err(CliError::Usage(node_usage()));
        }
        Some(action) => action,
    };
    match action {
        "test" => {
            if args
                .get(1)
                .is_some_and(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
            {
                return Err(CliError::Usage(node_usage()));
            }
            let workflow_id = required_node_workflow_id(args, 1)?;
            ensure_no_extra_args(args, 2, "node test")?;
            let report = super::node_conformance::node_conformance(service, workflow_id)?;
            let valid = report.valid;
            let value = serde_json::to_value(report)?;
            if valid {
                Ok(value)
            } else {
                Err(CliError::Usage(value.to_string()))
            }
        }
        _ => Err(CliError::Usage(format!(
            "node action must be test\n{}",
            node_usage()
        ))),
    }
}

fn required_node_workflow_id(args: &[String], index: usize) -> CliResult<&str> {
    let Some(value) = args.get(index).map(String::as_str) else {
        return Err(CliError::Usage(node_usage()));
    };
    if value.starts_with('-') || value == "|" {
        return Err(CliError::Usage(node_usage()));
    }
    Ok(value)
}

fn node_usage() -> String {
    [
        "usage:",
        "  lfw node test <workflow_id>",
        "",
        "Runs workflow node conformance checks for developer handoff.",
        "Checks validation, generated help, port schema metadata, placeholder text, model readiness, runtime executor metadata, and colocated agent skill coverage.",
    ]
    .join("\n")
}
