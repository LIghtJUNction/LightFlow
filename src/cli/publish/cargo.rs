use super::targets::display_path;
use crate::cli::{CliError, CliResult, run_status};
use std::fs;
use std::path::Path;
use std::process::Command;
use toml_edit::DocumentMut;

pub(super) fn workspace_document(root: &Path) -> CliResult<Option<DocumentMut>> {
    let manifest_path = root.join("Cargo.toml");
    if !manifest_path.exists() {
        return Ok(None);
    }
    let source = fs::read_to_string(&manifest_path)?;
    let document = source
        .parse::<DocumentMut>()
        .map_err(|error| CliError::Usage(format!("invalid Cargo manifest: {error}")))?;
    Ok(Some(document))
}

pub(super) fn cargo_publish_command(
    manifest_path: &Path,
    dry_run: bool,
    allow_dirty: bool,
) -> Vec<String> {
    let mut command = vec![
        "cargo".to_owned(),
        "publish".to_owned(),
        "--manifest-path".to_owned(),
        display_path(manifest_path),
    ];
    if allow_dirty {
        command.push("--allow-dirty".to_owned());
    }
    if dry_run {
        command.push("--dry-run".to_owned());
    }
    command
}

pub(super) fn run_cargo_command(command: &[String]) -> CliResult<()> {
    let mut process = Command::new("cargo");
    for arg in &command[1..] {
        process.arg(arg);
    }
    run_status(&mut process)
}
