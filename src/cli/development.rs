use super::{CliError, CliResult};
use crate::api::{ApiService, CheckProfile, ReleaseCheckOptions};

mod templates;

use templates::{project_config_template_json, skill_template_json};

#[derive(Debug, Clone, Eq, PartialEq)]
struct DevelopmentOptions {
    apply: bool,
    workflow_id: String,
    project: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct SkillTemplateOptions {
    workflow_id: String,
    write: bool,
    force: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct ProjectConfigTemplateOptions {
    write: bool,
    force: bool,
}

pub(super) fn manage_development(
    service: &ApiService,
    args: &[String],
) -> CliResult<serde_json::Value> {
    let action = args.first().map(String::as_str).unwrap_or("check");
    match action {
        "check" => {
            let options = parse_development_options(args)?;
            development_check(service, &options)
        }
        value if value.starts_with('-') => {
            let options = parse_development_options(args)?;
            development_check(service, &options)
        }
        "skill-template" | "skill" => {
            let options = parse_skill_template_options(&args[1..])?;
            let workflow = service.get_workflow(&options.workflow_id)?;
            skill_template_json(service, &workflow, &options)
        }
        "project-config-template" | "project-config" => {
            let options = parse_project_config_template_options(&args[1..])?;
            project_config_template_json(service, &options)
        }
        "-h" | "--help" | "help" => Err(CliError::Usage(development_usage())),
        value => Err(CliError::Usage(format!(
            "unknown dev action: {value}\n{}",
            development_usage()
        ))),
    }
}

fn parse_development_options(args: &[String]) -> CliResult<DevelopmentOptions> {
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
                    return Err(CliError::Usage(development_usage()));
                };
                workflow_id = value.clone();
                index += 2;
            }
            "--project" => {
                let Some(value) = args.get(index + 1).filter(|value| !value.starts_with('-'))
                else {
                    return Err(CliError::Usage(development_usage()));
                };
                project = Some(value.clone());
                index += 2;
            }
            "-h" | "--help" | "help" => {
                return Err(CliError::Usage(development_usage()));
            }
            value if value.starts_with('-') => {
                return Err(CliError::Usage(development_usage()));
            }
            value => {
                return Err(CliError::Usage(format!(
                    "unexpected argument for dev check: {value}"
                )));
            }
        }
    }
    Ok(DevelopmentOptions {
        apply,
        workflow_id,
        project,
    })
}

fn parse_skill_template_options(args: &[String]) -> CliResult<SkillTemplateOptions> {
    let mut workflow_id = None;
    let mut write = false;
    let mut force = false;
    for arg in args {
        match arg.as_str() {
            "--write" => write = true,
            "--force" => force = true,
            "-h" | "--help" | "help" => return Err(CliError::Usage(development_usage())),
            value if value.starts_with('-') => return Err(CliError::Usage(development_usage())),
            value => {
                if workflow_id.replace(value.to_owned()).is_some() {
                    return Err(CliError::Usage(development_usage()));
                }
            }
        }
    }
    let Some(workflow_id) = workflow_id else {
        return Err(CliError::Usage(development_usage()));
    };
    Ok(SkillTemplateOptions {
        workflow_id,
        write,
        force,
    })
}

fn development_check(
    service: &ApiService,
    options: &DevelopmentOptions,
) -> CliResult<serde_json::Value> {
    let report = service.release_check(&ReleaseCheckOptions {
        apply: options.apply,
        workflow_id: options.workflow_id.clone(),
        project: options.project.clone(),
        profile: CheckProfile::Development,
    })?;
    let value = serde_json::to_value(report)?;
    if options.apply && value.get("valid") == Some(&serde_json::Value::Bool(false)) {
        return Err(CliError::Usage(value.to_string()));
    }
    Ok(value)
}

fn parse_project_config_template_options(
    args: &[String],
) -> CliResult<ProjectConfigTemplateOptions> {
    let mut write = false;
    let mut force = false;
    for arg in args {
        match arg.as_str() {
            "--write" => write = true,
            "--force" => force = true,
            "-h" | "--help" | "help" => return Err(CliError::Usage(development_usage())),
            _ => return Err(CliError::Usage(development_usage())),
        }
    }
    Ok(ProjectConfigTemplateOptions { write, force })
}

fn development_usage() -> String {
    [
        "usage:",
        "  lfw dev check [--apply] [--workflow <workflow_id>] [--project <name>]",
        "  lfw dev skill-template <workflow_id> [--write] [--force]",
        "  lfw dev project-config-template [--write] [--force]",
        "",
        "Runs the developer gate plan used before handing off code changes.",
        "Use skill-template to generate a compliant starter SKILL.md for one workflow.",
        "Use project-config-template to generate projects/lightflow-projects.toml and report project_submodule_update_command.",
        "--write creates the generated file; --force allows overwriting it.",
        "Without --apply, commands are reported but not executed.",
        "The selected workflow gate defaults to lightflow.text_plan.",
        "--project accepts full names, paths, labels, or lightflow-* short aliases such as std, flux, rig, or custom-tools.",
    ]
    .join("\n")
}
