use super::{CliError, CliResult, patches, request_json, required_arg, required_flag_value};
use crate::workflow::WorkflowExecutionOptions;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub(super) fn lfx_usage() -> String {
    "usage:\n  lfx <workflow_id> [--input|-i <name=json>] [--inputs <json|-|@file>] [--text <text>] [--image <path>] [--output <path>] [--disable <node>] [--enable <node>] [--patch <json|-|@file|name>] ['|' <workflow_id> ...]"
        .to_owned()
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
    let mut stages = Vec::new();
    let mut stage_start = 0;
    for (index, arg) in args.iter().enumerate() {
        if arg == "|" {
            stages.push(parse_run_stage(root, &args[stage_start..index])?);
            stage_start = index + 1;
        }
    }
    stages.push(parse_run_stage(root, &args[stage_start..])?);
    Ok(RunOptions { stages })
}

fn parse_run_stage(root: &Path, args: &[String]) -> CliResult<RunStage> {
    let workflow_id = required_arg(args, 0, "workflow id")?.to_owned();
    let mut execution = WorkflowExecutionOptions::default();
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--input" | "-i" => {
                let value = required_flag_value(args, index, "--input")?;
                insert_input_assignment(&mut execution, value, "--input")?;
                index += 2;
            }
            "--inputs" | "--json" => {
                let value = required_flag_value(args, index, "--inputs")?;
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
                        required_flag_value(args, index, "--text")?.to_owned(),
                    ),
                )?;
                index += 2;
            }
            "--prompt" => {
                insert_named_input(
                    &mut execution,
                    "prompt",
                    serde_json::Value::String(
                        required_flag_value(args, index, "--prompt")?.to_owned(),
                    ),
                )?;
                index += 2;
            }
            "--image" | "--image-path" => {
                insert_named_input(
                    &mut execution,
                    "image_path",
                    serde_json::Value::String(
                        required_flag_value(args, index, args[index].as_str())?.to_owned(),
                    ),
                )?;
                index += 2;
            }
            "--output" | "--output-path" | "-o" => {
                insert_named_input(
                    &mut execution,
                    "output_path",
                    serde_json::Value::String(
                        required_flag_value(args, index, args[index].as_str())?.to_owned(),
                    ),
                )?;
                index += 2;
            }
            "--disable" => {
                execution
                    .disabled_nodes
                    .push(required_flag_value(args, index, "--disable")?.to_owned());
                index += 2;
            }
            "--enable" => {
                execution
                    .enabled_nodes
                    .push(required_flag_value(args, index, "--enable")?.to_owned());
                index += 2;
            }
            "--patch" => {
                let value = required_flag_value(args, index, "--patch")?;
                execution.patch = Some(patches::parse_patch_argument(root, value)?);
                index += 2;
            }
            value => {
                return Err(CliError::Usage(format!(
                    "unexpected argument for run: {value}"
                )));
            }
        }
    }
    Ok(RunStage {
        workflow_id,
        execution,
    })
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
