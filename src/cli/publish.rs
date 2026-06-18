use super::project::{normalize_workflow_id, workflow_crate_dir_name};
use super::{CliError, CliResult, required_flag_value, run_status};
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use toml_edit::{DocumentMut, Item};

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct PublishOptions {
    pub(super) target: PublishTarget,
    pub(super) apply: bool,
    pub(super) allow_dirty: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) enum PublishTarget {
    Root,
    Workflow(String),
    Crate(PathBuf),
    Workflows,
}

pub(super) fn parse_publish_options(args: &[String]) -> CliResult<PublishOptions> {
    let mut target = None;
    let mut apply = false;
    let mut allow_dirty = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--apply" => {
                apply = true;
                index += 1;
            }
            "--workflows" => {
                if target.is_some() {
                    return Err(CliError::Usage(
                        "publish accepts only one target".to_owned(),
                    ));
                }
                target = Some(PublishTarget::Workflows);
                index += 1;
            }
            "--dry-run" => {
                apply = false;
                index += 1;
            }
            "--allow-dirty" => {
                allow_dirty = true;
                index += 1;
            }
            "--crate" => {
                if target.is_some() {
                    return Err(CliError::Usage(
                        "publish accepts only one target".to_owned(),
                    ));
                }
                target = Some(PublishTarget::Crate(PathBuf::from(required_flag_value(
                    args, index, "--crate",
                )?)));
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::Usage(format!(
                    "unexpected argument for publish: {value}"
                )));
            }
            value => {
                if target.is_some() {
                    return Err(CliError::Usage(
                        "publish accepts only one target".to_owned(),
                    ));
                }
                target = Some(PublishTarget::Workflow(normalize_workflow_id(value)));
                index += 1;
            }
        }
    }
    Ok(PublishOptions {
        target: target.unwrap_or(PublishTarget::Root),
        apply,
        allow_dirty,
    })
}

pub(super) fn publish_crate(root: &Path, options: &PublishOptions) -> CliResult<serde_json::Value> {
    if matches!(options.target, PublishTarget::Workflows) {
        return publish_workflow_crates(root, options.apply, options.allow_dirty);
    }
    let manifest_path = publish_manifest_path(root, &options.target)?;
    if !manifest_path.exists() {
        return Err(CliError::Usage(format!(
            "publish manifest does not exist: {}",
            manifest_path.display()
        )));
    }
    let source = fs::read_to_string(&manifest_path)?;
    let document = source
        .parse::<DocumentMut>()
        .map_err(|error| CliError::Usage(format!("invalid Cargo manifest: {error}")))?;
    let package = package_field(&document, "name")?;
    let version = package_field(&document, "version")?;
    let workspace_document = workspace_document(root)?;
    let issues = publish_issues(&document, workspace_document.as_ref());
    let command = cargo_publish_command(&manifest_path, !options.apply, options.allow_dirty);

    if options.apply {
        if !issues.is_empty() {
            return Err(CliError::Usage(format!(
                "crate is not publishable: {}",
                issues.join("; ")
            )));
        }
        let mut process = Command::new("cargo");
        for arg in &command[1..] {
            process.arg(arg);
        }
        run_status(&mut process)?;
    }

    Ok(json!({
        "dry_run": !options.apply,
        "target": publish_target_json(&options.target),
        "manifest": manifest_path,
        "package": package,
        "version": version,
        "publishable": issues.is_empty(),
        "issues": issues,
        "command": command,
        "executed": if options.apply { vec![command] } else { Vec::<Vec<String>>::new() },
    }))
}

fn display_path(path: &Path) -> String {
    path.strip_prefix(".").unwrap_or(path).display().to_string()
}

fn publish_manifest_path(root: &Path, target: &PublishTarget) -> CliResult<PathBuf> {
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

fn publish_target_json(target: &PublishTarget) -> serde_json::Value {
    match target {
        PublishTarget::Root => json!({ "kind": "root" }),
        PublishTarget::Workflow(workflow_id) => {
            json!({ "kind": "workflow", "workflow_id": workflow_id })
        }
        PublishTarget::Crate(path) => json!({ "kind": "crate", "path": path }),
        PublishTarget::Workflows => json!({ "kind": "workflows" }),
    }
}

fn package_field(document: &DocumentMut, field: &str) -> CliResult<String> {
    document
        .get("package")
        .and_then(|package| package.get(field))
        .and_then(Item::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| CliError::Usage(format!("Cargo manifest is missing package.{field}")))
}

fn publish_workflow_crates(
    root: &Path,
    apply: bool,
    allow_dirty: bool,
) -> CliResult<serde_json::Value> {
    let manifests = discover_workflow_manifests(root)?;
    if manifests.is_empty() {
        return Err(CliError::Usage(
            "no workflow crates found under workflows/*/*".to_owned(),
        ));
    }
    let workspace_document = workspace_document(root)?;
    let mut plans = Vec::new();
    for manifest_path in manifests {
        plans.push(workflow_publish_plan(
            &manifest_path,
            workspace_document.as_ref(),
            apply,
            allow_dirty,
        )?);
    }
    order_workflow_publish_plans(&mut plans)?;

    let publishable = plans.iter().all(|plan| plan.issues.is_empty());
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

    Ok(json!({
        "dry_run": !apply,
        "target": publish_target_json(&PublishTarget::Workflows),
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
    }))
}

#[derive(Debug)]
struct WorkflowPublishPlan {
    manifest_path: PathBuf,
    package: String,
    version: String,
    issues: Vec<String>,
    command: Vec<String>,
    internal_dependencies: BTreeSet<String>,
}

impl WorkflowPublishPlan {
    fn to_json(&self) -> serde_json::Value {
        json!({
            "manifest": self.manifest_path,
            "package": self.package,
            "version": self.version,
            "publishable": self.issues.is_empty(),
            "issues": self.issues,
            "command": self.command,
            "internal_dependencies": self.internal_dependencies,
        })
    }
}

fn workflow_publish_plan(
    manifest_path: &Path,
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
        package: package_field(&document, "name")?,
        version: package_field(&document, "version")?,
        issues: publish_issues(&document, workspace_document),
        command: cargo_publish_command(manifest_path, !apply, allow_dirty),
        internal_dependencies: BTreeSet::new(),
    })
}

