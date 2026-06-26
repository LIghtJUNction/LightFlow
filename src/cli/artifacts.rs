use super::{CliError, CliResult};
use crate::api::{ApiService, ArtifactCatalog, ArtifactListOptions};

pub(super) fn list_artifacts(service: &ApiService, args: &[String]) -> CliResult<ArtifactCatalog> {
    let options = parse_artifact_options(args)?;
    Ok(service.list_artifacts_with_options(&options)?)
}

fn parse_artifact_options(args: &[String]) -> CliResult<ArtifactListOptions> {
    let mut options = ArtifactListOptions::default();
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--run" | "--run-id" => {
                options.run_id = Some(required_artifact_flag_value(args, index)?.to_owned());
                index += 2;
            }
            "--workflow" | "--workflow-id" => {
                options.workflow_id = Some(required_artifact_flag_value(args, index)?.to_owned());
                index += 2;
            }
            "--kind" => {
                options.kind = Some(required_artifact_flag_value(args, index)?.to_owned());
                index += 2;
            }
            "--limit" => {
                let value = required_artifact_flag_value(args, index)?;
                options.limit = Some(
                    value
                        .parse::<usize>()
                        .map_err(|_| CliError::Usage(artifacts_usage()))?,
                );
                index += 2;
            }
            "-h" | "--help" | "help" => return Err(CliError::Usage(artifacts_usage())),
            extra if extra.starts_with('-') => {
                return Err(CliError::Usage(artifacts_usage()));
            }
            extra => {
                return Err(CliError::Usage(format!(
                    "unexpected argument for artifacts: {extra}"
                )));
            }
        }
    }
    Ok(options)
}

fn required_artifact_flag_value(args: &[String], index: usize) -> CliResult<&str> {
    let Some(value) = args.get(index + 1).map(String::as_str) else {
        return Err(CliError::Usage(artifacts_usage()));
    };
    if value.starts_with("--") {
        return Err(CliError::Usage(artifacts_usage()));
    }
    Ok(value)
}

pub(super) fn artifacts_usage() -> String {
    [
        "usage:",
        "  lfw artifacts [--run <last|run_id>] [--workflow <workflow_id>] [--kind <kind>] [--limit <n>]",
        "",
        "Lists artifact files recorded under .lightflow/runs/ with run, stage, node, workflow, kind, path, size, and content-type context.",
        "Use --run last to inspect the newest run, --workflow to narrow cross-run output, and --kind to focus on images, masks, or other artifact classes.",
        "",
        "examples:",
        "  lfw artifacts --run last",
        "  lfw artifacts --workflow lightflow.text_to_image --kind image --limit 20",
    ]
    .join("\n")
}
