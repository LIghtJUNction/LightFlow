use super::*;
use crate::workflow::workflow_with_identity;
use std::fs;
use std::process::Command;

#[test]
fn save_rejects_uninitialized_repository_without_creating_workflow_files() {
    let root = tempfile::tempdir().expect("tempdir");
    let service = ApiService::new(root.path());
    let mut workflow = workflow_with_identity("lightflow.saved_flow", "2.3.4")
        .name("Saved Flow")
        .build();
    workflow.category = Some("tests".to_owned());

    let error = service.save_workflow(workflow).expect_err("save error");

    assert!(error.message().contains("run `lfw init`"));
    assert!(!root.path().join(".lightflow/workflows").exists());
}

#[test]
fn save_rejects_non_official_workspace_without_creating_workflow_files() {
    let root = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(root.path().join("src")).expect("source dir");
    fs::write(
        root.path().join("Cargo.toml"),
        "[package]\nname = \"ordinary\"\nversion = \"0.1.0\"\nedition = \"2024\"\n",
    )
    .expect("manifest");
    fs::write(root.path().join("src/lib.rs"), "pub fn library() {}\n").expect("source");
    assert!(!root.path().join("Cargo.lock").exists());
    let service = ApiService::new(root.path());
    let mut workflow = workflow_with_identity("lightflow.saved_flow", "2.3.4")
        .name("Saved Flow")
        .build();
    workflow.category = Some("tests".to_owned());

    let error = service.save_workflow(workflow).expect_err("save error");

    assert!(error.message().contains("run `lfw init`"));
    assert!(!root.path().join("Cargo.lock").exists());
    assert!(!root.path().join(".lightflow/workflows").exists());
}

#[test]
fn save_and_reload_preserve_manifest_identity_and_version() {
    let root = tempfile::tempdir().expect("tempdir");
    fs::create_dir_all(root.path().join(".lightflow")).expect("host dir");
    fs::write(
        root.path().join("Cargo.toml"),
        format!(
            r#"[package]
name = "save-test-lightflow-host"
version = "0.0.0"
edition = "2024"
publish = false

[lib]
path = ".lightflow/workspace.rs"

[dependencies]

[workspace]
resolver = "3"
members = [".lightflow/workflows/*"]

[workspace.dependencies]
lightflow = {{ path = {:?} }}
"#,
            env!("CARGO_MANIFEST_DIR")
        ),
    )
    .expect("host manifest");
    fs::write(
        root.path().join(".lightflow/workspace.rs"),
        "//! Test workflow host.\n",
    )
    .expect("host source");
    let service = ApiService::new(root.path());
    let mut workflow = workflow_with_identity("lightflow.saved_flow", "2.3.4")
        .name("Saved Flow")
        .input("condition", "boolean")
        .input_description("condition", "Whether to render.")
        .input_required("condition", true)
        .input_default_json("condition", "false")
        .input_widget("condition", "checkbox")
        .input("strength", "number")
        .input_range("strength", 0.0, 1.0, 0.05)
        .input_enum_json("strength", "[0.25,0.5,0.75,1.0]")
        .input("source", "artifact")
        .input_artifact_kind("source", "image")
        .input_model_requirement("source", "image_model")
        .output("image", "artifact")
        .output_description("image", "Generated image.")
        .output_artifact_kind("image", "image")
        .output_model_requirement("image", "image_model")
        .model("image_model", "image-generation")
        .runtime("test_runtime", "lightflow.test")
        .build();
    workflow.category = Some("tests".to_owned());
    let expected = workflow.clone();

    service.save_workflow(workflow).expect("save workflow");
    let reloaded = service
        .get_workflow("lightflow.saved_flow")
        .expect("reload workflow");
    let workflow_manifest = root
        .path()
        .join(".lightflow/workflows/saved_flow/Cargo.toml");
    let manifest = fs::read_to_string(&workflow_manifest).expect("manifest");
    let metadata = Command::new("cargo")
        .args(["metadata", "--format-version", "1", "--no-deps"])
        .current_dir(root.path())
        .output()
        .expect("cargo metadata");
    assert!(
        metadata.status.success(),
        "cargo metadata failed: {}",
        String::from_utf8_lossy(&metadata.stderr)
    );
    let metadata: serde_json::Value =
        serde_json::from_slice(&metadata.stdout).expect("metadata JSON");
    assert!(
        metadata["workspace_members"]
            .as_array()
            .is_some_and(|members| {
                members.iter().any(|member| {
                    member
                        .as_str()
                        .is_some_and(|member| member.contains("lightflow-saved-flow"))
                })
            })
    );

    assert_eq!(reloaded.id, "lightflow.saved_flow");
    assert_eq!(reloaded.version, "2.3.4");
    assert_eq!(reloaded.category.as_deref(), Some("tests"));
    assert_eq!(reloaded, expected);
    assert!(manifest.contains("name = \"lightflow-saved-flow\""));
    assert!(manifest.contains("version = \"2.3.4\""));
    assert!(manifest.contains("lightflow = { workspace = true }"));
    let source = fs::read_to_string(
        root.path()
            .join(".lightflow/workflows/saved_flow/src/lib.rs"),
    )
    .expect("workflow source");
    assert!(source.contains("category: \"tests\","));
    assert!(source.contains("input \"condition\": \"boolean\" {"));
    assert!(source.contains("default: false,"));
    assert!(source.contains("choices: [0.25,0.5,0.75,1.0],"));
    assert!(source.contains("output \"image\": \"artifact\" {"));
    assert!(!source.contains(".input_description("));
    assert!(!source.contains(".output_artifact_kind("));
}
