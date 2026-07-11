use super::*;

const TEMPLATE_WORKSPACE: &str = "[workspace]\nmembers = [\".lightflow/workflows/*\"]\n";
const ROOT_TEMPLATE_WORKSPACE: &str = "[workspace]\nmembers = [\"workflows/*\"]\n";
const HOST_TEMPLATE_WORKSPACE: &str = r#"[package]
name = "fixture-lightflow-host"
version = "0.0.0"
edition = "2024"
publish = false

[lib]
path = ".lightflow/workspace.rs"

[dependencies]

[workspace]
members = [".lightflow/workflows/*"]
"#;

#[test]
fn empty_template_workspace_skips_metadata() {
    let root = tempfile::tempdir().expect("tempdir");
    let manifest = root.path().join("Cargo.toml");
    fs::write(&manifest, TEMPLATE_WORKSPACE).expect("manifest");

    assert!(should_skip_empty_template_workspace_metadata(&manifest).expect("guard"));
}

#[test]
fn empty_generated_host_workspace_skips_metadata() {
    let root = tempfile::tempdir().expect("tempdir");
    let manifest = root.path().join("Cargo.toml");
    fs::write(&manifest, HOST_TEMPLATE_WORKSPACE).expect("manifest");

    assert!(should_skip_empty_template_workspace_metadata(&manifest).expect("guard"));
}

#[test]
fn generated_host_with_core_lightflow_dependency_skips_metadata() {
    let root = tempfile::tempdir().expect("tempdir");
    let manifest = root.path().join("Cargo.toml");
    fs::write(
        &manifest,
        HOST_TEMPLATE_WORKSPACE.replace(
            "[dependencies]\n",
            "[dependencies]\nlightflow = \"0.1.3\"\n",
        ),
    )
    .expect("manifest");

    assert!(should_skip_empty_template_workspace_metadata(&manifest).expect("guard"));
}

#[test]
fn generated_host_with_extra_dependency_runs_metadata() {
    let root = tempfile::tempdir().expect("tempdir");
    let manifest = root.path().join("Cargo.toml");
    fs::write(
        &manifest,
        HOST_TEMPLATE_WORKSPACE.replace("[dependencies]\n", "[dependencies]\nserde = \"1\"\n"),
    )
    .expect("manifest");

    assert!(!should_skip_empty_template_workspace_metadata(&manifest).expect("guard"));
}

#[test]
fn template_workspace_with_only_lightflow_dependency_skips_metadata() {
    let root = tempfile::tempdir().expect("tempdir");
    let manifest = root.path().join("Cargo.toml");
    fs::write(&manifest, TEMPLATE_WORKSPACE).expect("manifest");
    let workflow = root.path().join(".lightflow/workflows/example");
    fs::create_dir_all(&workflow).expect("workflow dir");
    fs::write(
        workflow.join("Cargo.toml"),
        "[package]\nname = \"example\"\n\n[dependencies]\nlightflow = \"0.1.3\"\n",
    )
    .expect("workflow manifest");

    assert!(should_skip_empty_template_workspace_metadata(&manifest).expect("guard"));
}

#[test]
fn empty_root_template_workspace_skips_metadata() {
    let root = tempfile::tempdir().expect("tempdir");
    let manifest = root.path().join("Cargo.toml");
    fs::write(&manifest, ROOT_TEMPLATE_WORKSPACE).expect("manifest");

    assert!(should_skip_empty_template_workspace_metadata(&manifest).expect("guard"));
}

#[test]
fn root_template_workspace_with_dependency_free_workflow_skips_metadata() {
    let root = tempfile::tempdir().expect("tempdir");
    let manifest = root.path().join("Cargo.toml");
    fs::write(&manifest, ROOT_TEMPLATE_WORKSPACE).expect("manifest");
    let workflow = root.path().join("workflows/example");
    fs::create_dir_all(&workflow).expect("workflow dir");
    fs::write(
        workflow.join("Cargo.toml"),
        "[package]\nname = \"example\"\n",
    )
    .expect("workflow manifest");

    assert!(should_skip_empty_template_workspace_metadata(&manifest).expect("guard"));
}

#[test]
fn template_workspace_with_extra_dependency_runs_metadata() {
    let root = tempfile::tempdir().expect("tempdir");
    let manifest = root.path().join("Cargo.toml");
    fs::write(&manifest, TEMPLATE_WORKSPACE).expect("manifest");
    let workflow = root.path().join(".lightflow/workflows/example");
    fs::create_dir_all(&workflow).expect("workflow dir");
    fs::write(
        workflow.join("Cargo.toml"),
        "[package]\nname = \"example\"\n\n[dependencies]\nlightflow = \"0.1.3\"\nserde = \"1\"\n",
    )
    .expect("workflow manifest");

    assert!(!should_skip_empty_template_workspace_metadata(&manifest).expect("guard"));
}

#[test]
fn template_workspace_with_renamed_lightflow_dependency_skips_metadata() {
    let root = tempfile::tempdir().expect("tempdir");
    let manifest = root.path().join("Cargo.toml");
    fs::write(&manifest, TEMPLATE_WORKSPACE).expect("manifest");
    let workflow = root.path().join(".lightflow/workflows/example");
    fs::create_dir_all(&workflow).expect("workflow dir");
    fs::write(
        workflow.join("Cargo.toml"),
        "[package]\nname = \"example\"\n\n[dependencies]\nlf = { package = \"lightflow\", version = \"0.1.3\" }\n",
    )
    .expect("workflow manifest");

    assert!(should_skip_empty_template_workspace_metadata(&manifest).expect("guard"));
}

#[test]
fn ordinary_package_manifest_runs_metadata() {
    let root = tempfile::tempdir().expect("tempdir");
    let manifest = root.path().join("Cargo.toml");
    fs::write(
        &manifest,
        "[package]\nname = \"ordinary\"\nversion = \"0.1.0\"\n",
    )
    .expect("manifest");

    assert!(!should_skip_empty_template_workspace_metadata(&manifest).expect("guard"));
}
