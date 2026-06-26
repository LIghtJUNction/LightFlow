use crate::api::read_workflow_source;
use crate::workflow::{PortSpec, WorkflowSpec};
use std::path::Path;
use toml_edit::{DocumentMut, Item};

pub(super) fn publish_issues(
    document: &DocumentMut,
    workspace_document: Option<&DocumentMut>,
) -> Vec<String> {
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

pub(super) fn workflow_publish_metadata_issues(manifest: &Path) -> Vec<String> {
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

pub(super) fn workflow_id_from_manifest(manifest: &Path) -> Option<String> {
    let crate_dir = manifest.parent()?;
    let lib = crate_dir.join("src").join("lib.rs");
    read_workflow_source(&lib).ok().map(|workflow| workflow.id)
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
