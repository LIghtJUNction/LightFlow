use super::options::PublishTarget;
use crate::api::{categorized_workflow_manifest_path, package_field_value};
use crate::cli::{CliError, CliResult};
use serde_json::json;
use std::path::{Path, PathBuf};
use toml_edit::DocumentMut;

pub(super) fn publish_manifest_path(root: &Path, target: &PublishTarget) -> CliResult<PathBuf> {
    match target {
        PublishTarget::Root => Ok(root.join("Cargo.toml")),
        PublishTarget::Workflow(workflow_id) => {
            categorized_workflow_manifest_path(root, workflow_id).map_err(CliError::Io)
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
    package_field_value(document, field)
        .ok_or_else(|| CliError::Usage(format!("Cargo manifest is missing package.{field}")))
}
