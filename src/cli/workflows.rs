use super::support::{
    required_workflow_id_arg, required_workflow_json_arg, workflow_json_argument,
    workflow_shortcuts_usage, workflows_usage,
};
use super::{CliError, CliResult, ensure_no_extra_args};
use crate::api::ApiService;
use crate::workflow::WorkflowSpec;

pub(super) fn workflow_subcommand(
    service: &ApiService,
    args: &[String],
) -> CliResult<serde_json::Value> {
    let action = match args.first().map(String::as_str) {
        Some("-h" | "--help") | None => {
            return Err(CliError::Usage(workflows_usage()));
        }
        Some(action) => action,
    };
    match action {
        "list" => workflow_list(service, args),
        "get" => workflow_get(service, args),
        "deps" | "dependencies" => workflow_dependencies(service, args),
        "plan" => workflow_plan(service, args),
        "help" => workflow_help(service, args),
        "validate" => workflow_validate(service, args),
        "save" => workflow_save(service, args),
        _ => Err(CliError::Usage(format!(
            "workflow action must be list|get|help|deps|plan|validate|save\n{}",
            workflows_usage()
        ))),
    }
}

pub(super) fn workflow_dependencies_shortcut(
    service: &ApiService,
    args: &[String],
) -> CliResult<serde_json::Value> {
    if is_help_arg(args.first()) {
        return Err(CliError::Usage(workflow_shortcuts_usage()));
    }
    let workflow_id = required_workflow_id_arg(args, 0, workflow_shortcuts_usage)?;
    ensure_no_extra_args(args, 1, "deps")?;
    Ok(serde_json::to_value(
        service.workflow_dependencies(workflow_id)?,
    )?)
}

pub(super) fn workflow_plan_shortcut(
    service: &ApiService,
    args: &[String],
) -> CliResult<serde_json::Value> {
    if is_help_arg(args.first()) {
        return Err(CliError::Usage(workflow_shortcuts_usage()));
    }
    let workflow_id = required_workflow_id_arg(args, 0, workflow_shortcuts_usage)?;
    ensure_no_extra_args(args, 1, "plan")?;
    Ok(serde_json::to_value(service.plan_workflow(workflow_id)?)?)
}

pub(super) fn workflow_help_shortcut(
    service: &ApiService,
    args: &[String],
) -> CliResult<serde_json::Value> {
    if args.is_empty() || is_help_arg(args.first()) {
        return Err(CliError::Usage(workflow_shortcuts_usage()));
    }
    required_workflow_id_arg(args, 0, workflow_shortcuts_usage)?;
    super::workflow_help::workflow_help(service, args, "help")
}

fn workflow_list(service: &ApiService, args: &[String]) -> CliResult<serde_json::Value> {
    ensure_subcommand_args(args)?;
    ensure_no_extra_args(args, 1, "workflows list")?;
    Ok(serde_json::to_value(service.list_workflows()?)?)
}

fn workflow_get(service: &ApiService, args: &[String]) -> CliResult<serde_json::Value> {
    ensure_subcommand_args(args)?;
    let workflow_id = required_workflow_id_arg(args, 1, workflows_usage)?;
    ensure_no_extra_args(args, 2, "workflows get")?;
    Ok(serde_json::to_value(service.get_workflow(workflow_id)?)?)
}

fn workflow_dependencies(service: &ApiService, args: &[String]) -> CliResult<serde_json::Value> {
    ensure_subcommand_args(args)?;
    let workflow_id = required_workflow_id_arg(args, 1, workflows_usage)?;
    ensure_no_extra_args(args, 2, "workflows deps")?;
    Ok(serde_json::to_value(
        service.workflow_dependencies(workflow_id)?,
    )?)
}

fn workflow_plan(service: &ApiService, args: &[String]) -> CliResult<serde_json::Value> {
    ensure_subcommand_args(args)?;
    let workflow_id = required_workflow_id_arg(args, 1, workflows_usage)?;
    ensure_no_extra_args(args, 2, "workflows plan")?;
    Ok(serde_json::to_value(service.plan_workflow(workflow_id)?)?)
}

fn workflow_help(service: &ApiService, args: &[String]) -> CliResult<serde_json::Value> {
    if is_help_arg(args.get(1)) {
        return Err(CliError::Usage(workflow_shortcuts_usage()));
    }
    let workflow_id = required_workflow_id_arg(args, 1, workflows_usage)?;
    ensure_no_extra_args(args, 2, "workflows help")?;
    let workflow = service.get_workflow(workflow_id)?;
    let dependencies = service.workflow_dependencies(workflow_id)?;
    Ok(super::workflow_help::workflow_help_json(
        &workflow,
        dependencies,
    ))
}

fn workflow_validate(service: &ApiService, args: &[String]) -> CliResult<serde_json::Value> {
    ensure_subcommand_args(args)?;
    let workflow: WorkflowSpec =
        workflow_json_argument(required_workflow_json_arg(args, 1)?, "workflows validate")?;
    ensure_no_extra_args(args, 2, "workflows validate")?;
    Ok(serde_json::to_value(service.validate_workflow(&workflow))?)
}

fn workflow_save(service: &ApiService, args: &[String]) -> CliResult<serde_json::Value> {
    ensure_subcommand_args(args)?;
    let workflow: WorkflowSpec =
        workflow_json_argument(required_workflow_json_arg(args, 1)?, "workflows save")?;
    ensure_no_extra_args(args, 2, "workflows save")?;
    Ok(serde_json::to_value(service.save_workflow(workflow)?)?)
}

fn ensure_subcommand_args(args: &[String]) -> CliResult<()> {
    if is_help_arg(args.get(1)) {
        return Err(CliError::Usage(workflows_usage()));
    }
    Ok(())
}

fn is_help_arg(arg: Option<&String>) -> bool {
    arg.is_some_and(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
}
