use super::workflow_crates::{discover_local_workflow_crates, display_path};
use super::{ApiError, ApiResult, WorkflowPublishCheck};
use crate::api::{dsl::read_workflow_source, util};
use crate::workflow::{PortSpec, WorkflowSpec};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item};

pub(super) fn workflow_publish_check_from_manifest(
    workflow_id: &str,
    manifest: PathBuf,
    root: &Path,
) -> ApiResult<WorkflowPublishCheck> {
    if !manifest.exists() {
        return Err(ApiError::NotFound(format!(
            "publish manifest does not exist: {}",
            manifest.display()
        )));
    }
    let source = fs::read_to_string(&manifest)?;
    let document = source
        .parse::<DocumentMut>()
        .map_err(|error| ApiError::InvalidRequest(format!("invalid Cargo manifest: {error}")))?;
    let workspace_document = workspace_document(root)?;
    let mut issues = publish_issues(&document, workspace_document.as_ref());
    issues.extend(workflow_publish_metadata_issues(&manifest));
    let package = package_field(&document, "name")?;
    let version = package_field(&document, "version")?;
    let package_by_dir = workflow_package_by_dir(root)?;
    let internal_dependencies = internal_path_dependencies(&manifest, &package_by_dir)?
        .into_iter()
        .collect();
    Ok(WorkflowPublishCheck {
        workflow_id: workflow_id.to_owned(),
        package,
        version,
        workspace: publish_workspace_label(root, &manifest),
        command: cargo_publish_command(&manifest),
        publishable: issues.is_empty(),
        issues,
        internal_dependencies,
        manifest,
    })
}

fn publish_workspace_label(root: &Path, manifest: &Path) -> String {
    let Ok(relative) = manifest.strip_prefix(root) else {
        return "external".to_owned();
    };
    let mut components = relative.components();
    match (components.next(), components.next()) {
        (Some(first), Some(name)) if first.as_os_str() == "projects" => {
            format!("projects/{}", name.as_os_str().to_string_lossy())
        }
        _ => "root".to_owned(),
    }
}

fn cargo_publish_command(manifest_path: &Path) -> Vec<String> {
    vec![
        "cargo".to_owned(),
        "publish".to_owned(),
        "--manifest-path".to_owned(),
        display_path(manifest_path),
        "--dry-run".to_owned(),
    ]
}

pub(super) fn categorized_workflow_manifest_path(
    root: &Path,
    workflow_id: &str,
) -> ApiResult<PathBuf> {
    let project_workflows = root.join(".lightflow").join("workflows");
    let workflows = root.join("workflows");
    let legacy_workflows = root.join("lightflow").join("workflows");
    let entries = match fs::read_dir(&project_workflows).or_else(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            fs::read_dir(&workflows).or_else(|error| {
                if error.kind() == std::io::ErrorKind::NotFound {
                    fs::read_dir(&legacy_workflows)
                } else {
                    Err(error)
                }
            })
        } else {
            Err(error)
        }
    }) {
        Ok(entries) => entries,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(project_workflows.join(workflow_id).join("Cargo.toml"));
        }
        Err(error) => return Err(ApiError::Io(error)),
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
            .join(util::workflow_crate_dir_name(workflow_id))
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
    Ok(project_workflows.join(workflow_id).join("Cargo.toml"))
}

fn workflow_category_short_name(workflow_id: &str, category: &str) -> Option<String> {
    let prefixed = workflow_id
        .strip_prefix("lightflow.")
        .unwrap_or(workflow_id);
    let short = prefixed.strip_prefix(category)?.strip_prefix('.')?;
    Some(short.replace('.', "_"))
}

fn workspace_document(root: &Path) -> ApiResult<Option<DocumentMut>> {
    let manifest_path = root.join("Cargo.toml");
    if !manifest_path.exists() {
        return Ok(None);
    }
    let source = fs::read_to_string(&manifest_path)?;
    let document = source
        .parse::<DocumentMut>()
        .map_err(|error| ApiError::InvalidRequest(format!("invalid Cargo manifest: {error}")))?;
    Ok(Some(document))
}

