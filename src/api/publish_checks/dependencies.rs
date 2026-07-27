use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item};

mod dependency_issues;

pub(crate) fn publish_issues(
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
    dependency_issues::collect(document, workspace_document, &mut issues);
    issues
}

pub(crate) fn path_dependency_release_issues(
    manifest_path: &Path,
    document: &DocumentMut,
    workspace_document: Option<&DocumentMut>,
    workspace_root: &Path,
) -> Vec<String> {
    let mut issues = Vec::new();
    let workspace_members = workspace_document.and_then(|workspace| {
        match resolve_workspace_member_manifests(&workspace_root.join("Cargo.toml"), workspace) {
            Ok(members) => Some(members.into_iter().collect::<BTreeSet<_>>()),
            Err(error) => {
                issues.push(format!("cannot resolve workspace release catalog: {error}"));
                None
            }
        }
    });
    let manifest_dir = manifest_path.parent().unwrap_or_else(|| Path::new("."));
    collect_path_dependency_release_issues(
        document,
        manifest_dir,
        workspace_document,
        Some(workspace_root),
        workspace_members.as_ref(),
        &mut issues,
    );
    issues
}

fn collect_path_dependency_release_issues(
    document: &DocumentMut,
    manifest_dir: &Path,
    workspace_document: Option<&DocumentMut>,
    workspace_root: Option<&Path>,
    workspace_members: Option<&BTreeSet<PathBuf>>,
    issues: &mut Vec<String>,
) {
    for section in ["dependencies", "build-dependencies", "dev-dependencies"] {
        inspect_dependency_section(
            document.get(section),
            manifest_dir,
            workspace_document,
            workspace_root,
            workspace_members,
            issues,
        );
    }
    if let Some(targets) = document.get("target").and_then(Item::as_table_like) {
        for (_target, target) in targets.iter() {
            for section in ["dependencies", "build-dependencies", "dev-dependencies"] {
                inspect_dependency_section(
                    target.get(section),
                    manifest_dir,
                    workspace_document,
                    workspace_root,
                    workspace_members,
                    issues,
                );
            }
        }
    }
}

fn inspect_dependency_section(
    dependencies: Option<&Item>,
    manifest_dir: &Path,
    workspace_document: Option<&DocumentMut>,
    workspace_root: Option<&Path>,
    workspace_members: Option<&BTreeSet<PathBuf>>,
    issues: &mut Vec<String>,
) {
    let Some(dependencies) = dependencies.and_then(Item::as_table_like) else {
        return;
    };
    for (name, dependency) in dependencies.iter() {
        if dependency.get("workspace").and_then(Item::as_bool) == Some(true) {
            let Some(workspace_dependency) = workspace_document
                .and_then(|document| document.get("workspace"))
                .and_then(|workspace| workspace.get("dependencies"))
                .and_then(Item::as_table_like)
                .and_then(|dependencies| dependencies.get(name))
            else {
                continue;
            };
            if let Some(root) = workspace_root {
                inspect_path_dependency(
                    name,
                    workspace_dependency,
                    root,
                    workspace_document,
                    workspace_root,
                    workspace_members,
                    issues,
                );
            }
        } else {
            inspect_path_dependency(
                name,
                dependency,
                manifest_dir,
                workspace_document,
                workspace_root,
                workspace_members,
                issues,
            );
        }
    }
}

fn inspect_path_dependency(
    name: &str,
    dependency: &Item,
    base: &Path,
    workspace_document: Option<&DocumentMut>,
    workspace_root: Option<&Path>,
    workspace_members: Option<&BTreeSet<PathBuf>>,
    issues: &mut Vec<String>,
) {
    let Some(path) = dependency.get("path").and_then(Item::as_str) else {
        return;
    };
    let target_manifest = base.join(path).join("Cargo.toml");
    let target_document = match fs::read_to_string(&target_manifest)
        .ok()
        .and_then(|source| source.parse::<DocumentMut>().ok())
    {
        Some(document) => document,
        None => {
            issues.push(format!(
                "dependency {name} path target has no readable Cargo.toml: {}",
                target_manifest.display()
            ));
            return;
        }
    };
    if target_document
        .get("package")
        .and_then(|package| package.get("publish"))
        .and_then(Item::as_bool)
        == Some(false)
    {
        issues.push(format!(
            "dependency {name} path target has package.publish = false"
        ));
    }
    if let (Some(_workspace), Some(workspace_root), Some(workspace_members)) =
        (workspace_document, workspace_root, workspace_members)
        && manifest_is_within_workspace(workspace_root, &target_manifest)
        && !workspace_includes_manifest(workspace_members, &target_manifest)
    {
        issues.push(format!(
            "dependency {name} path target is excluded from the workspace release catalog"
        ));
    }
}

