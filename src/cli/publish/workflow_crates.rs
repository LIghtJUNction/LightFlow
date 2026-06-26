use super::cargo::{cargo_publish_command, run_cargo_command, workspace_document};
use super::checks::{publish_issues, workflow_id_from_manifest, workflow_publish_metadata_issues};
use super::discovery::{discover_workflow_manifest_refs, publish_project_matches};
use super::options::PublishTarget;
use super::ordering::{
    dedupe_workflow_publish_plans, order_workflow_publish_plans, workflow_package_by_dir_from_plans,
};
use super::targets::{package_field, publish_target_json};
use crate::cli::loop_check;
use crate::cli::{CliError, CliResult};
use serde_json::json;
use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::DocumentMut;

pub(super) fn publish_workflow_crates(
    root: &Path,
    apply: bool,
    allow_dirty: bool,
    require_publishable: bool,
    project: Option<&str>,
) -> CliResult<serde_json::Value> {
    let manifests = discover_workflow_manifest_refs(root, project)?;
    if manifests.is_empty() {
        let message = match project {
            Some(project) => format!("no workflow crates found for project workspace: {project}"),
            None => "no workflow crates found under workflows/*/*".to_owned(),
        };
        return Err(CliError::Usage(message));
    }
    let project_filter_matched = project.is_none()
        || manifests.iter().any(|manifest| {
            manifest.project_name.as_deref().is_some_and(|name| {
                project.is_some_and(|project| {
                    publish_project_matches(project, name, &manifest.workspace)
                })
            })
        });
    let mut plans = Vec::new();
    for manifest in manifests {
        let workspace_document = workspace_document(&manifest.workspace_root)?;
        plans.push(workflow_publish_plan(
            &manifest.path,
            &manifest.workspace,
            workspace_document.as_ref(),
            apply,
            allow_dirty,
        )?);
    }
    let package_by_dir = workflow_package_by_dir_from_plans(&plans);
    dedupe_workflow_publish_plans(&mut plans);
    order_workflow_publish_plans(&mut plans, &package_by_dir)?;

    let total = plans.len();
    let publishable_count = plans.iter().filter(|plan| plan.issues.is_empty()).count();
    let blocked_count = total.saturating_sub(publishable_count);
    let publishable = total > 0 && blocked_count == 0;
    if apply && !publishable {
        let issues = plans
            .iter()
            .filter(|plan| !plan.issues.is_empty())
            .map(|plan| format!("{}: {}", plan.package, plan.issues.join("; ")))
            .collect::<Vec<_>>()
            .join("; ");
        return Err(CliError::Usage(format!(
            "not all workflow crates are publishable: {issues}"
        )));
    }
    if apply {
        loop_check::ensure_loop_changes_valid(root)?;
    }

    let preflight_commands = plans
        .iter()
        .map(|plan| cargo_publish_command(&plan.manifest_path, true, allow_dirty))
        .collect::<Vec<_>>();
    let commands = plans
        .iter()
        .map(|plan| plan.command.clone())
        .collect::<Vec<_>>();
    let mut executed = Vec::new();
    if apply {
        for command in &preflight_commands {
            run_cargo_command(command)?;
            executed.push(command.clone());
        }
        for command in &commands {
            run_cargo_command(command)?;
            executed.push(command.clone());
        }
    }

    let output = json!({
        "dry_run": !apply,
        "target": publish_target_json(&PublishTarget::Workflows),
        "project": project,
        "project_filter_matched": project_filter_matched,
        "total": total,
        "publishable_count": publishable_count,
        "blocked_count": blocked_count,
        "publishable": publishable,
        "issues": plans
            .iter()
            .flat_map(|plan| {
                plan.issues
                    .iter()
                    .map(|issue| format!("{}: {}", plan.package, issue))
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>(),
        "crates": plans.iter().map(WorkflowPublishPlan::to_json).collect::<Vec<_>>(),
        "commands": commands,
        "preflight_commands": if apply { preflight_commands } else { Vec::<Vec<String>>::new() },
        "executed": executed,
    });
    if require_publishable && !publishable {
        return Err(CliError::Usage(output.to_string()));
    }
    Ok(output)
}

#[derive(Debug)]
pub(super) struct WorkflowPublishPlan {
    pub(super) manifest_path: PathBuf,
    pub(super) workflow_id: Option<String>,
    pub(super) package: String,
    version: String,
    workspace: String,
    pub(super) issues: Vec<String>,
    command: Vec<String>,
    pub(super) internal_dependencies: BTreeSet<String>,
}

impl WorkflowPublishPlan {
    pub(super) fn to_json(&self) -> serde_json::Value {
        json!({
            "manifest": self.manifest_path,
            "workflow_id": self.workflow_id,
            "package": self.package,
            "version": self.version,
            "workspace": self.workspace,
            "publishable": self.issues.is_empty(),
            "issues": self.issues,
            "command": self.command,
            "internal_dependencies": self.internal_dependencies,
        })
    }
}

pub(super) fn workflow_publish_plan(
    manifest_path: &Path,
    workspace: &str,
    workspace_document: Option<&DocumentMut>,
    apply: bool,
    allow_dirty: bool,
) -> CliResult<WorkflowPublishPlan> {
    let source = fs::read_to_string(manifest_path)?;
    let document = source
        .parse::<DocumentMut>()
        .map_err(|error| CliError::Usage(format!("invalid Cargo manifest: {error}")))?;
    Ok(WorkflowPublishPlan {
        manifest_path: manifest_path.to_path_buf(),
        workflow_id: workflow_id_from_manifest(manifest_path),
        package: package_field(&document, "name")?,
        version: package_field(&document, "version")?,
        workspace: workspace.to_owned(),
        issues: {
            let mut issues = publish_issues(&document, workspace_document);
            issues.extend(workflow_publish_metadata_issues(manifest_path));
            issues
        },
        command: cargo_publish_command(manifest_path, !apply, allow_dirty),
        internal_dependencies: BTreeSet::new(),
    })
}
