use super::{CliError, CliResult};
use crate::api::{ApiService, CheckProfile, ReleaseCheckOptions};

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct ReleaseOptions {
    pub(super) apply: bool,
    pub(super) workflow_id: String,
    pub(super) project: Option<String>,
}

pub(super) fn parse_release_options(args: &[String]) -> CliResult<ReleaseOptions> {
    let args = if args.first().is_some_and(|arg| arg == "check") {
        &args[1..]
    } else {
        args
    };
    let mut apply = false;
    let mut workflow_id = "lightflow.text_plan".to_owned();
    let mut project = None;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--apply" => {
                apply = true;
                index += 1;
            }
            "--dry-run" => {
                apply = false;
                index += 1;
            }
            "--workflow" | "--workflow-id" => {
                let Some(value) = args.get(index + 1).filter(|value| !value.starts_with('-'))
                else {
                    return Err(CliError::Usage(release_usage()));
                };
                workflow_id = value.clone();
                index += 2;
            }
            "--project" => {
                let Some(value) = args.get(index + 1).filter(|value| !value.starts_with('-'))
                else {
                    return Err(CliError::Usage(release_usage()));
                };
                project = Some(value.clone());
                index += 2;
            }
            "-h" | "--help" | "help" => {
                return Err(CliError::Usage(release_usage()));
            }
            value if value.starts_with('-') => {
                return Err(CliError::Usage(release_usage()));
            }
            value => {
                return Err(CliError::Usage(format!(
                    "unexpected argument for release check: {value}"
                )));
            }
        }
    }
    Ok(ReleaseOptions {
        apply,
        workflow_id,
        project,
    })
}

pub(super) fn release_check(
    service: &ApiService,
    options: &ReleaseOptions,
) -> CliResult<serde_json::Value> {
    let report = service.release_check(&ReleaseCheckOptions {
        apply: options.apply,
        workflow_id: options.workflow_id.clone(),
        project: options.project.clone(),
        profile: CheckProfile::Release,
    })?;
    let value = serde_json::to_value(report)?;
    if options.apply && value.get("valid") == Some(&serde_json::Value::Bool(false)) {
        return Err(CliError::Usage(value.to_string()));
    }
    Ok(value)
}

fn release_usage() -> String {
    [
        "usage:",
        "  lfw release check [--apply] [--workflow <workflow_id>] [--project <name>]",
        "",
        "Without --apply, commands are reported but not executed.",
        "The selected workflow gate defaults to lightflow.text_plan.",
        "--project accepts full names, paths, labels, or lightflow-* short aliases such as std, flux, rig, or custom-tools.",
    ]
    .join("\n")
}