fn discover_workflow_manifests(root: &Path) -> CliResult<Vec<PathBuf>> {
    let workflows = root.join("workflows");
    let legacy_workflows = root.join("lightflow").join("workflows");
    let source_root = if workflows.exists() {
        workflows
    } else {
        legacy_workflows
    };
    if !source_root.exists() {
        return Ok(Vec::new());
    }

    let mut manifests = Vec::new();
    for entry in sorted_dir_entries(&source_root)? {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if is_workflow_crate_dir(&path) {
            manifests.push(path.join("Cargo.toml"));
            continue;
        }
        for child in sorted_dir_entries(&path)? {
            let crate_dir = child.path();
            if crate_dir.is_dir() && is_workflow_crate_dir(&crate_dir) {
                manifests.push(crate_dir.join("Cargo.toml"));
            }
        }
    }
    manifests.sort();
    Ok(manifests)
}

fn sorted_dir_entries(path: &Path) -> CliResult<Vec<fs::DirEntry>> {
    let mut entries = fs::read_dir(path)?.collect::<Result<Vec<_>, io::Error>>()?;
    entries.sort_by_key(|entry| entry.path());
    Ok(entries)
}

fn is_workflow_crate_dir(path: &Path) -> bool {
    path.join("Cargo.toml").exists() && path.join("src").join("lib.rs").exists()
}

fn order_workflow_publish_plans(plans: &mut Vec<WorkflowPublishPlan>) -> CliResult<()> {
    let package_by_dir = plans
        .iter()
        .filter_map(|plan| {
            plan.manifest_path
                .parent()
                .and_then(|dir| canonicalize_existing(dir).ok())
                .map(|dir| (dir, plan.package.clone()))
        })
        .collect::<BTreeMap<_, _>>();

    for plan in plans.iter_mut() {
        plan.internal_dependencies =
            internal_path_dependencies(&plan.manifest_path, &package_by_dir)?;
    }

    let mut pending = plans.drain(..).collect::<Vec<_>>();
    let mut published = BTreeSet::new();
    let mut ordered = Vec::new();
    while !pending.is_empty() {
        let ready = pending
            .iter()
            .position(|plan| plan.internal_dependencies.is_subset(&published));
        let Some(index) = ready else {
            return Err(CliError::Usage(
                "workflow crate path dependencies contain a cycle".to_owned(),
            ));
        };
        let plan = pending.remove(index);
        published.insert(plan.package.clone());
        ordered.push(plan);
    }
    *plans = ordered;
    Ok(())
}

fn internal_path_dependencies(
    manifest_path: &Path,
    package_by_dir: &BTreeMap<PathBuf, String>,
) -> CliResult<BTreeSet<String>> {
    let source = fs::read_to_string(manifest_path)?;
    let document = source
        .parse::<DocumentMut>()
        .map_err(|error| CliError::Usage(format!("invalid Cargo manifest: {error}")))?;
    let manifest_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    let mut dependencies = BTreeSet::new();
    collect_internal_path_dependencies(
        document.get("dependencies"),
        manifest_dir,
        package_by_dir,
        &mut dependencies,
    )?;
    collect_internal_path_dependencies(
        document.get("build-dependencies"),
        manifest_dir,
        package_by_dir,
        &mut dependencies,
    )?;
    collect_internal_path_dependencies(
        document.get("dev-dependencies"),
        manifest_dir,
        package_by_dir,
        &mut dependencies,
    )?;
    Ok(dependencies)
}

