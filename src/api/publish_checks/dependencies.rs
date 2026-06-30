use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item};

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
    collect_publish_dependency_issues(document.get("dependencies"), &mut issues);
    collect_publish_dependency_issues(document.get("build-dependencies"), &mut issues);
    collect_publish_dependency_issues(document.get("dev-dependencies"), &mut issues);
    collect_target_publish_dependency_issues(document, &mut issues);
    collect_publish_dependency_issues(
        document
            .get("workspace")
            .and_then(|workspace| workspace.get("dependencies")),
        &mut issues,
    );
    collect_inherited_publish_dependency_issues(document, workspace_document, &mut issues);
    issues
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

fn collect_target_publish_dependency_issues(document: &DocumentMut, issues: &mut Vec<String>) {
    let Some(targets) = document.get("target").and_then(Item::as_table_like) else {
        return;
    };
    for (_target, target) in targets.iter() {
        collect_publish_dependency_issues(target.get("dependencies"), issues);
        collect_publish_dependency_issues(target.get("build-dependencies"), issues);
        collect_publish_dependency_issues(target.get("dev-dependencies"), issues);
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
    collect_inherited_target_publish_dependency_issues(document, workspace_dependencies, issues);
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

fn collect_inherited_target_publish_dependency_issues(
    document: &DocumentMut,
    workspace_dependencies: &dyn toml_edit::TableLike,
    issues: &mut Vec<String>,
) {
    let Some(targets) = document.get("target").and_then(Item::as_table_like) else {
        return;
    };
    for (_target, target) in targets.iter() {
        collect_inherited_publish_dependency_section_issues(
            target.get("dependencies"),
            workspace_dependencies,
            issues,
        );
        collect_inherited_publish_dependency_section_issues(
            target.get("build-dependencies"),
            workspace_dependencies,
            issues,
        );
        collect_inherited_publish_dependency_section_issues(
            target.get("dev-dependencies"),
            workspace_dependencies,
            issues,
        );
    }
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
