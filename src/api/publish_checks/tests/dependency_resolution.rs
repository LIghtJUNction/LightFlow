use super::super::*;
use super::support::TestDir;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use toml_edit::DocumentMut;

#[test]
fn internal_path_dependency_packages_resolves_known_path_dependencies() {
    let root = TestDir::new("lightflow-internal-path-dependencies");
    let workflow_dir = root.path().join("workflow");
    let dep_dir = root.path().join("dep");
    let build_dep_dir = root.path().join("build-dep");
    let dev_dep_dir = root.path().join("dev-dep");
    fs::create_dir_all(&workflow_dir).unwrap();
    fs::create_dir_all(&dep_dir).unwrap();
    fs::create_dir_all(&build_dep_dir).unwrap();
    fs::create_dir_all(&dev_dep_dir).unwrap();
    let mut package_by_dir = BTreeMap::new();
    package_by_dir.insert(dep_dir.canonicalize().unwrap(), "dep-package".to_owned());
    package_by_dir.insert(
        build_dep_dir.canonicalize().unwrap(),
        "build-dep-package".to_owned(),
    );
    package_by_dir.insert(
        dev_dep_dir.canonicalize().unwrap(),
        "dev-dep-package".to_owned(),
    );
    let document = r#"
[package]
name = "demo"
version = "0.1.0"

[dependencies]
dep = { path = "../dep" }
external = { path = "../external" }

[build-dependencies]
build-dep = { path = "../build-dep" }

[dev-dependencies]
dev-dep = { path = "../dev-dep" }
"#
    .parse::<DocumentMut>()
    .expect("manifest");

    assert_eq!(
        internal_path_dependency_packages(
            &document,
            None,
            &workflow_dir,
            root.path(),
            &package_by_dir
        ),
        BTreeSet::from([
            "build-dep-package".to_owned(),
            "dep-package".to_owned(),
            "dev-dep-package".to_owned(),
        ])
    );
}

#[test]
fn internal_path_dependency_packages_resolves_workspace_path_dependencies() {
    let root = TestDir::new("lightflow-workspace-internal-path-dependencies");
    let workflow_dir = root.path().join("workflows").join("app");
    let dep_dir = root.path().join("workflows").join("dep");
    fs::create_dir_all(&workflow_dir).unwrap();
    fs::create_dir_all(&dep_dir).unwrap();
    let mut package_by_dir = BTreeMap::new();
    package_by_dir.insert(
        dep_dir.canonicalize().unwrap(),
        "dep-workflow-package".to_owned(),
    );
    let document = r#"
[package]
name = "app"
version = "0.1.0"

[dependencies]
dep-workflow = { workspace = true }
"#
    .parse::<DocumentMut>()
    .expect("manifest");
    let workspace_document = r#"
[workspace]

[workspace.dependencies]
dep-workflow = { path = "workflows/dep", version = "0.1.0" }
"#
    .parse::<DocumentMut>()
    .expect("workspace manifest");

    assert_eq!(
        internal_path_dependency_packages(
            &document,
            Some(&workspace_document),
            &workflow_dir,
            root.path(),
            &package_by_dir,
        ),
        BTreeSet::from(["dep-workflow-package".to_owned()])
    );
}

#[test]
fn internal_path_dependency_packages_resolves_target_specific_path_dependencies() {
    let root = TestDir::new("lightflow-target-internal-path-dependencies");
    let workflow_dir = root.path().join("workflow");
    let unix_dep_dir = root.path().join("unix-dep");
    let unix_build_dep_dir = root.path().join("unix-build-dep");
    let workspace_dep_dir = root.path().join("workspace-dep");
    fs::create_dir_all(&workflow_dir).unwrap();
    fs::create_dir_all(&unix_dep_dir).unwrap();
    fs::create_dir_all(&unix_build_dep_dir).unwrap();
    fs::create_dir_all(&workspace_dep_dir).unwrap();
    let mut package_by_dir = BTreeMap::new();
    package_by_dir.insert(
        unix_dep_dir.canonicalize().unwrap(),
        "unix-dep-package".to_owned(),
    );
    package_by_dir.insert(
        unix_build_dep_dir.canonicalize().unwrap(),
        "unix-build-dep-package".to_owned(),
    );
    package_by_dir.insert(
        workspace_dep_dir.canonicalize().unwrap(),
        "workspace-dep-package".to_owned(),
    );
    let document = r#"
[package]
name = "demo"
version = "0.1.0"

[target.'cfg(unix)'.dependencies]
unix-dep = { path = "../unix-dep" }
workspace-dep = { workspace = true }

[target.'cfg(unix)'.build-dependencies]
unix-build-dep = { path = "../unix-build-dep" }

[target.'cfg(unix)'.dev-dependencies]
workspace-dev-dep = { workspace = true }
"#
    .parse::<DocumentMut>()
    .expect("manifest");
    let workspace_document = r#"
[workspace]

[workspace.dependencies]
workspace-dep = { path = "workspace-dep", version = "0.1.0" }
workspace-dev-dep = { path = "workspace-dep", version = "0.1.0" }
"#
    .parse::<DocumentMut>()
    .expect("workspace manifest");

    assert_eq!(
        internal_path_dependency_packages(
            &document,
            Some(&workspace_document),
            &workflow_dir,
            root.path(),
            &package_by_dir,
        ),
        BTreeSet::from([
            "unix-dep-package".to_owned(),
            "unix-build-dep-package".to_owned(),
            "workspace-dep-package".to_owned(),
        ])
    );
}
