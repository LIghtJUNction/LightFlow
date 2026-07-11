use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

pub(super) fn temp_root() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!(
        "lightflow-loop-check-test-{}-{nanos}",
        std::process::id()
    ))
}

pub(super) fn std_project_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("projects/lightflow-std")
}

pub(super) fn write_test_workflow_crate(
    root: &Path,
    workflow_id: &str,
    source: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let crate_dir = test_workflow_crate_dir(root, workflow_id);
    fs::create_dir_all(crate_dir.join("src"))?;
    fs::write(
        crate_dir.join("Cargo.toml"),
        format!(
            r#"[package]
name = "{}"
version = "0.1.0"
edition = "2024"
publish = false

[dependencies]
lightflow = {{ workspace = true }}
"#,
            workflow_id.replace('.', "-")
        ),
    )?;
    fs::write(crate_dir.join("src/lib.rs"), source)?;
    Ok(())
}

pub(super) fn test_workflow_crate_dir(root: &Path, workflow_id: &str) -> PathBuf {
    let crate_dir_name = workflow_id
        .strip_prefix("lightflow.")
        .unwrap_or(workflow_id)
        .replace('.', "_");
    root.join("workflows").join(crate_dir_name)
}

pub(super) fn test_workflow_manifest(root: &Path, workflow_id: &str) -> PathBuf {
    test_workflow_crate_dir(root, workflow_id).join("Cargo.toml")
}

pub(super) fn git_ok<const N: usize>(
    root: &Path,
    args: [&str; N],
) -> Result<(), Box<dyn std::error::Error>> {
    let mut command_args = Vec::with_capacity(N + 6);
    if args.contains(&"commit") {
        command_args.extend([
            "-c",
            "user.name=LightFlow Test",
            "-c",
            "user.email=lightflow-test@example.invalid",
            "-c",
            "commit.gpgsign=false",
        ]);
    }
    command_args.extend_from_slice(&args);

    let output = Command::new("git")
        .args(command_args)
        .current_dir(root)
        .output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git failed with status {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into())
    }
}

pub(super) fn write_test_extension_crate(
    root: &Path,
    _workflow_id: &str,
    source: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let crate_dir = root.join("extensions/lightflow-external-child");
    fs::create_dir_all(crate_dir.join("src"))?;
    fs::write(
        crate_dir.join("Cargo.toml"),
        r#"[package]
name = "lightflow-external-child"
version = "0.1.0"
edition = "2024"
description = "External child fixture."
license = "MIT"
"#,
    )?;
    fs::write(crate_dir.join("src/lib.rs"), source)?;
    assert!(source.contains("workflow!()"));
    Ok(())
}
