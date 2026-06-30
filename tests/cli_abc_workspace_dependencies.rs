mod support;

use std::fs;
use support::*;

#[test]
fn add_path_dependency_creates_missing_workspace_dependencies_table()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    fs::write(
        root.join("Cargo.toml"),
        r#"[workspace]
members = []
resolver = "2"
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
    assert!(manifest.contains("[workspace.dependencies]"));
    assert!(
        manifest.contains("lightflow-local = { path = \"../lightflow-local/workflows/demo\" }")
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}