fn manifest_is_within_workspace(workspace_root: &Path, target_manifest: &Path) -> bool {
    let canonical_root = workspace_root
        .canonicalize()
        .unwrap_or_else(|_| workspace_root.to_path_buf());
    let target_dir = target_manifest.parent().unwrap_or(target_manifest);
    let canonical_target = target_dir
        .canonicalize()
        .unwrap_or_else(|_| target_dir.to_path_buf());
    canonical_target.starts_with(canonical_root)
}

fn workspace_includes_manifest(
    workspace_members: &BTreeSet<PathBuf>,
    target_manifest: &Path,
) -> bool {
    workspace_members.contains(&normalized_manifest(target_manifest))
}

pub(crate) fn resolve_workspace_member_manifests(
    workspace_manifest: &Path,
    workspace: &DocumentMut,
) -> Result<Vec<PathBuf>, String> {
    let workspace_root = workspace_manifest
        .parent()
        .unwrap_or_else(|| Path::new("."));
    let members = workspace
        .get("workspace")
        .and_then(|workspace| workspace.get("members"))
        .and_then(Item::as_array);
    let excludes = workspace
        .get("workspace")
        .and_then(|workspace| workspace.get("exclude"))
        .and_then(Item::as_array);
    let mut manifests = BTreeSet::new();
    if workspace.get("package").is_some() {
        manifests.insert(normalized_manifest(workspace_manifest));
    }
    if let Some(members) = members {
        for pattern in members.iter().filter_map(toml_edit::Value::as_str) {
            manifests.extend(expand_workspace_pattern(workspace_root, pattern)?);
        }
    }
    if let Some(excludes) = excludes {
        for pattern in excludes.iter().filter_map(toml_edit::Value::as_str) {
            for manifest in expand_workspace_pattern(workspace_root, pattern)? {
                manifests.remove(&manifest);
            }
        }
    }
    Ok(manifests.into_iter().collect())
}

fn expand_workspace_pattern(root: &Path, pattern: &str) -> Result<Vec<PathBuf>, String> {
    let absolute_pattern = root.join(pattern);
    let pattern = absolute_pattern.to_str().ok_or_else(|| {
        format!(
            "workspace member pattern is not valid UTF-8: {}",
            absolute_pattern.display()
        )
    })?;
    let entries = glob::glob(pattern)
        .map_err(|error| format!("invalid workspace member pattern {pattern:?}: {error}"))?;
    let mut manifests = Vec::new();
    for entry in entries {
        let path = entry.map_err(|error| {
            format!("cannot expand workspace member pattern {pattern:?}: {error}")
        })?;
        let manifest = if path.file_name().is_some_and(|name| name == "Cargo.toml") {
            path
        } else {
            path.join("Cargo.toml")
        };
        if manifest.is_file() {
            manifests.push(normalized_manifest(&manifest));
        }
    }
    Ok(manifests)
}

fn normalized_manifest(manifest: &Path) -> PathBuf {
    manifest
        .canonicalize()
        .unwrap_or_else(|_| manifest.to_path_buf())
}

pub(crate) fn internal_path_dependency_packages(
    document: &DocumentMut,
    workspace_document: Option<&DocumentMut>,
    manifest_dir: &Path,
    workspace_root: &Path,
    package_by_dir: &BTreeMap<PathBuf, String>,
) -> BTreeSet<String> {
    let mut dependencies = BTreeSet::new();
    collect_internal_path_dependency_packages(
        document.get("dependencies"),
        manifest_dir,
        package_by_dir,
        &mut dependencies,
    );
    collect_internal_path_dependency_packages(
        document.get("build-dependencies"),
        manifest_dir,
        package_by_dir,
        &mut dependencies,
    );
    collect_internal_path_dependency_packages(
        document.get("dev-dependencies"),
        manifest_dir,
        package_by_dir,
        &mut dependencies,
    );
    collect_target_internal_path_dependency_packages(
        document,
        manifest_dir,
        package_by_dir,
        &mut dependencies,
    );
    collect_inherited_internal_path_dependency_packages(
        document,
        workspace_document,
        workspace_root,
        package_by_dir,
        &mut dependencies,
    );
    dependencies
}

