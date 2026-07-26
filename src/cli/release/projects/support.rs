use super::manifests::{
    publishable_workspace_member_manifests, read_manifest, required_package_name,
    workspace_member_manifests,
};
use crate::api::{
    internal_path_dependency_packages, path_dependency_release_issues, publish_issues,
};
use crate::cli::{CliError, CliResult};
use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

pub(super) fn checked_support_manifests(
    workspace_manifest: &Path,
    workflow_manifests: &[PathBuf],
) -> CliResult<Vec<PathBuf>> {
    let workspace_root = workspace_manifest
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let workspace_document = read_manifest(workspace_manifest)?;
    let member_manifests = workspace_member_manifests(workspace_manifest)?;
    let publishable_members = publishable_workspace_member_manifests(workspace_manifest)?
        .into_iter()
        .collect::<BTreeSet<_>>();
    let workflow_manifests = workflow_manifests
        .iter()
        .map(|manifest| normalize(manifest))
        .collect::<BTreeSet<_>>();
    let support_manifests = publishable_members
        .iter()
        .filter(|manifest| !workflow_manifests.contains(*manifest))
        .cloned()
        .collect::<Vec<_>>();

    let mut package_by_dir = BTreeMap::new();
    for manifest in &member_manifests {
        let package = required_package_name(manifest)?;
        let directory = manifest.parent().unwrap_or(workspace_root);
        let directory = directory
            .canonicalize()
            .unwrap_or_else(|_| directory.to_path_buf());
        if let Some(previous) = package_by_dir.insert(directory, package.clone()) {
            return Err(CliError::Usage(format!(
                "workspace release catalog has duplicate package mapping for {package} and {previous}"
            )));
        }
    }

    let mut documents = BTreeMap::new();
    let mut manifests_by_package = BTreeMap::new();
    let mut issues = Vec::new();
    for manifest in &support_manifests {
        let document = read_manifest(manifest)?;
        let package = required_package_name(manifest)?;
        if manifests_by_package
            .insert(package.clone(), manifest.clone())
            .is_some()
        {
            issues.push(format!(
                "support package {package} is declared more than once"
            ));
        }
        for issue in publish_issues(&document, Some(&workspace_document)) {
            issues.push(format!("{package}: {issue}"));
        }
        for issue in path_dependency_release_issues(
            manifest,
            &document,
            Some(&workspace_document),
            workspace_root,
        ) {
            issues.push(format!("{package}: {issue}"));
        }
        documents.insert(package, document);
    }
    if !issues.is_empty() {
        return blocked_support(issues);
    }

    let support_packages = manifests_by_package
        .keys()
        .cloned()
        .collect::<BTreeSet<_>>();
    let workspace_packages = package_by_dir.values().cloned().collect::<BTreeSet<_>>();
    let mut dependencies = BTreeMap::new();
    for (package, manifest) in &manifests_by_package {
        let document = documents
            .get(package)
            .expect("support document collected with manifest");
        let internal = internal_path_dependency_packages(
            document,
            Some(&workspace_document),
            manifest.parent().unwrap_or(workspace_root),
            workspace_root,
            &package_by_dir,
        );
        let invalid = internal
            .iter()
            .filter(|dependency| {
                workspace_packages.contains(*dependency) && !support_packages.contains(*dependency)
            })
            .cloned()
            .collect::<Vec<_>>();
        if !invalid.is_empty() {
            issues.push(format!(
                "support package {package} depends on non-support workspace package(s): {}",
                invalid.join(", ")
            ));
        }
        dependencies.insert(
            package.clone(),
            internal
                .intersection(&support_packages)
                .cloned()
                .collect::<BTreeSet<_>>(),
        );
    }
    if !issues.is_empty() {
        return blocked_support(issues);
    }

    let ordered_packages = topological_support_order(&dependencies)?;
    Ok(ordered_packages
        .into_iter()
        .map(|package| {
            manifests_by_package
                .remove(&package)
                .expect("ordered support package has manifest")
        })
        .collect())
}

pub(super) fn topological_support_order(
    dependencies: &BTreeMap<String, BTreeSet<String>>,
) -> CliResult<Vec<String>> {
    let mut remaining = dependencies.clone();
    let mut ordered = Vec::with_capacity(remaining.len());
    while !remaining.is_empty() {
        let ready = remaining
            .iter()
            .filter(|(_, dependencies)| dependencies.is_empty())
            .map(|(package, _)| package.clone())
            .collect::<Vec<_>>();
        if ready.is_empty() {
            return blocked_support(vec![format!(
                "support package dependency cycle: {}",
                remaining.keys().cloned().collect::<Vec<_>>().join(", ")
            )]);
        }
        for package in ready {
            remaining.remove(&package);
            for dependencies in remaining.values_mut() {
                dependencies.remove(&package);
            }
            ordered.push(package);
        }
    }
    Ok(ordered)
}

fn blocked_support<T>(issues: Vec<String>) -> CliResult<T> {
    Err(CliError::Usage(
        serde_json::json!({
            "blocked_count": issues.len(),
            "executed": Vec::<Vec<String>>::new(),
            "issues": issues,
        })
        .to_string(),
    ))
}

fn normalize(path: &Path) -> PathBuf {
    path.canonicalize().unwrap_or_else(|_| path.to_path_buf())
}
