use super::{CliError, CliResult, patches, request_json};
use crate::workflow::WorkflowExecutionOptions;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub(super) fn lfx_usage(command: &str) -> String {
    run_usage_for(command)
}

fn run_usage_for(command: &str) -> String {
    [
        "usage:".to_owned(),
        format!(
            "  {command} <workflow_id> [--input|-i <name=json>] [--inputs <json|-|@file>] [--text <text>] [--image <path>] [--output <path>] [--disable <node>] [--enable <node>] [--patch <json|-|@file|name>] ['|' <workflow_id> ...]"
        ),
        String::new(),
        "Runs one workflow or a pipeline of workflows and records execution under .lightflow/runs/."
            .to_owned(),
        "Use --input name=json-or-text for one input, --inputs for a JSON object, '-' for stdin, or '@file' for a file path."
            .to_owned(),
        "Use '|' between workflow ids to pipe outputs into the next stage.".to_owned(),
        "Use --patch to apply an inline patch, '@file' patch, or saved patch name without editing workflow source."
            .to_owned(),
    ]
    .join("\n")
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct RunStage {
    pub(super) workflow_id: String,
    pub(super) execution: WorkflowExecutionOptions,
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct RunOptions {
    pub(super) stages: Vec<RunStage>,
}

pub(super) fn parse_run_options(root: &Path, args: &[String]) -> CliResult<RunOptions> {
    parse_run_options_for_command(root, args, "lfw run")
}

pub(super) fn parse_run_options_for_command(
    root: &Path,
    args: &[String],
    command: &str,
) -> CliResult<RunOptions> {
    let usage = lfx_usage(command);
    if args.is_empty()
        || args
            .first()
            .is_some_and(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
    {
        return Err(CliError::Usage(usage));
    }
    let mut stages = Vec::new();
    let mut stage_start = 0;
    for (index, arg) in args.iter().enumerate() {
        if arg == "|" {
            stages.push(parse_run_stage(root, &args[stage_start..index], command)?);
            stage_start = index + 1;
        }
    }
    stages.push(parse_run_stage(root, &args[stage_start..], command)?);
    Ok(RunOptions { stages })
}

fn parse_run_stage(root: &Path, args: &[String], command: &str) -> CliResult<RunStage> {
    let workflow_id = required_run_workflow_id(args, command)?.to_owned();
    let mut execution = WorkflowExecutionOptions::default();
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--input" | "-i" => {
                let value = required_run_flag_value(args, index, command)?;
                insert_input_assignment(&mut execution, value, "--input")?;
                index += 2;
            }
            "--inputs" | "--json" => {
                let value = required_run_flag_value(args, index, command)?;
                let inputs = request_json(value)?;
                let Some(inputs) = inputs.as_object() else {
                    return Err(CliError::Usage("--inputs must be a JSON object".to_owned()));
                };
                execution.inputs.extend(inputs.clone());
                index += 2;
            }
            "--text" => {
                insert_named_input(
                    &mut execution,
                    "text",
                    serde_json::Value::String(
                        required_run_flag_value(args, index, command)?.to_owned(),
                    ),
                )?;
                index += 2;
            }
            "--prompt" => {
                insert_named_input(
                    &mut execution,
                    "prompt",
                    serde_json::Value::String(
                        required_run_flag_value(args, index, command)?.to_owned(),
                    ),
                )?;
                index += 2;
            }
            "--image" | "--image-path" => {
                insert_named_input(
                    &mut execution,
                    "image_path",
                    serde_json::Value::String(
                        required_run_flag_value(args, index, command)?.to_owned(),
                    ),
                )?;
                index += 2;
            }
            "--output" | "--output-path" | "-o" => {
                insert_named_input(
                    &mut execution,
                    "output_path",
                    serde_json::Value::String(
                        required_run_flag_value(args, index, command)?.to_owned(),
                    ),
                )?;
                index += 2;
            }
            "--disable" => {
                execution
                    .disabled_nodes
                    .push(required_run_flag_value(args, index, command)?.to_owned());
                index += 2;
            }
            "--enable" => {
                execution
                    .enabled_nodes
                    .push(required_run_flag_value(args, index, command)?.to_owned());
                index += 2;
            }
            "--patch" => {
                let value = required_run_flag_value(args, index, command)?;
                execution.patch = Some(patches::parse_patch_argument(root, value)?);
                index += 2;
            }
            value => {
                if value.starts_with('-') {
                    return Err(CliError::Usage(lfx_usage(command)));
                }
                return Err(CliError::Usage(format!(
                    "unexpected argument for {command}: {value}\n{}",
                    lfx_usage(command)
                )));
            }
        }
    }
    Ok(RunStage {
        workflow_id,
        execution,
    })
}

fn required_run_workflow_id<'a>(args: &'a [String], command: &str) -> CliResult<&'a str> {
    let Some(value) = args.first().map(String::as_str) else {
        return Err(CliError::Usage(lfx_usage(command)));
    };
    if value.starts_with('-') || value == "|" {
        return Err(CliError::Usage(lfx_usage(command)));
    }
    Ok(value)
}

fn required_run_flag_value<'a>(
    args: &'a [String],
    index: usize,
    command: &str,
) -> CliResult<&'a str> {
    let Some(value) = args.get(index + 1).map(String::as_str) else {
        return Err(CliError::Usage(lfx_usage(command)));
    };
    if value.starts_with("--") || value == "|" {
        return Err(CliError::Usage(lfx_usage(command)));
    }
    Ok(value)
}

fn insert_input_assignment(
    execution: &mut WorkflowExecutionOptions,
    value: &str,
    flag: &str,
) -> CliResult<()> {
    let Some((name, raw_value)) = value.split_once('=') else {
        return Err(CliError::Usage(format!(
            "{flag} must use <name=json-or-text>"
        )));
    };
    insert_named_input(execution, name, parse_input_value(raw_value))
}

fn insert_named_input(
    execution: &mut WorkflowExecutionOptions,
    name: &str,
    value: serde_json::Value,
) -> CliResult<()> {
    if name.is_empty() {
        return Err(CliError::Usage("input name cannot be empty".to_owned()));
    }
    execution.inputs.insert(name.to_owned(), value);
    Ok(())
}

fn parse_input_value(value: &str) -> serde_json::Value {
    serde_json::from_str(value).unwrap_or_else(|_| serde_json::Value::String(value.to_owned()))
}
