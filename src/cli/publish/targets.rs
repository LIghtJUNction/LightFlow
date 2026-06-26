use super::super::project::workflow_crate_dir_name;
use super::options::PublishTarget;
use crate::cli::{CliError, CliResult};
use serde_json::json;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item};

pub(super) fn display_path(path: &Path) -> String {
    path.strip_prefix(".").unwrap_or(path).display().to_string()
}

pub(super) fn publish_manifest_path(root: &Path, target: &PublishTarget) -> CliResult<PathBuf> {
    match target {
        PublishTarget::Root => Ok(root.join("Cargo.toml")),
        PublishTarget::Workflow(workflow_id) => {
            categorized_workflow_manifest_path(root, workflow_id)
        }
        PublishTarget::Crate(path) => Ok({
            if path.ends_with("Cargo.toml") {
                root.join(path)
            } else {
                root.join(path).join("Cargo.toml")
            }
        }),
        PublishTarget::Workflows => Err(CliError::Usage(
            "--workflows does not resolve to one Cargo manifest".to_owned(),
        )),
    }
}

fn categorized_workflow_manifest_path(root: &Path, workflow_id: &str) -> CliResult<PathBuf> {
    let workflows = root.join("workflows");
    let legacy_workflows = root.join("lightflow").join("workflows");
    let entries = match fs::read_dir(&workflows).or_else(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            fs::read_dir(&legacy_workflows)
        } else {
            Err(error)
        }
    }) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(root.join("workflows").join(workflow_id).join("Cargo.toml"));
        }
        Err(error) => return Err(CliError::Io(error)),
    };
    for entry in entries {
        let path = entry?.path();
        if !path.is_dir() || path.join("src").join("lib.rs").exists() {
            continue;
        }
        let category = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let manifest = path
            .join(workflow_crate_dir_name(workflow_id))
            .join("Cargo.toml");
        if manifest.exists() {
            return Ok(manifest);
        }
        if let Some(short_name) = workflow_category_short_name(workflow_id, category) {
            let manifest = path.join(short_name).join("Cargo.toml");
            if manifest.exists() {
                return Ok(manifest);
            }
        }
    }
    Ok(root.join("workflows").join(workflow_id).join("Cargo.toml"))
}

fn workflow_category_short_name(workflow_id: &str, category: &str) -> Option<String> {
    let prefixed = workflow_id
        .strip_prefix("lightflow.")
        .unwrap_or(workflow_id);
    let short = prefixed.strip_prefix(category)?.strip_prefix('.')?;
    Some(short.replace('.', "_"))
}

pub(super) fn publish_target_json(target: &PublishTarget) -> serde_json::Value {
    match target {
        PublishTarget::Root => json!({ "kind": "root" }),
        PublishTarget::Workflow(workflow_id) => {
            json!({ "kind": "workflow", "workflow_id": workflow_id })
        }
        PublishTarget::Crate(path) => json!({ "kind": "crate", "path": path }),
        PublishTarget::Workflows => json!({ "kind": "workflows" }),
    }
}

pub(super) fn package_field(document: &DocumentMut, field: &str) -> CliResult<String> {
    document
        .get("package")
        .and_then(|package| package.get(field))
        .and_then(Item::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| CliError::Usage(format!("Cargo manifest is missing package.{field}")))
}
