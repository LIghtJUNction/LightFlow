use super::project::{normalize_workflow_id, workflow_crate_dir_name};
use super::{CliError, CliResult, required_flag_value, run_status};
use serde_json::json;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use std::process::Command;
use toml_edit::{DocumentMut, Item};

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) struct PublishOptions {
    pub(super) target: PublishTarget,
    pub(super) apply: bool,
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub(super) enum PublishTarget {
    Root,
    Workflow(String),
    Crate(PathBuf),
}

pub(super) fn parse_publish_options(args: &[String]) -> CliResult<PublishOptions> {
    let mut target = None;
    let mut apply = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--apply" => {
                apply = true;
                index += 1;
            }
            "--dry-run" => {
                apply = false;
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
    })
}

pub(super) fn publish_crate(root: &Path, options: &PublishOptions) -> CliResult<serde_json::Value> {
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
    let issues = publish_issues(&document);
    let mut command = vec![
        "cargo".to_owned(),
        "publish".to_owned(),
        "--manifest-path".to_owned(),
        display_path(&manifest_path),
    ];
    if !options.apply {
        command.push("--dry-run".to_owned());
    }

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
        let manifest = path
            .join(workflow_crate_dir_name(workflow_id))
            .join("Cargo.toml");
        if manifest.exists() {
            return Ok(manifest);
        }
    }
    Ok(root.join("workflows").join(workflow_id).join("Cargo.toml"))
}

fn publish_target_json(target: &PublishTarget) -> serde_json::Value {
    match target {
        PublishTarget::Root => json!({ "kind": "root" }),
        PublishTarget::Workflow(workflow_id) => {
            json!({ "kind": "workflow", "workflow_id": workflow_id })
        }
        PublishTarget::Crate(path) => json!({ "kind": "crate", "path": path }),
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

fn publish_issues(document: &DocumentMut) -> Vec<String> {
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
    collect_publish_dependency_issues(
        document
            .get("workspace")
            .and_then(|workspace| workspace.get("dependencies")),
        &mut issues,
    );
    issues
}

fn collect_publish_dependency_issues(dependencies: Option<&Item>, issues: &mut Vec<String>) {
    let Some(dependencies) = dependencies.and_then(Item::as_table_like) else {
        return;
    };
    for (name, dependency) in dependencies.iter() {
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
}