fn collect_internal_path_dependency_packages(
    dependencies: Option<&Item>,
    manifest_dir: &Path,
    package_by_dir: &BTreeMap<PathBuf, String>,
    internal_dependencies: &mut BTreeSet<String>,
) {
    let Some(dependencies) = dependencies.and_then(Item::as_table_like) else {
        return;
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
}

fn collect_target_internal_path_dependency_packages(
    document: &DocumentMut,
    manifest_dir: &Path,
    package_by_dir: &BTreeMap<PathBuf, String>,
    internal_dependencies: &mut BTreeSet<String>,
) {
    let Some(targets) = document.get("target").and_then(Item::as_table_like) else {
        return;
    };
    for (_target, target) in targets.iter() {
        collect_internal_path_dependency_packages(
            target.get("dependencies"),
            manifest_dir,
            package_by_dir,
            internal_dependencies,
        );
        collect_internal_path_dependency_packages(
            target.get("build-dependencies"),
            manifest_dir,
            package_by_dir,
            internal_dependencies,
        );
        collect_internal_path_dependency_packages(
            target.get("dev-dependencies"),
            manifest_dir,
            package_by_dir,
            internal_dependencies,
        );
    }
}

fn collect_inherited_internal_path_dependency_packages(
    document: &DocumentMut,
    workspace_document: Option<&DocumentMut>,
    workspace_root: &Path,
    package_by_dir: &BTreeMap<PathBuf, String>,
    internal_dependencies: &mut BTreeSet<String>,
) {
    let Some(workspace_dependencies) = workspace_document
        .and_then(|document| document.get("workspace"))
        .and_then(|workspace| workspace.get("dependencies"))
        .and_then(Item::as_table_like)
    else {
        return;
    };
    collect_inherited_internal_path_dependency_section_packages(
        document.get("dependencies"),
        workspace_dependencies,
        workspace_root,
        package_by_dir,
        internal_dependencies,
    );
    collect_inherited_internal_path_dependency_section_packages(
        document.get("build-dependencies"),
        workspace_dependencies,
        workspace_root,
        package_by_dir,
        internal_dependencies,
    );
    collect_inherited_internal_path_dependency_section_packages(
        document.get("dev-dependencies"),
        workspace_dependencies,
        workspace_root,
        package_by_dir,
        internal_dependencies,
    );
    collect_inherited_target_internal_path_dependency_packages(
        document,
        workspace_dependencies,
        workspace_root,
        package_by_dir,
        internal_dependencies,
    );
}

fn collect_inherited_internal_path_dependency_section_packages(
    dependencies: Option<&Item>,
    workspace_dependencies: &dyn toml_edit::TableLike,
    workspace_root: &Path,
    package_by_dir: &BTreeMap<PathBuf, String>,
    internal_dependencies: &mut BTreeSet<String>,
) {
    let Some(dependencies) = dependencies.and_then(Item::as_table_like) else {
        return;
    };
    for (name, dependency) in dependencies.iter() {
        if dependency.get("workspace").and_then(Item::as_bool) != Some(true) {
            continue;
        }
        let Some(path) = workspace_dependencies
            .get(name)
            .and_then(|dependency| dependency.get("path"))
            .and_then(Item::as_str)
        else {
            continue;
        };
        let dependency_dir = workspace_root.join(path);
        if let Ok(dependency_dir) = dependency_dir.canonicalize()
            && let Some(package) = package_by_dir.get(&dependency_dir)
        {
            internal_dependencies.insert(package.clone());
        }
    }
}

fn collect_inherited_target_internal_path_dependency_packages(
    document: &DocumentMut,
    workspace_dependencies: &dyn toml_edit::TableLike,
    workspace_root: &Path,
    package_by_dir: &BTreeMap<PathBuf, String>,
    internal_dependencies: &mut BTreeSet<String>,
) {
    let Some(targets) = document.get("target").and_then(Item::as_table_like) else {
        return;
    };
    for (_target, target) in targets.iter() {
        collect_inherited_internal_path_dependency_section_packages(
            target.get("dependencies"),
            workspace_dependencies,
            workspace_root,
            package_by_dir,
            internal_dependencies,
        );
        collect_inherited_internal_path_dependency_section_packages(
            target.get("build-dependencies"),
            workspace_dependencies,
            workspace_root,
            package_by_dir,
            internal_dependencies,
        );
        collect_inherited_internal_path_dependency_section_packages(
            target.get("dev-dependencies"),
            workspace_dependencies,
            workspace_root,
            package_by_dir,
            internal_dependencies,
        );
    }
}
