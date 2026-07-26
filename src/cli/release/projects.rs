use super::super::publish::{
    publish_release_project, publish_release_root, run_publish_command,
    run_publish_preflight_with_retries,
};
use super::{CliError, CliResult, release_usage};
use crate::api::{ApiService, ProjectWorkspaceSummary};
use semver::Version;
use serde::Serialize;
use std::collections::BTreeSet;
use std::path::PathBuf;

mod manifests;
mod support;
#[cfg(test)]
use manifests::read_manifest;
use manifests::{
    DependencyUpdate, ManifestChange, apply_manifests, plan_manifest, required_package_name,
    restore_manifests, validate_workspace_manifest,
};

#[derive(Debug, Clone, Eq, PartialEq)]
struct Options {
    version: Version,
    apply: bool,
    publish: bool,
    allow_dirty: bool,
}

#[derive(Debug, Serialize)]
struct Report {
    dry_run: bool,
    apply: bool,
    publish: bool,
    version: String,
    selected_projects: Vec<String>,
    skipped_projects: Vec<SkippedProject>,
    manifest_changes: Vec<ManifestChange>,
    dependency_updates: Vec<DependencyUpdate>,
    publish_order: Vec<String>,
    publish_commands: Vec<Vec<String>>,
    issues: Vec<String>,
    executed: Vec<Vec<String>>,
}

#[derive(Debug, Serialize)]
struct SkippedProject {
    name: String,
    reason: String,
}

