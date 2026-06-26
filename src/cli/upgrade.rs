use super::{CliError, CliResult, run_status};
use serde_json::json;
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub(super) struct CargoWorkspaceOptions {
    pub(super) global: bool,
}

pub(super) fn parse_cargo_workspace_options(
    args: &[String],
    command: &str,
) -> CliResult<CargoWorkspaceOptions> {
    let mut global = false;
    for arg in args {
        match arg.as_str() {
            "-h" | "--help" | "help" => {
                return Err(CliError::Usage(cargo_workspace_usage(command)));
            }
            "--global" | "-g" => global = true,
            value if value.starts_with('-') => {
                return Err(CliError::Usage(cargo_workspace_usage(command)));
            }
            value => {
                return Err(CliError::Usage(format!(
                    "unexpected argument for {command}: {value}\n{}",
                    cargo_workspace_usage(command)
                )));
            }
        }
    }
    Ok(CargoWorkspaceOptions { global })
}

fn cargo_workspace_usage(command: &str) -> String {
    let cargo_action = match command {
        "upgrade" => "cargo update",
        _ => "cargo fetch",
    };
    [
        "usage:",
        &format!("  lfw {command} [--global|-g]"),
        "",
        &format!("Runs `{cargo_action}` in a LightFlow workflow Cargo workspace."),
        "Without --global, the current directory is used.",
        "--global targets the default LightFlow home workflow workspace used for global workflow discovery.",
        "Use update to fetch dependency indexes and package data; use upgrade to update Cargo.lock resolution.",
    ]
    .join("\n")
}

pub(super) fn update_index(root: &Path) -> CliResult<serde_json::Value> {
    run_cargo(root, "fetch")
}

pub(super) fn upgrade_workspace(root: &Path) -> CliResult<serde_json::Value> {
    run_cargo(root, "update")
}

fn run_cargo(root: &Path, action: &str) -> CliResult<serde_json::Value> {
    ensure_workspace_manifest(root)?;
    let mut process = Command::new("cargo");
    process.arg(action).current_dir(root);
    run_status(&mut process)?;
    Ok(json!({
        "workspace": root,
        "command": ["cargo", action],
        "executed": true
    }))
}

fn ensure_workspace_manifest(root: &Path) -> CliResult<()> {
    let manifest_path = root.join("Cargo.toml");
    if manifest_path.exists() {
        Ok(())
    } else {
        Err(CliError::Usage(format!(
            "Cargo manifest not found: {}",
            manifest_path.display()
        )))
    }
}

pub(super) fn cargo_workspace_root(
    current_dir: &Path,
    default_workflow_path: &Path,
    options: &CargoWorkspaceOptions,
) -> PathBuf {
    if options.global {
        default_workflow_path.to_path_buf()
    } else {
        current_dir.to_path_buf()
    }
}
