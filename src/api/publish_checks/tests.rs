use super::*;
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use toml_edit::DocumentMut;

#[test]
fn cargo_publish_command_matches_cli_argument_order() {
    assert_eq!(
        cargo_publish_command(Path::new("./workflows/std/demo/Cargo.toml"), true, true),
        vec![
            "cargo",
            "publish",
            "--manifest-path",
            "workflows/std/demo/Cargo.toml",
            "--allow-dirty",
            "--dry-run",
        ]
    );
}

#[test]
fn cargo_publish_command_omits_optional_flags_when_disabled() {
    assert_eq!(
        cargo_publish_command(Path::new("workflows/std/demo/Cargo.toml"), false, false),
        vec![
            "cargo",
            "publish",
            "--manifest-path",
            "workflows/std/demo/Cargo.toml",
        ]
    );
}

#[test]
fn publish_issues_reports_package_and_dependency_blockers() {
    let document = r#"
[package]
name = "demo"
version = "not-semver"
publish = false

[dependencies]
local-only = { path = "../local-only" }
git-only = { git = "https://example.invalid/repo.git" }
"#
    .parse::<DocumentMut>()
    .expect("manifest");

    assert_eq!(
        publish_issues(&document, None),
        vec![
            "package.publish is false",
            "package.version not-semver is not semantic version",
            "package.description is missing",
            "package.license or package.license-file is missing",
            "dependency local-only uses path without a crates.io version",
            "dependency git-only uses git, which cannot be published to crates.io",
        ]
    );
}

#[test]
fn publish_issues_checks_inherited_workspace_dependencies() {
    let document = r#"
[package]
name = "demo"
version = "0.1.0"
description = "Demo"
license = "MIT"

[dependencies]
local-only = { workspace = true }
"#
    .parse::<DocumentMut>()
    .expect("manifest");
    let workspace = r#"
[workspace]

[workspace.dependencies]
local-only = { path = "../local-only" }
"#
    .parse::<DocumentMut>()
    .expect("workspace manifest");

    assert_eq!(
        publish_issues(&document, Some(&workspace)),
        vec!["dependency local-only uses path without a crates.io version"]
    );
    let versioned_workspace = r#"
[workspace]

[workspace.dependencies]
local-only = { version = "0.1.0", path = "../local-only" }
"#
    .parse::<DocumentMut>()
    .expect("workspace manifest");

    assert!(publish_issues(&document, Some(&versioned_workspace)).is_empty());
}

#[test]
fn publish_issues_checks_target_specific_dependencies() {
    let document = r#"
[package]
name = "demo"
version = "0.1.0"
description = "Demo"
license = "MIT"

[target.'cfg(unix)'.dependencies]
unix-local = { path = "../unix-local" }
unix-git = { git = "https://example.invalid/unix.git" }
workspace-local = { workspace = true }

[target.'cfg(unix)'.build-dependencies]
unix-build-local = { path = "../unix-build-local" }

[target.'cfg(unix)'.dev-dependencies]
unix-dev-git = { git = "https://example.invalid/unix-dev.git" }
"#
    .parse::<DocumentMut>()
    .expect("manifest");
    let workspace = r#"
[workspace]

[workspace.dependencies]
workspace-local = { path = "../workspace-local" }
"#
    .parse::<DocumentMut>()
    .expect("workspace manifest");

    assert_eq!(
        publish_issues(&document, Some(&workspace)),
        vec![
            "dependency unix-local uses path without a crates.io version",
            "dependency unix-git uses git, which cannot be published to crates.io",
            "dependency unix-build-local uses path without a crates.io version",
            "dependency unix-dev-git uses git, which cannot be published to crates.io",
            "dependency workspace-local uses path without a crates.io version",
        ]
    );
}

#[test]
fn package_field_value_reads_string_package_fields() {
    let document = r#"
[package]
name = "demo"
version = "0.1.0"
"#
    .parse::<DocumentMut>()
    .expect("manifest");

    assert_eq!(
        package_field_value(&document, "name").as_deref(),
        Some("demo")
    );
    assert_eq!(
        package_field_value(&document, "version").as_deref(),
        Some("0.1.0")
    );
    assert_eq!(package_field_value(&document, "description"), None);
}

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

#[test]
fn parse_cargo_manifest_reports_invalid_toml() {
    let error = parse_cargo_manifest("[package").expect_err("invalid manifest");
    assert!(!error.to_string().is_empty());
}

#[test]
fn read_cargo_manifest_reports_invalid_toml() {
    let root = TestDir::new("lightflow-invalid-cargo-manifest");
    fs::create_dir_all(root.path()).unwrap();
    let manifest = root.path().join("Cargo.toml");
    fs::write(&manifest, "[package").unwrap();

    let error = read_cargo_manifest(&manifest).expect_err("invalid manifest");

    assert!(matches!(error, CargoManifestReadError::Parse(_)));
}

#[test]
fn read_cargo_manifest_reports_io_errors() {
    let root = TestDir::new("lightflow-missing-cargo-manifest");
    let manifest = root.path().join("Cargo.toml");

    let error = read_cargo_manifest(&manifest).expect_err("missing manifest");

    assert!(matches!(error, CargoManifestReadError::Io(_)));
}

#[test]
fn read_workspace_cargo_manifest_reads_optional_root_manifest() {
    let root = TestDir::new("lightflow-workspace-manifest");

    assert!(
        read_workspace_cargo_manifest(root.path())
            .unwrap()
            .is_none()
    );

    fs::create_dir_all(root.path()).unwrap();
    fs::write(
        root.path().join("Cargo.toml"),
        "[workspace]\nmembers = []\n",
    )
    .unwrap();

    assert!(
        read_workspace_cargo_manifest(root.path())
            .unwrap()
            .is_some()
    );
}

struct TestDir {
    path: std::path::PathBuf,
}

impl TestDir {
    fn new(name: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after unix epoch")
            .as_nanos();
        Self {
            path: std::env::temp_dir().join(format!(
                "lightflow-publish-checks-{name}-{}-{nanos}",
                std::process::id()
            )),
        }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TestDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
