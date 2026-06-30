mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use support::*;

#[test]
fn lfw_publish_plans_publishable_workflow_crates() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    lfw(&root, ["init"])?;

    let workflow_plan = lfw(&root, ["publish", "lightflow.example"])?;
    assert_eq!(workflow_plan["dry_run"], true);
    assert_eq!(workflow_plan["target"]["workflow_id"], "lightflow.example");
    assert_eq!(workflow_plan["package"], "lightflow-example");
    assert_eq!(workflow_plan["version"], "0.1.0");
    assert_eq!(workflow_plan["publishable"], false);
    assert!(
        workflow_plan["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue == "workflow.description contains unresolved TODO")
    );
    assert_eq!(
        workflow_plan["command"],
        serde_json::json!([
            "cargo",
            "publish",
            "--manifest-path",
            ".lightflow/workflows/examples/example/Cargo.toml",
            "--dry-run"
        ])
    );
    complete_generated_workflow_metadata(&root, "examples", "example")?;
    let workflow_plan = lfw(&root, ["publish", "lightflow.example"])?;
    assert_eq!(workflow_plan["publishable"], true);
    assert_eq!(workflow_plan["issues"], serde_json::json!([]));

    let git_init = Command::new("git")
        .arg("init")
        .current_dir(&root)
        .output()?;
    assert!(
        git_init.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&git_init.stderr)
    );
    let git_add = Command::new("git")
        .args(["add", "."])
        .current_dir(&root)
        .output()?;
    assert!(
        git_add.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&git_add.stderr)
    );
    let git_commit = Command::new("git")
        .args([
            "-c",
            "user.email=lightflow@example.invalid",
            "-c",
            "user.name=LightFlow Test",
            "commit",
            "-m",
            "fixture",
        ])
        .current_dir(&root)
        .output()?;
    assert!(
        git_commit.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&git_commit.stderr)
    );

    let fake_bin = root.join("fake-bin");
    fs::create_dir_all(&fake_bin)?;
    let cargo_log = root.join("cargo-publish.log");
    let cargo_path = fake_bin.join("cargo");
    fs::write(
        &cargo_path,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\n",
            cargo_log.display()
        ),
    )?;
    let mut permissions = fs::metadata(&cargo_path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&cargo_path, permissions)?;
    let path = format!(
        "{}:{}",
        fake_bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let applied = Command::new(env!("CARGO_BIN_EXE_lfw"))
        .args(["publish", "lightflow.example", "--apply"])
        .current_dir(&root)
        .env("HOME", &root)
        .env("SHELL", "/bin/zsh")
        .env("XDG_CONFIG_HOME", root.join(".test-xdg/config"))
        .env("XDG_DATA_HOME", root.join(".test-xdg/data"))
        .env("PATH", path)
        .env_remove("LFW_PATH")
        .output()?;
    assert!(
        applied.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&applied.stdout),
        String::from_utf8_lossy(&applied.stderr)
    );
    let applied_json: serde_json::Value = serde_json::from_slice(&applied.stdout)?;
    assert_eq!(applied_json["dry_run"], false);
    assert_eq!(
        applied_json["executed"].as_array().expect("executed").len(),
        2
    );
    assert_eq!(
        applied_json["preflight_commands"][0],
        serde_json::json!([
            "cargo",
            "publish",
            "--manifest-path",
            ".lightflow/workflows/examples/example/Cargo.toml",
            "--dry-run"
        ])
    );
    let cargo_lines = fs::read_to_string(&cargo_log)?
        .lines()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(cargo_lines.len(), 2);
    assert!(cargo_lines[0].contains("--dry-run"));
    assert!(!cargo_lines[1].contains("--dry-run"));

    let workspace_root_publish = Command::new(env!("CARGO_BIN_EXE_lfw"))
        .arg("publish")
        .current_dir(&root)
        .output()?;
    assert!(!workspace_root_publish.status.success());
    assert!(
        String::from_utf8_lossy(&workspace_root_publish.stderr)
            .contains("Cargo manifest is missing package.name")
    );

    let root_plan = lfw(Path::new(env!("CARGO_MANIFEST_DIR")), ["publish"])?;
    assert_eq!(root_plan["package"], "lightflow");
    assert_eq!(root_plan["publishable"], true);

    let extension = root.join("extensions/lightflow-extension");
    write_publishable_extension_crate(&extension)?;
    let extension_plan = lfw(
        &root,
        ["publish", "--crate", "extensions/lightflow-extension"],
    )?;
    assert_eq!(extension_plan["package"], "lightflow-extension");
    assert_eq!(extension_plan["publishable"], true);
    assert_eq!(
        extension_plan["command"],
        serde_json::json!([
            "cargo",
            "publish",
            "--manifest-path",
            "extensions/lightflow-extension/Cargo.toml",
            "--dry-run"
        ])
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

fn complete_generated_workflow_metadata(
    root: &Path,
    category: &str,
    name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = root
        .join(".lightflow/workflows")
        .join(category)
        .join(name)
        .join("src/lib.rs");
    let source = fs::read_to_string(&path)?
        .replace(
            "TODO: describe this workflow.",
            "Publishes a completed test workflow.",
        )
        .replace(
            "TODO: describe the input value.",
            "Input value for the test workflow.",
        )
        .replace(
            "TODO: describe the output value.",
            "Output value from the test workflow.",
        )
        .replace(
            "TODO: describe the runtime input value.",
            "Runtime input value for the test workflow.",
        )
        .replace(
            "TODO: describe the runtime output value.",
            "Runtime output value from the test workflow.",
        );
    fs::write(path, source)?;
    Ok(())
}
