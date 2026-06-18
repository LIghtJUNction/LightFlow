use super::{CliError, CliResult, request_json, required_arg};
use crate::workflow::WorkflowPatch;
use serde_json::json;
use std::fs;
use std::path::{Path, PathBuf};

const PATCHES_DIR: &str = ".lightflow/patches";

pub(super) fn manage_patches(root: &Path, args: &[String]) -> CliResult<serde_json::Value> {
    let action = args.first().map(String::as_str).unwrap_or("list");
    match action {
        "list" | "ls" => {
            ensure_no_patch_extra_args(args, 1, "patch list")?;
            list_patches(root)
        }
        "get" | "show" => {
            let name = required_arg(args, 1, "patch name")?;
            ensure_no_patch_extra_args(args, 2, "patch get")?;
            let patch = read_registered_patch(root, name)?;
            Ok(json!({
                "name": normalized_patch_name(name)?,
                "path": patch_path(root, name)?,
                "patch": patch,
            }))
        }
        "save" | "set" => {
            let name = required_arg(args, 1, "patch name")?;
            let value = required_arg(args, 2, "patch json")?;
            ensure_no_patch_extra_args(args, 3, "patch save")?;
            let patch = parse_patch_argument(root, value)?;
            let path = write_registered_patch(root, name, &patch)?;
            Ok(json!({
                "saved": true,
                "name": normalized_patch_name(name)?,
                "path": path,
                "patch": patch,
            }))
        }
        "rm" | "remove" | "delete" => {
            let name = required_arg(args, 1, "patch name")?;
            ensure_no_patch_extra_args(args, 2, "patch rm")?;
            let path = patch_path(root, name)?;
            let removed = if path.exists() {
                fs::remove_file(&path)?;
                true
            } else {
                false
            };
            Ok(json!({
                "removed": removed,
                "name": normalized_patch_name(name)?,
                "path": path,
            }))
        }
        "validate" | "check" => {
            let value = required_arg(args, 1, "patch json or name")?;
            ensure_no_patch_extra_args(args, 2, "patch validate")?;
            let patch = parse_patch_argument(root, value)?;
            Ok(json!({
                "valid": true,
                "patch": patch,
            }))
        }
        "-h" | "--help" | "help" => Err(CliError::Usage(patch_usage())),
        _ => Err(CliError::Usage(patch_usage())),
    }
}

pub(super) fn parse_patch_argument(root: &Path, value: &str) -> CliResult<WorkflowPatch> {
    serde_json::from_value::<WorkflowPatch>(patch_json_argument(root, value)?).map_err(Into::into)
}

fn patch_json_argument(root: &Path, value: &str) -> CliResult<serde_json::Value> {
    if value == "-"
        || value.starts_with('@')
        || value.trim_start().starts_with('{')
        || value.trim_start().starts_with('[')
    {
        return request_json(value);
    }
    Ok(serde_json::to_value(read_registered_patch(root, value)?)?)
}

fn list_patches(root: &Path) -> CliResult<serde_json::Value> {
    let patches_root = patches_root(root);
    let mut patches = Vec::new();
    if patches_root.exists() {
        for entry in fs::read_dir(&patches_root)? {
            let entry = entry?;
            let path = entry.path();
            if path.extension().and_then(|extension| extension.to_str()) != Some("json") {
                continue;
            }
            let Some(stem) = path.file_stem().and_then(|stem| stem.to_str()) else {
                continue;
            };
            patches.push(json!({
                "name": stem,
                "path": path,
            }));
        }
    }
    patches.sort_by(|left, right| left["name"].as_str().cmp(&right["name"].as_str()));
    Ok(json!({
        "patches": patches,
        "root": patches_root,
    }))
}

fn read_registered_patch(root: &Path, name: &str) -> CliResult<WorkflowPatch> {
    let path = patch_path(root, name)?;
    let value = serde_json::from_slice(&fs::read(path)?)?;
    serde_json::from_value::<WorkflowPatch>(value).map_err(Into::into)
}

fn write_registered_patch(root: &Path, name: &str, patch: &WorkflowPatch) -> CliResult<PathBuf> {
    let path = patch_path(root, name)?;
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, format!("{}\n", serde_json::to_string_pretty(patch)?))?;
    Ok(path)
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
        "  lfw patch validate <json|-|@file|registered-name>",
        "  lfw patch rm <name>",
    ]
    .join("\n")
}
