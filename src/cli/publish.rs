use super::{CliError, CliResult};
use serde_json::json;
use std::path::Path;

mod cargo;
mod discovery;
mod options;
mod ordering;
mod targets;
mod workflow_crates;

use crate::api::{cargo_publish_command, publish_issues, workflow_publish_metadata_issues};
use cargo::{
    cargo_manifest_error, run_cargo_command, workspace_document, workspace_root_for_manifest,
};
pub(super) use options::{PublishOptions, PublishTarget, parse_publish_options};
use targets::{package_field, publish_manifest_path, publish_target_json};
use workflow_crates::publish_workflow_crates;

pub(super) fn publish_crate(root: &Path, options: &PublishOptions) -> CliResult<serde_json::Value> {
    if matches!(options.target, PublishTarget::Workflows) {
        return publish_workflow_crates(
            root,
            options.apply,
            options.allow_dirty,
            options.require_publishable,
            options.project.as_deref(),
        );
    }
    let manifest_path = publish_manifest_path(root, &options.target)?;
    if !manifest_path.exists() {
        return Err(CliError::Usage(format!(
            "publish manifest does not exist: {}",
            manifest_path.display()
        )));
    }
    let document = crate::api::read_cargo_manifest(&manifest_path).map_err(cargo_manifest_error)?;
    let package = package_field(&document, "name")?;
    let version = package_field(&document, "version")?;
    let workspace_root = workspace_root_for_manifest(root, &manifest_path)?;
    let workspace_document = workspace_document(&workspace_root)?;
    let mut issues = publish_issues(&document, workspace_document.as_ref());
    if matches!(options.target, PublishTarget::Workflow(_)) {
        issues.extend(workflow_publish_metadata_issues(&manifest_path));
    }
    let command = cargo_publish_command(&manifest_path, !options.apply, options.allow_dirty);
    let preflight_command = cargo_publish_command(&manifest_path, true, options.allow_dirty);

    if options.apply {
        if !issues.is_empty() {
            return Err(CliError::Usage(format!(
                "crate is not publishable: {}",
                issues.join("; ")
            )));
        }
        if matches!(options.target, PublishTarget::Workflow(_)) {
            super::loop_check::ensure_loop_changes_valid(root)?;
        }
        run_cargo_command(&preflight_command)?;
        run_cargo_command(&command)?;
    }

    let output = json!({
        "dry_run": !options.apply,
        "target": publish_target_json(&options.target),
        "manifest": manifest_path,
        "package": package,
        "version": version,
        "publishable": issues.is_empty(),
        "issues": issues,
        "command": command.clone(),
        "preflight_commands": if options.apply {
            vec![preflight_command.clone()]
        } else {
            Vec::<Vec<String>>::new()
        },
        "executed": if options.apply {
            vec![
                preflight_command,
                command,
            ]
        } else {
            Vec::<Vec<String>>::new()
        },
    });
    if options.require_publishable && output["publishable"] != true {
        return Err(CliError::Usage(output.to_string()));
    }
    Ok(output)
}