fn collect_internal_path_dependencies(
    dependencies: Option<&Item>,
    manifest_dir: &Path,
    package_by_dir: &BTreeMap<PathBuf, String>,
    internal_dependencies: &mut BTreeSet<String>,
) -> CliResult<()> {
    let Some(dependencies) = dependencies.and_then(Item::as_table_like) else {
        return Ok(());
    };
    for (_name, dependency) in dependencies.iter() {
        let Some(path) = dependency.get("path").and_then(Item::as_str) else {
            continue;
        };
        let dependency_dir = manifest_dir.join(path);
        if let Ok(dependency_dir) = canonicalize_existing(&dependency_dir) {
            if let Some(package) = package_by_dir.get(&dependency_dir) {
                internal_dependencies.insert(package.clone());
            }
        }
    }
    Ok(())
}

fn canonicalize_existing(path: &Path) -> io::Result<PathBuf> {
    path.canonicalize()
}

fn workspace_document(root: &Path) -> CliResult<Option<DocumentMut>> {
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

fn cargo_publish_command(manifest_path: &Path, dry_run: bool, allow_dirty: bool) -> Vec<String> {
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

fn run_cargo_command(command: &[String]) -> CliResult<()> {
    let mut process = Command::new("cargo");
    for arg in &command[1..] {
        process.arg(arg);
    }
    run_status(&mut process)
}

fn publish_issues(document: &DocumentMut, workspace_document: Option<&DocumentMut>) -> Vec<String> {
    let mut issues = Vec::new();
    let package = document.get("package");
    if package
        .and_then(|package| package.get("publish"))
        .and_then(Item::as_bool)
        == Some(false)
    {
        issues.push("package.publish is false".to_owned());
    }
    match package
        .and_then(|package| package.get("version"))
        .and_then(Item::as_str)
    {
        Some(version) if semver::Version::parse(version).is_err() => {
            issues.push(format!("package.version {version} is not semantic version"));
        }
        Some(_) => {}
        None => issues.push("package.version is missing".to_owned()),
    }
    if package
        .and_then(|package| package.get("description"))
        .and_then(Item::as_str)
        .is_none_or(str::is_empty)
    {
        issues.push("package.description is missing".to_owned());
    }
    let has_license = package
        .and_then(|package| package.get("license"))
        .and_then(Item::as_str)
        .is_some_and(|license| !license.is_empty())
        || package
            .and_then(|package| package.get("license-file"))
            .and_then(Item::as_str)
            .is_some_and(|license_file| !license_file.is_empty());
    if !has_license {
        issues.push("package.license or package.license-file is missing".to_owned());
    }
    collect_publish_dependency_issues(document.get("dependencies"), &mut issues);
    collect_publish_dependency_issues(document.get("build-dependencies"), &mut issues);
    collect_publish_dependency_issues(document.get("dev-dependencies"), &mut issues);
    collect_publish_dependency_issues(
        document
            .get("workspace")
            .and_then(|workspace| workspace.get("dependencies")),
        &mut issues,
    );
    collect_inherited_publish_dependency_issues(document, workspace_document, &mut issues);
    issues
}

fn collect_publish_dependency_issues(dependencies: Option<&Item>, issues: &mut Vec<String>) {
    let Some(dependencies) = dependencies.and_then(Item::as_table_like) else {
        return;
    };
    for (name, dependency) in dependencies.iter() {
        collect_publish_dependency_issue(name, dependency, issues);
    }
}

fn collect_publish_dependency_issue(name: &str, dependency: &Item, issues: &mut Vec<String>) {
    if dependency.get("git").is_some() {
        issues.push(format!(
            "dependency {name} uses git, which cannot be published to crates.io"
        ));
    }
    if dependency.get("path").is_some() && dependency.get("version").is_none() {
        issues.push(format!(
            "dependency {name} uses path without a crates.io version"
        ));
    }
}

fn collect_inherited_publish_dependency_issues(
    document: &DocumentMut,
    workspace_document: Option<&DocumentMut>,
    issues: &mut Vec<String>,
) {
    let Some(workspace_dependencies) = workspace_document
        .and_then(|document| document.get("workspace"))
        .and_then(|workspace| workspace.get("dependencies"))
        .and_then(Item::as_table_like)
    else {
        return;
    };
    collect_inherited_publish_dependency_section_issues(
        document.get("dependencies"),
        workspace_dependencies,
        issues,
    );
    collect_inherited_publish_dependency_section_issues(
        document.get("build-dependencies"),
        workspace_dependencies,
        issues,
    );
    collect_inherited_publish_dependency_section_issues(
        document.get("dev-dependencies"),
        workspace_dependencies,
        issues,
    );
}

fn collect_inherited_publish_dependency_section_issues(
    dependencies: Option<&Item>,
    workspace_dependencies: &dyn toml_edit::TableLike,
    issues: &mut Vec<String>,
) {
    let Some(dependencies) = dependencies.and_then(Item::as_table_like) else {
        return;
    };
    for (name, dependency) in dependencies.iter() {
        if dependency.get("workspace").and_then(Item::as_bool) != Some(true) {
            continue;
        }
        let Some(workspace_dependency) = workspace_dependencies.get(name) else {
            continue;
        };
        collect_publish_dependency_issue(name, workspace_dependency, issues);
    }
}
