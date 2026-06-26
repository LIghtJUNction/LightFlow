use super::{CliError, CliResult, request_json};
use crate::api::ApiService;
use crate::workflow::WorkflowPatch;
use std::fs;
use std::path::{Path, PathBuf};

const PATCHES_DIR: &str = ".lightflow/patches";

pub(super) fn manage_patches(
    service: &ApiService,
    args: &[String],
) -> CliResult<serde_json::Value> {
    let root = service.repo_root();
    let action = args.first().map(String::as_str).unwrap_or("list");
    match action {
        "list" | "ls" => {
            if args
                .get(1)
                .is_some_and(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
            {
                return Err(CliError::Usage(patch_usage()));
            }
            ensure_no_patch_extra_args(args, 1, "patch list")?;
            Ok(serde_json::to_value(service.list_patches()?)?)
        }
        "get" | "show" => {
            if args
                .get(1)
                .is_some_and(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
            {
                return Err(CliError::Usage(patch_usage()));
            }
            let Some(name) = args.get(1).map(String::as_str) else {
                return Err(CliError::Usage(patch_usage()));
            };
            ensure_no_patch_extra_args(args, 2, "patch get")?;
            Ok(serde_json::to_value(service.get_patch(name)?)?)
        }
        "save" | "set" => {
            if args
                .get(1)
                .is_some_and(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
            {
                return Err(CliError::Usage(patch_usage()));
            }
            let Some(name) = args.get(1).map(String::as_str) else {
                return Err(CliError::Usage(patch_usage()));
            };
            let Some(value) = args.get(2).map(String::as_str) else {
                return Err(CliError::Usage(patch_usage()));
            };
            ensure_no_patch_extra_args(args, 3, "patch save")?;
            let patch = parse_patch_argument(root, value)?;
            Ok(serde_json::to_value(service.save_patch(name, &patch)?)?)
        }
        "rm" | "remove" | "delete" => {
            if args
                .get(1)
                .is_some_and(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
            {
                return Err(CliError::Usage(patch_usage()));
            }
            let Some(name) = args.get(1).map(String::as_str) else {
                return Err(CliError::Usage(patch_usage()));
            };
            ensure_no_patch_extra_args(args, 2, "patch rm")?;
            Ok(serde_json::to_value(service.remove_patch(name)?)?)
        }
        "validate" | "check" => {
            if args
                .get(1)
                .is_some_and(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
            {
                return Err(CliError::Usage(patch_usage()));
            }
            let (value, workflow_id) = parse_patch_validate_args(args)?;
            let patch = parse_patch_argument(root, value)?;
            let validation = if let Some(workflow_id) = workflow_id {
                service.validate_patch_for_workflow(workflow_id, patch)
            } else {
                service.validate_patch(patch)
            };
            let value = serde_json::to_value(validation)?;
            if value
                .get("valid")
                .and_then(serde_json::Value::as_bool)
                .unwrap_or(false)
            {
                Ok(value)
            } else {
                Err(CliError::Usage(value.to_string()))
            }
        }
        "-h" | "--help" | "help" => Err(CliError::Usage(patch_usage())),
        _ => Err(CliError::Usage(patch_usage())),
    }
}

fn parse_patch_validate_args(args: &[String]) -> CliResult<(&str, Option<&str>)> {
    let Some(value) = args.get(1).map(String::as_str) else {
        return Err(CliError::Usage(patch_usage()));
    };
    let mut workflow_id = None;
    let mut index = 2;
    while index < args.len() {
        match args[index].as_str() {
            "--workflow" | "--workflow-id" => {
                if workflow_id.is_some() {
                    return Err(CliError::Usage(
                        "patch validate accepts only one workflow id".to_owned(),
                    ));
                }
                workflow_id = Some(required_patch_workflow_id(args, index + 1)?);
                index += 2;
            }
            value if value.starts_with('-') => {
                return Err(CliError::Usage(patch_usage()));
            }
            value => {
                return Err(CliError::Usage(format!(
                    "unexpected argument for patch validate: {value}"
                )));
            }
        }
    }
    Ok((value, workflow_id))
}

fn required_patch_workflow_id(args: &[String], index: usize) -> CliResult<&str> {
    let Some(value) = args.get(index).map(String::as_str) else {
        return Err(CliError::Usage(patch_usage()));
    };
    if value.starts_with('-') || value == "|" {
        return Err(CliError::Usage(patch_usage()));
    }
    Ok(value)
}

pub(super) fn parse_patch_argument(root: &Path, value: &str) -> CliResult<WorkflowPatch> {
    let value = patch_json_argument(root, value)?;
    serde_json::from_value::<WorkflowPatch>(value)
        .map_err(|error| CliError::Usage(format!("invalid patch JSON: {error}\n{}", patch_usage())))
}

fn patch_json_argument(root: &Path, value: &str) -> CliResult<serde_json::Value> {
    if value == "-"
        || value.starts_with('@')
        || value.trim_start().starts_with('{')
        || value.trim_start().starts_with('[')
    {
        return request_json(value).map_err(|error| match error {
            CliError::Usage(message) => CliError::Usage(message),
            other => CliError::Usage(format!("invalid patch JSON: {other}\n{}", patch_usage())),
        });
    }
    Ok(serde_json::to_value(read_registered_patch(root, value)?)?)
}

fn read_registered_patch(root: &Path, name: &str) -> CliResult<WorkflowPatch> {
    let path = patch_path(root, name)?;
    let value = serde_json::from_slice(&fs::read(path)?)?;
    serde_json::from_value::<WorkflowPatch>(value).map_err(Into::into)
}

fn patch_path(root: &Path, name: &str) -> CliResult<PathBuf> {
    Ok(patches_root(root).join(format!("{}.json", normalized_patch_name(name)?)))
}

fn patches_root(root: &Path) -> PathBuf {
    root.join(PATCHES_DIR)
}

fn normalized_patch_name(name: &str) -> CliResult<String> {
    let name = name.strip_suffix(".json").unwrap_or(name);
    if name.is_empty()
        || name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.chars().any(char::is_whitespace)
    {
        return Err(CliError::Usage(
            "patch name must be a single non-empty file name".to_owned(),
        ));
    }
    Ok(name.to_owned())
}

fn ensure_no_patch_extra_args(args: &[String], max_len: usize, command: &str) -> CliResult<()> {
    if let Some(extra) = args.get(max_len) {
        return Err(CliError::Usage(format!(
            "unexpected argument for {command}: {extra}"
        )));
    }
    Ok(())
}

fn patch_usage() -> String {
    [
        "usage:",
        "  lfw patch list",
        "  lfw patch get <name>",
        "  lfw patch save <name> <json|-|@file|registered-name>",
        "  lfw patch validate <json|-|@file|registered-name> [--workflow <workflow_id>]",
        "  lfw patch rm <name>",
        "",
        "Stores and validates reusable workflow run patches under .lightflow/patches/.",
        "Patch JSON can be inline JSON, '-' for stdin, '@file', or a saved patch name.",
        "Use validate before running with --patch to catch unknown nodes and port mismatches.",
    ]
    .join("\n")
}
