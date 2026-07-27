use super::super::*;
use super::support::TestDir;
use std::fs;
use std::path::Path;
use toml_edit::DocumentMut;

#[test]
fn cargo_publish_command_matches_cli_argument_order() {
    assert_eq!(
        cargo_publish_command(Path::new("./workflows/demo/Cargo.toml"), true, true),
        vec![
            "cargo",
            "publish",
            "--manifest-path",
            "workflows/demo/Cargo.toml",
            "--allow-dirty",
            "--dry-run",
        ]
    );
}

#[test]
fn cargo_publish_command_omits_optional_flags_when_disabled() {
    assert_eq!(
        cargo_publish_command(Path::new("workflows/demo/Cargo.toml"), false, false),
        vec![
            "cargo",
            "publish",
            "--manifest-path",
            "workflows/demo/Cargo.toml",
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
fn path_dependency_release_issues_require_publishable_catalog_member() {
    let root = TestDir::new("release-path-dependency");
    let runtime = root.path().join("runtime");
    let workflow = root.path().join("workflows/demo");
    fs::create_dir_all(&runtime).unwrap();
    fs::create_dir_all(&workflow).unwrap();
    let workspace_source = r#"
[workspace]
members = ["runtime", "workflows/*"]

[workspace.dependencies]
support = { path = "runtime", version = "0.1.0" }
"#;
    fs::write(root.path().join("Cargo.toml"), workspace_source).unwrap();
    fs::write(
        runtime.join("Cargo.toml"),
        r#"[package]
name = "support"
version = "0.1.0"
publish = false
"#,
    )
    .unwrap();
    let manifest = workflow.join("Cargo.toml");
    fs::write(
        &manifest,
        r#"[package]
name = "demo"
version = "0.1.0"

[dependencies]
support = { workspace = true }
"#,
    )
    .unwrap();
    let document = read_cargo_manifest(&manifest).unwrap();
    let workspace = read_cargo_manifest(&root.path().join("Cargo.toml")).unwrap();
    assert_eq!(
        path_dependency_release_issues(&manifest, &document, Some(&workspace), root.path()),
        vec!["dependency support path target has package.publish = false"]
    );

    fs::write(
        runtime.join("Cargo.toml"),
        r#"[package]
name = "support"
version = "0.1.0"
"#,
    )
    .unwrap();
    assert!(
        path_dependency_release_issues(&manifest, &document, Some(&workspace), root.path())
            .is_empty()
    );

    let excluded = workspace_source.replace(
        "members = [\"runtime\", \"workflows/*\"]",
        "members = [\"runtime\", \"workflows/*\"]\nexclude = [\"runtime\"]",
    );
    fs::write(root.path().join("Cargo.toml"), &excluded).unwrap();
    let excluded_workspace = excluded.parse::<DocumentMut>().unwrap();
    assert_eq!(
        path_dependency_release_issues(
            &manifest,
            &document,
            Some(&excluded_workspace),
            root.path()
        ),
        vec!["dependency support path target is excluded from the workspace release catalog"]
    );

    let external = root.path().join("../external");
    fs::create_dir_all(&external).unwrap();
    fs::write(
        external.join("Cargo.toml"),
        "[package]\nname = \"external\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    let external_document = r#"
[package]
name = "demo"
version = "0.1.0"

[dependencies]
external = { path = "../../../external", version = "0.1.0" }
"#
    .parse::<DocumentMut>()
    .unwrap();
    assert!(
        path_dependency_release_issues(
            &manifest,
            &external_document,
            Some(&excluded_workspace),
            root.path()
        )
        .is_empty()
    );
}

#[test]
fn nested_workspace_member_glob_includes_path_dependency() {
    let root = TestDir::new("nested-member-glob");
    let support = root.path().join("crates/support/runtime");
    let workflow = root.path().join("crates/workflows/demo");
    fs::create_dir_all(&support).unwrap();
    fs::create_dir_all(&workflow).unwrap();
    fs::write(
        root.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\"crates/*/*\"]\n",
    )
    .unwrap();
    fs::write(
        support.join("Cargo.toml"),
        "[package]\nname = \"support\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    let manifest = workflow.join("Cargo.toml");
    fs::write(
        &manifest,
        r#"[package]
name = "demo"
version = "0.1.0"

[dependencies]
support = { path = "../../support/runtime", version = "0.1.0" }
"#,
    )
    .unwrap();
    let document = read_cargo_manifest(&manifest).unwrap();
    let workspace = read_cargo_manifest(&root.path().join("Cargo.toml")).unwrap();

    assert!(
        path_dependency_release_issues(&manifest, &document, Some(&workspace), root.path())
            .is_empty()
    );
}

#[test]
fn path_dependency_members_use_the_callers_workspace_root() {
    let root = TestDir::new("nested-workspace-members");
    let nested = root.path().join(".lightflow");
    let support = nested.join("workflows/support");
    let workflow = nested.join("workflows/demo");
    fs::create_dir_all(&support).unwrap();
    fs::create_dir_all(&workflow).unwrap();
    fs::write(
        root.path().join("Cargo.toml"),
        "[workspace]\nmembers = [\".lightflow/workflows/*\"]\n",
    )
    .unwrap();
    fs::write(
        nested.join("Cargo.toml"),
        "[workspace]\nmembers = [\"workflows/*\"]\n",
    )
    .unwrap();
    fs::write(
        support.join("Cargo.toml"),
        "[package]\nname = \"support\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    let manifest = workflow.join("Cargo.toml");
    fs::write(
        &manifest,
        r#"[package]
name = "demo"
version = "0.1.0"

[dependencies]
support = { path = "../support", version = "0.1.0" }
"#,
    )
    .unwrap();
    let document = read_cargo_manifest(&manifest).unwrap();
    let workspace = read_cargo_manifest(&root.path().join("Cargo.toml")).unwrap();

    assert!(
        path_dependency_release_issues(&manifest, &document, Some(&workspace), root.path())
            .is_empty()
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