struct ProjectTarget {
    name: String,
    root: PathBuf,
    workflow_manifests: Vec<PathBuf>,
    support_manifests: Vec<PathBuf>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum PublishPhase {
    Preflight,
    Upload,
}

struct PublishStep {
    phase: PublishPhase,
    command: Vec<String>,
}

pub(super) fn release_projects(
    service: &ApiService,
    args: &[String],
) -> CliResult<serde_json::Value> {
    let options = parse_options(args)?;
    let root = service.repo_root();
    let (projects, skipped) = selected_projects(service)?;
    let root_manifest = root.join("Cargo.toml");
    let mut manifest_paths = vec![root_manifest.clone()];
    let mut train_packages = BTreeSet::new();
    train_packages.insert(required_package_name(&root_manifest)?);

    for project in &projects {
        let workspace_manifest = project.root.join("Cargo.toml");
        if let Some(package) = validate_workspace_manifest(&workspace_manifest)? {
            train_packages.insert(package);
        }
        manifest_paths.push(workspace_manifest);
        for manifest in &project.workflow_manifests {
            train_packages.insert(required_package_name(manifest)?);
            manifest_paths.push(manifest.clone());
        }
        for manifest in &project.support_manifests {
            train_packages.insert(required_package_name(manifest)?);
            manifest_paths.push(manifest.clone());
        }
    }
    manifest_paths.sort();
    manifest_paths.dedup();

    let version = options.version.to_string();
    let mut planned = manifest_paths
        .iter()
        .map(|path| plan_manifest(root, path, &train_packages, &version))
        .collect::<CliResult<Vec<_>>>()?;

    if options.apply {
        apply_manifests(&planned)?;
    }

    let mut upload_started = false;
    let result = (|| {
        let mut publish_order = vec![
            train_packages
                .iter()
                .find(|package| package.as_str() == "lightflow")
                .cloned()
                .unwrap_or_else(|| "lightflow".to_owned()),
        ];
        let mut dry_run_commands = Vec::new();
        let root_plan = publish_release_root(root, false, options.allow_dirty)?;
        append_commands(&root_plan, &mut dry_run_commands);
        for project in &projects {
            for manifest in &project.support_manifests {
                publish_order.push(required_package_name(manifest)?);
                dry_run_commands.push(support_publish_command(manifest, options.allow_dirty));
            }
            let plan = publish_release_project(root, &project.name, false, options.allow_dirty)?;
            append_publish_order(&plan, &mut publish_order);
            append_commands(&plan, &mut dry_run_commands);
        }
        let publish_steps = interleaved_publish_steps(&dry_run_commands);
        let publish_commands = publish_steps
            .iter()
            .map(|step| step.command.clone())
            .collect::<Vec<_>>();

        let mut executed = Vec::new();
        if options.apply && options.publish {
            crate::cli::loop_check::ensure_loop_changes_valid(root)?;
            for (index, step) in publish_steps.iter().enumerate() {
                match step.phase {
                    PublishPhase::Preflight => {
                        let attempts = if index == 0 { 1 } else { 3 };
                        let attempts_used =
                            run_publish_preflight_with_retries(&step.command, attempts)?;
                        executed.extend(std::iter::repeat_n(step.command.clone(), attempts_used));
                    }
                    PublishPhase::Upload => {
                        upload_started = true;
                        run_publish_command(&step.command)?;
                        executed.push(step.command.clone());
                    }
                }
            }
        }

        let manifest_changes = planned
            .iter_mut()
            .map(|manifest| {
                std::mem::replace(
                    &mut manifest.change,
                    ManifestChange {
                        manifest: PathBuf::new(),
                        package: None,
                        old_version: None,
                        new_version: None,
                        changed: false,
                    },
                )
            })
            .collect();
        let dependency_updates = planned
            .iter_mut()
            .flat_map(|manifest| std::mem::take(&mut manifest.dependency_updates))
            .collect();
        Ok(serde_json::to_value(Report {
            dry_run: !options.apply,
            apply: options.apply,
            publish: options.publish,
            version,
            selected_projects: projects
                .iter()
                .map(|project| project.name.clone())
                .collect(),
            skipped_projects: skipped,
            manifest_changes,
            dependency_updates,
            publish_order,
            publish_commands: if options.publish {
                publish_commands
            } else {
                Vec::new()
            },
            issues: Vec::new(),
            executed,
        })?)
    })();

    if result.is_err() && options.apply && !upload_started {
        restore_manifests(&planned);
    }
    result
}

fn parse_options(args: &[String]) -> CliResult<Options> {
    let Some(raw_version) = args.first().filter(|value| !value.starts_with('-')) else {
        return Err(CliError::Usage(release_usage()));
    };
    let version = Version::parse(raw_version).map_err(|error| {
        CliError::Usage(format!("invalid release SemVer {raw_version:?}: {error}"))
    })?;
    let mut apply = false;
    let mut publish = false;
    let mut allow_dirty = false;
    for arg in &args[1..] {
        match arg.as_str() {
            "--apply" => apply = true,
            "--publish" => publish = true,
            "--allow-dirty" => allow_dirty = true,
            "--dry-run" => apply = false,
            "-h" | "--help" | "help" => return Err(CliError::Usage(release_usage())),
            _ => return Err(CliError::Usage(release_usage())),
        }
    }
    if allow_dirty && !publish {
        return Err(CliError::Usage(
            "--allow-dirty is only valid with --publish".to_owned(),
        ));
    }
    Ok(Options {
        version,
        apply,
        publish,
        allow_dirty,
    })
}

fn selected_projects(service: &ApiService) -> CliResult<(Vec<ProjectTarget>, Vec<SkippedProject>)> {
    let catalog = service.project_workspaces()?;
    if !catalog.project_config_valid {
        return Err(CliError::Usage(
            catalog
                .project_config_error
                .unwrap_or_else(|| "project workspace config is invalid".to_owned()),
        ));
    }
    let mut selected = Vec::new();
    let mut skipped = Vec::new();
    for workspace in catalog
        .workspaces
        .into_iter()
        .filter(|workspace| workspace.expected || workspace.optional)
    {
        if !workspace.exists {
            if workspace.optional {
                skipped.push(SkippedProject {
                    name: workspace.name,
                    reason: "optional workspace is not present".to_owned(),
                });
                continue;
            }
            return Err(workspace_error(&workspace, "missing expected workspace"));
        }
        if workspace.broken {
            return Err(workspace_error(&workspace, "project workspace is broken"));
        }
        let project_root = service.repo_root().join(&workspace.path);
        if !project_root.join("Cargo.toml").is_file() {
            return Err(workspace_error(&workspace, "Cargo.toml does not exist"));
        }
        let publish = service.workflow_publish_checks_for_project(&workspace.name)?;
        if publish.checks.is_empty() || workspace.workflow_crate_count == 0 {
            return Err(workspace_error(&workspace, "no workflow crates found"));
        }
        if publish.blocked_count > 0 {
            let issues = publish
                .checks
                .iter()
                .filter(|check| !check.publishable)
                .flat_map(|check| {
                    check
                        .issues
                        .iter()
                        .map(move |issue| format!("{}: {issue}", check.package))
                })
                .collect::<Vec<_>>();
            let blocked = serde_json::json!({
                "blocked_count": publish.blocked_count,
                "executed": Vec::<Vec<String>>::new(),
                "issues": issues,
            });
            return Err(workspace_error(
                &workspace,
                &format!("release requires every workflow crate to be publishable: {blocked}"),
            ));
        }
        if publish.publishable_count == 0 {
            return Err(workspace_error(
                &workspace,
                "no publishable workflow target found",
            ));
        }
        let workflow_manifests = publish
            .checks
            .into_iter()
            .map(|check| check.manifest)
            .collect::<Vec<_>>();
        let support_manifests = support::checked_support_manifests(
            &project_root.join("Cargo.toml"),
            &workflow_manifests,
        )
        .map_err(|error| workspace_error(&workspace, &error.to_string()))?;
        selected.push(ProjectTarget {
            name: workspace.name,
            root: project_root,
            workflow_manifests,
            support_manifests,
        });
    }
    Ok((selected, skipped))
}

fn support_publish_command(manifest: &std::path::Path, allow_dirty: bool) -> Vec<String> {
    let mut command = vec![
        "cargo".to_owned(),
        "publish".to_owned(),
        "--manifest-path".to_owned(),
        manifest.display().to_string(),
        "--dry-run".to_owned(),
    ];
    if allow_dirty {
        command.push("--allow-dirty".to_owned());
    }
    command
}

fn workspace_error(workspace: &ProjectWorkspaceSummary, issue: &str) -> CliError {
    CliError::Usage(format!("{}: {issue}", workspace.label))
}

fn append_commands(value: &serde_json::Value, commands: &mut Vec<Vec<String>>) {
    let source = value.get("commands").or_else(|| value.get("command"));
    if let Some(items) = source.and_then(serde_json::Value::as_array) {
        if items.first().is_some_and(serde_json::Value::is_string) {
            commands.push(json_command(items));
        } else {
            commands.extend(
                items
                    .iter()
                    .filter_map(|item| item.as_array().map(|command| json_command(command))),
            );
        }
    }
}

fn append_publish_order(value: &serde_json::Value, order: &mut Vec<String>) {
    if let Some(crates) = value.get("crates").and_then(serde_json::Value::as_array) {
        order.extend(crates.iter().filter_map(|item| {
            item.get("package")
                .and_then(serde_json::Value::as_str)
                .map(ToOwned::to_owned)
        }));
    }
}

fn interleaved_publish_steps(dry_run_commands: &[Vec<String>]) -> Vec<PublishStep> {
    let mut steps = Vec::with_capacity(dry_run_commands.len() * 2);
    for command in dry_run_commands {
        steps.push(PublishStep {
            phase: PublishPhase::Preflight,
            command: command.clone(),
        });
        steps.push(PublishStep {
            phase: PublishPhase::Upload,
            command: upload_command(command),
        });
    }
    steps
}

fn upload_command(preflight: &[String]) -> Vec<String> {
    preflight
        .iter()
        .filter(|argument| argument.as_str() != "--dry-run")
        .cloned()
        .collect()
}

fn json_command(values: &[serde_json::Value]) -> Vec<String> {
    values
        .iter()
        .filter_map(serde_json::Value::as_str)
        .map(ToOwned::to_owned)
        .collect()
}

#[cfg(test)]
mod support_tests;
#[cfg(test)]
mod tests;
