mod support;

use std::fs;
use std::path::Path;
use support::{lfw, lfw_command, unique_temp_root};

#[test]
fn migrate_flattens_all_known_collections_and_updates_manifests()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    write_legacy_crate(&root.join(".lightflow/workflows/alpha/one"), "one")?;
    write_legacy_crate(&root.join("workflows/beta/two"), "two")?;
    write_legacy_crate(&root.join("lightflow/workflows/gamma/three"), "three")?;
    fs::write(
        root.join("Cargo.toml"),
        r#"# preserve this comment
[workspace]
members = [
  ".lightflow/workflows/*/*",
  "workflows/*/*",
  "lightflow/workflows/*/*",
  "custom/workflows/*/*",
  "workflows/*",
]

[workspace.metadata.lightflow]
example = "workflows/*/*"
"#,
    )?;
    fs::create_dir_all(root.join(".lightflow"))?;
    fs::write(
        root.join(".lightflow/Cargo.toml"),
        "[workspace]\nmembers = [\"workflows/*/*\"]\n",
    )?;

    let result = lfw(&root, ["migrate"])?;

    assert_eq!(result["migrated"], 3);
    assert!(root.join(".lightflow/workflows/one/src/lib.rs").is_file());
    assert!(root.join("workflows/two/src/lib.rs").is_file());
    assert!(root.join("lightflow/workflows/three/src/lib.rs").is_file());
    assert!(!root.join(".lightflow/workflows/alpha").exists());
    assert!(!root.join("workflows/beta").exists());
    assert!(!root.join("lightflow/workflows/gamma").exists());
    assert_eq!(result["updated_manifests"].as_array().unwrap().len(), 2);
    let root_manifest = fs::read_to_string(root.join("Cargo.toml"))?;
    assert!(root_manifest.contains("# preserve this comment"));
    assert!(root_manifest.contains("\".lightflow/workflows/*\""));
    assert!(root_manifest.contains("\"workflows/*\""));
    assert!(root_manifest.contains("\"lightflow/workflows/*\""));
    assert!(root_manifest.contains("\"custom/workflows/*/*\""));
    assert!(root_manifest.contains("example = \"workflows/*/*\""));
    assert!(!fs::read_to_string(root.join(".lightflow/Cargo.toml"))?.contains("*/*"));

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn migrate_rejects_invalid_manifest_without_moving_crates() -> Result<(), Box<dyn std::error::Error>>
{
    let root = unique_temp_root();
    let source = root.join("workflows/alpha/one");
    write_legacy_crate(&source, "one")?;
    fs::write(root.join("Cargo.toml"), "[workspace\n")?;

    let output = lfw_command(&root).arg("migrate").output()?;

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("invalid Cargo manifest"));
    assert!(source.is_dir());
    assert!(!root.join("workflows/one").exists());
    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn migrate_reports_collection_io_errors() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    fs::write(root.join("workflows"), "not a directory\n")?;

    let output = lfw_command(&root).arg("migrate").output()?;

    assert!(!output.status.success());
    assert_ne!(output.status.code(), Some(0));
    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[cfg(unix)]
#[test]
fn migrate_does_not_follow_symlink_categories() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    let external = unique_temp_root();
    let external_crate = external.join("category/one");
    write_legacy_crate(&external_crate, "one")?;
    fs::create_dir_all(root.join("workflows"))?;
    std::os::unix::fs::symlink(external.join("category"), root.join("workflows/link"))?;

    let result = lfw(&root, ["migrate"])?;

    assert_eq!(result["migrated"], 0);
    assert!(external_crate.join("src/lib.rs").is_file());
    assert!(!root.join("workflows/one").exists());
    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(external);
    Ok(())
}

#[test]
fn migrate_rejects_duplicate_targets_without_partial_moves()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    let first = root.join("workflows/alpha/shared");
    let second = root.join("workflows/beta/shared");
    write_legacy_crate(&first, "first")?;
    write_legacy_crate(&second, "second")?;

    let output = lfw_command(&root).arg("migrate").output()?;

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("maps both"));
    assert!(first.is_dir());
    assert!(second.is_dir());
    assert!(!root.join("workflows/shared").exists());

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn migrate_rejects_existing_flat_target_without_moving_any_crates()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    let conflicting_source = root.join("workflows/alpha/one");
    let untouched_source = root.join("workflows/beta/two");
    write_legacy_crate(&conflicting_source, "legacy-one")?;
    write_legacy_crate(&untouched_source, "two")?;
    write_legacy_crate(&root.join("workflows/one"), "flat-one")?;

    let output = lfw_command(&root).arg("migrate").output()?;

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("target already exists"));
    assert!(conflicting_source.is_dir());
    assert!(untouched_source.is_dir());
    assert!(root.join("workflows/one/src/lib.rs").is_file());
    assert!(!root.join("workflows/two").exists());

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn migrate_is_idempotent() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    write_legacy_crate(&root.join("workflows/alpha/one"), "one")?;

    assert_eq!(lfw(&root, ["migrate"])?["migrated"], 1);
    let second = lfw(&root, ["migrate"])?;
    assert_eq!(second["migrated"], 0);
    assert_eq!(second["updated_manifests"], serde_json::json!([]));

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn migrate_retains_non_crate_category_content() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    let category = root.join("workflows/alpha");
    write_legacy_crate(&category.join("one"), "one")?;
    fs::write(category.join("notes.md"), "keep\n")?;

    let result = lfw(&root, ["migrate"])?;

    assert_eq!(result["migrated"], 1);
    assert!(root.join("workflows/one").is_dir());
    assert_eq!(fs::read_to_string(category.join("notes.md"))?, "keep\n");
    assert_eq!(result["retained"].as_array().unwrap().len(), 1);
    let listed = lfw(&root, ["list"])?;
    assert_eq!(listed["workflows"].as_array().unwrap().len(), 1);
    assert_eq!(listed["workflows"][0]["id"], "lightflow.one");

    let _ = fs::remove_dir_all(root);
    Ok(())
}

fn write_legacy_crate(root: &Path, package: &str) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(root.join("src"))?;
    fs::write(
        root.join("Cargo.toml"),
        format!(
            "[package]\nname = {:?}\nversion = \"0.1.0\"\n",
            format!("lightflow-{package}")
        ),
    )?;
    fs::write(
        root.join("src/lib.rs"),
        format!(
            "use lightflow::preload::*;\n\npub fn define() -> WorkflowSpec {{\n    workflow!().name({package:?}).build()\n}}\n"
        ),
    )?;
    Ok(())
}
