mod support;

use std::fs;
use std::process::Command;
use support::*;

#[test]
fn add_path_dependency_upgrades_virtual_workspace_and_writes_root_dependencies()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    fs::write(
        root.join("Cargo.toml"),
        r#"[workspace]
members = []
resolver = "2"

[workspace.dependencies]
lightflow = "0.1.3"
lf-core = { package = "lightflow", version = "0.1.3" }
foo-git = { git = "https://example.com/foo.git" }
foo-registry = "1"
"#,
    )?;

    let added = lfw(
        &root,
        [
            "add",
            "lightflow-local",
            "--path",
            "../lightflow-local/workflows/demo",
            "--editable",
        ],
    )?;
    assert_eq!(added["dependency"], "lightflow-local");
    assert_eq!(added["source"]["path"], "../lightflow-local/workflows/demo");
    assert_eq!(added["editable"], true);

    let manifest = fs::read_to_string(root.join("Cargo.toml"))?;
    assert!(manifest.contains("[package]"));
    assert!(manifest.contains("name = \"lightflow-cli-test-"));
    assert!(manifest.contains("-lightflow-host\""));
    assert!(manifest.contains("publish = false"));
    assert!(manifest.contains("path = \".lightflow/workspace.rs\""));
    assert!(manifest.contains("[dependencies]"));
    assert!(manifest.contains("[workspace]"));
    assert!(manifest.contains("members = []"));
    assert!(
        manifest.contains("lightflow-local = { path = \"../lightflow-local/workflows/demo\" }")
    );
    assert!(root.join(".lightflow/workspace.rs").is_file());
    let document = manifest.parse::<toml_edit::DocumentMut>()?;
    assert!(document["dependencies"].get("foo-git").is_some());
    assert!(document["dependencies"].get("foo-registry").is_some());
    assert!(document["dependencies"].get("lightflow-local").is_some());
    assert!(
        document["workspace"]["dependencies"]
            .get("lightflow")
            .is_some()
    );
    assert!(
        document["workspace"]["dependencies"]
            .get("lf-core")
            .is_some()
    );
    assert!(
        document["workspace"]["dependencies"]
            .get("foo-git")
            .is_some()
    );
    assert!(
        document["workspace"]["dependencies"]
            .get("foo-registry")
            .is_some()
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn upgraded_workspace_keeps_member_inherited_dependencies_valid()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(root.join("member/src"))?;
    fs::create_dir_all(root.join("foo/src"))?;
    fs::write(root.join("member/src/lib.rs"), "pub fn member() {}\n")?;
    fs::write(root.join("foo/src/lib.rs"), "pub fn foo() {}\n")?;
    fs::write(
        root.join("foo/Cargo.toml"),
        "[package]\nname = \"foo\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )?;
    fs::write(
        root.join("member/Cargo.toml"),
        r#"[package]
name = "legacy-member"
version = "0.1.0"
edition = "2024"

[dependencies]
foo = { workspace = true }
"#,
    )?;
    fs::write(
        root.join("Cargo.toml"),
        format!(
            r#"[workspace]
resolver = "3"
members = ["member"]
exclude = ["foo"]

[workspace.dependencies]
lightflow = {{ path = {:?} }}
foo = {{ path = "foo" }}
"#,
            env!("CARGO_MANIFEST_DIR")
        ),
    )?;

    lfw(&root, ["add", "bar", "--path", "foo", "--package", "foo"])?;

    let manifest = fs::read_to_string(root.join("Cargo.toml"))?;
    let document = manifest.parse::<toml_edit::DocumentMut>()?;
    assert!(document["dependencies"].get("foo").is_some());
    assert!(document["dependencies"].get("bar").is_some());
    assert!(document["workspace"]["dependencies"].get("foo").is_some());
    assert!(
        document["workspace"]["dependencies"]
            .get("lightflow")
            .is_some()
    );

    let metadata = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(&root)
        .output()?;
    assert!(
        metadata.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&metadata.stderr)
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}
