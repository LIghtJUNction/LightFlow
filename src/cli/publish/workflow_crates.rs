use super::cargo::{cargo_manifest_error, run_cargo_command, workspace_document};
use super::discovery::discover_workflow_manifest_refs;
use super::options::PublishTarget;
use super::ordering::{
    dedupe_workflow_publish_plans, order_workflow_publish_plans, workflow_package_by_dir_from_plans,
};
use super::targets::{package_field, publish_target_json};
use crate::api::{
    cargo_publish_command, project_filter_matches, publish_issues, read_cargo_manifest,
    workflow_id_from_manifest, workflow_publish_metadata_issues,
};
use crate::cli::loop_check;
use crate::cli::{CliError, CliResult};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
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
                    project_filter_matches(
                        project,
                        name,
                        &manifest.workspace,
                        &manifest.workspace_root,
                    )
                })
            })
        });
    let mut plans = Vec::new();
    let mut workspace_documents = BTreeMap::new();
    for manifest in manifests {
        if !workspace_documents.contains_key(&manifest.workspace_root) {
            workspace_documents.insert(
                manifest.workspace_root.clone(),
                workspace_document(&manifest.workspace_root)?,
            );
        }
        let workspace_document = workspace_documents
            .get(&manifest.workspace_root)
            .and_then(Option::as_ref);
        plans.push(workflow_publish_plan(
            &manifest.path,
            &manifest.workspace,
            &manifest.workspace_root,
            workspace_document,
            apply,
            allow_dirty,
        )?);
    }
    let package_by_dir = workflow_package_by_dir_from_plans(&plans);
    dedupe_workflow_publish_plans(&mut plans);
    order_workflow_publish_plans(&mut plans, &package_by_dir, &workspace_documents)?;

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
    pub(super) workspace_root: PathBuf,
    pub(super) manifest_document: DocumentMut,
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
    workspace_root: &Path,
    workspace_document: Option<&DocumentMut>,
    apply: bool,
    allow_dirty: bool,
) -> CliResult<WorkflowPublishPlan> {
    let document = read_cargo_manifest(manifest_path).map_err(cargo_manifest_error)?;
    let package = package_field(&document, "name")?;
    let version = package_field(&document, "version")?;
    let mut issues = publish_issues(&document, workspace_document);
    issues.extend(workflow_publish_metadata_issues(manifest_path));
    Ok(WorkflowPublishPlan {
        manifest_path: manifest_path.to_path_buf(),
        workspace_root: workspace_root.to_path_buf(),
        manifest_document: document,
        workflow_id: workflow_id_from_manifest(manifest_path),
        package,
        version,
        workspace: workspace.to_owned(),
        issues,
        command: cargo_publish_command(manifest_path, !apply, allow_dirty),
        internal_dependencies: BTreeSet::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn workflow_publish_plan_builds_command_from_apply_and_allow_dirty_flags() {
        let root = test_dir("publish-plan-command");
        let manifest = publishable_manifest(root.path());

        let dry_run = workflow_publish_plan(&manifest, "root", root.path(), None, false, false)
            .expect("dry-run publish plan");
        assert_eq!(
            dry_run.command,
            vec![
                "cargo".to_owned(),
                "publish".to_owned(),
                "--manifest-path".to_owned(),
                manifest.display().to_string(),
                "--dry-run".to_owned(),
            ]
        );

        let apply = workflow_publish_plan(&manifest, "root", root.path(), None, true, true)
            .expect("apply publish plan");
        assert_eq!(
            apply.command,
            vec![
                "cargo".to_owned(),
                "publish".to_owned(),
                "--manifest-path".to_owned(),
                manifest.display().to_string(),
                "--allow-dirty".to_owned(),
            ]
        );
    }

    #[test]
    fn workflow_publish_plan_reports_workflow_source_parse_error() {
        let root = test_dir("publish-plan-source-error");
        let manifest = publishable_manifest(root.path());
        fs::create_dir_all(root.path().join("src")).expect("source dir");
        fs::write(root.path().join("src/lib.rs"), "pub fn define(").expect("workflow source");

        let plan = workflow_publish_plan(&manifest, "root", root.path(), None, false, false)
            .expect("publish plan");

        assert!(
            plan.issues
                .iter()
                .any(|issue| issue.starts_with("workflow source cannot be parsed:"))
        );
        assert_eq!(plan.to_json()["publishable"], false);
    }

    fn publishable_manifest(root: &Path) -> PathBuf {
        fs::create_dir_all(root).expect("publish test root");
        let manifest = root.join("Cargo.toml");
        fs::write(
            &manifest,
            r#"
[package]
name = "demo-workflow"
version = "0.1.0"
description = "Demo workflow."
license = "MIT"
"#,
        )
        .expect("manifest");
        manifest
    }

    struct TestDir {
        path: PathBuf,
    }

    impl TestDir {
        fn path(&self) -> &Path {
            &self.path
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn test_dir(name: &str) -> TestDir {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after unix epoch")
            .as_nanos();
        TestDir {
            path: std::env::temp_dir().join(format!(
                "lightflow-cli-workflow-publish-plan-{name}-{}-{nanos}",
                std::process::id()
            )),
        }
    }
}