fn package_field(document: &DocumentMut, field: &str) -> ApiResult<String> {
    document
        .get("package")
        .and_then(|package| package.get(field))
        .and_then(Item::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            ApiError::InvalidRequest(format!("Cargo manifest is missing package.{field}"))
        })
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

fn workflow_publish_metadata_issues(manifest: &Path) -> Vec<String> {
    let Some(crate_dir) = manifest.parent() else {
        return Vec::new();
    };
    let lib = crate_dir.join("src").join("lib.rs");
    if !lib.exists() {
        return Vec::new();
    }
    match read_workflow_source(&lib) {
        Ok(workflow) => workflow_placeholder_issues(&workflow),
        Err(error) => vec![format!("workflow source cannot be parsed: {error}")],
    }
}

fn workflow_placeholder_issues(workflow: &WorkflowSpec) -> Vec<String> {
    let mut issues = Vec::new();
    if unresolved_placeholder(workflow.description.as_deref()) {
        issues.push("workflow.description contains unresolved TODO".to_owned());
    }
    collect_port_placeholder_issues("input", &workflow.inputs, &mut issues);
    collect_port_placeholder_issues("output", &workflow.outputs, &mut issues);
    issues
}

fn collect_port_placeholder_issues(kind: &str, ports: &[PortSpec], issues: &mut Vec<String>) {
    for port in ports {
        if unresolved_placeholder(port.description.as_deref()) {
            issues.push(format!(
                "workflow.{kind}.{}.description contains unresolved TODO",
                port.name
            ));
        }
    }
}

fn unresolved_placeholder(value: Option<&str>) -> bool {
    value.is_some_and(|value| value.to_ascii_lowercase().contains("todo"))
}

pub(super) fn workflow_package_by_dir(root: &Path) -> ApiResult<BTreeMap<PathBuf, String>> {
    let mut packages = BTreeMap::new();
    for crate_dir in discover_local_workflow_crates(root)? {
        let manifest = crate_dir.join("Cargo.toml");
        let source = fs::read_to_string(&manifest)?;
        let document = source.parse::<DocumentMut>().map_err(|error| {
            ApiError::InvalidRequest(format!("invalid Cargo manifest: {error}"))
        })?;
        if let Ok(package) = package_field(&document, "name")
            && let Ok(crate_dir) = crate_dir.canonicalize()
        {
            packages.insert(crate_dir, package);
        }
    }
    Ok(packages)
}

pub(super) fn internal_path_dependencies(
    manifest_path: &Path,
    package_by_dir: &BTreeMap<PathBuf, String>,
) -> ApiResult<BTreeSet<String>> {
    let source = fs::read_to_string(manifest_path)?;
    let document = source
        .parse::<DocumentMut>()
        .map_err(|error| ApiError::InvalidRequest(format!("invalid Cargo manifest: {error}")))?;
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
) -> ApiResult<()> {
    let Some(dependencies) = dependencies.and_then(Item::as_table_like) else {
        return Ok(());
    };
    for (_name, dependency) in dependencies.iter() {
        let Some(path) = dependency.get("path").and_then(Item::as_str) else {
            continue;
        };
        let dependency_dir = manifest_dir.join(path);
        if let Ok(dependency_dir) = dependency_dir.canonicalize()
            && let Some(package) = package_by_dir.get(&dependency_dir)
        {
            internal_dependencies.insert(package.clone());
        }
    }
    Ok(())
}

pub(super) fn order_workflow_publish_checks(
    checks: &mut Vec<WorkflowPublishCheck>,
) -> ApiResult<()> {
    let mut pending = std::mem::take(checks);
    pending.sort_by(|left, right| left.workflow_id.cmp(&right.workflow_id));
    let mut published = BTreeSet::new();
    let mut ordered = Vec::new();
    while !pending.is_empty() {
        let ready = pending.iter().position(|check| {
            check
                .internal_dependencies
                .iter()
                .all(|dependency| published.contains(dependency))
        });
        let Some(index) = ready else {
            return Err(ApiError::InvalidRequest(
                "workflow crate path dependencies contain a cycle".to_owned(),
            ));
        };
        let check = pending.remove(index);
        published.insert(check.package.clone());
        ordered.push(check);
    }
    *checks = ordered;
    Ok(())
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
