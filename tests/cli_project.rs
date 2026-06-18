mod support;

use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;
use support::*;

#[test]
fn lfw_init_and_add_create_rust_workflow_files() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;

    let init = lfw(&root, ["init"])?;
    assert!(
        init["created"]
            .as_array()
            .unwrap()
            .iter()
            .any(|path| path.as_str().unwrap().ends_with("Cargo.toml"))
    );
    assert!(init["created"].as_array().unwrap().iter().any(|path| {
        path.as_str()
            .unwrap()
            .ends_with("examples/example/src/lib.rs")
    }));
    assert!(init["created"].as_array().unwrap().iter().any(|path| {
        path.as_str()
            .unwrap()
            .ends_with("examples/example/.agent/skills/lightflow-example/SKILL.md")
    }));

    let missing_category = Command::new(env!("CARGO_BIN_EXE_lfw"))
        .args(["new", "missing_category"])
        .current_dir(&root)
        .output()?;
    assert!(!missing_category.status.success());
    assert!(
        String::from_utf8_lossy(&missing_category.stderr)
            .contains("lfw new requires --category <name>")
    );

    let added = lfw(
        &root,
        [
            "new",
            "extra",
            "--category",
            "examples",
            "--name",
            "Extra Workflow",
        ],
    )?;
    assert_eq!(added["workflow_id"], "lightflow.extra");
    assert_eq!(added["category"], "examples");
    let manifest = fs::read_to_string(root.join("workflows/examples/extra/Cargo.toml"))?;
    assert!(manifest.contains("name = \"lightflow-extra\""));
    assert!(!manifest.contains("publish = false"));
    let workspace = fs::read_to_string(root.join("Cargo.toml"))?;
    assert!(workspace.contains(&format!("lightflow = {:?}", env!("CARGO_PKG_VERSION"))));
    let gitignore = fs::read_to_string(root.join(".gitignore"))?;
    assert!(gitignore.contains("/target/"));
    assert!(gitignore.contains("/lfw.lock"));
    let rc = fs::read_to_string(root.join(".test-xdg/config/lightflow/.lfwrc"))?;
    assert!(rc.contains("export LFW_PATH="));
    assert!(rc.contains(".test-xdg/data/lightflow/workflows"));
    let lfw_path_manifest = root.join(".test-xdg/data/lightflow/workflows/Cargo.toml");
    assert!(lfw_path_manifest.exists());
    let lfw_path_workspace = fs::read_to_string(&lfw_path_manifest)?;
    assert!(lfw_path_workspace.contains("members = [\"*/*\"]"));
    assert!(lfw_path_workspace.contains(&format!("lightflow = {:?}", env!("CARGO_PKG_VERSION"))));
    assert_eq!(
        init["config"]["workflow_workspace_manifest"],
        lfw_path_manifest.to_str().unwrap()
    );
    assert_eq!(init["config"]["workflow_workspace_created"], true);
    let zshrc = fs::read_to_string(root.join(".zshrc"))?;
    assert!(zshrc.contains("source "));
    assert!(zshrc.contains(".test-xdg/config/lightflow/.lfwrc"));
    assert_eq!(init["config"]["shell"], "zsh");
    assert_eq!(init["config"]["source_installed"], true);

    let second_init = lfw(&root, ["init"])?;
    assert_eq!(second_init["created"], serde_json::json!([]));
    assert_eq!(second_init["config"]["rc_created"], false);
    assert_eq!(second_init["config"]["source_installed"], false);
    assert_eq!(second_init["config"]["workflow_workspace_created"], false);
    let path = root.join("workflows/examples/extra/src/lib.rs");
    let source = fs::read_to_string(path)?;
    assert!(source.contains("workflow(\"lightflow.extra\")"));
    assert!(source.contains(".name(\"Extra Workflow\")"));
    let skill = fs::read_to_string(
        root.join("workflows/examples/extra/.agent/skills/lightflow-extra/SKILL.md"),
    )?;
    assert!(skill.contains("Workflow id: `lightflow.extra`"));
    assert!(!root.join("workflows/examples/extra/src/main.rs").exists());

    let workflow = lightflow(&root, ["workflows", "get", "lightflow.extra"])?;
    assert_eq!(workflow["id"], "lightflow.extra");

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_new_and_add_support_global_workflow_workspace() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    lfw(&root, ["init"])?;

    let global = lfw(
        &root,
        [
            "new",
            "-g",
            "global_tool",
            "--category",
            "tools",
            "--name",
            "Global Tool",
        ],
    )?;
    assert_eq!(global["workflow_id"], "lightflow.global_tool");
    assert_eq!(global["global"], true);
    let global_root = root.join(".test-xdg/data/lightflow/workflows");
    assert!(global_root.join("tools/global_tool/src/lib.rs").exists());
    assert!(!root.join("workflows/tools/global_tool/src/lib.rs").exists());

    let listed = lfw(&root, ["list"])?;
    assert!(
        listed["workflows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|workflow| workflow["id"] == "lightflow.global_tool")
    );

    let project_manifest = fs::read_to_string(root.join("Cargo.toml"))?;
    assert!(!project_manifest.contains("lightflow-std"));
    let added = lfw(
        &root,
        [
            "add",
            "-g",
            "lightflow-std",
            "--path",
            "vendor/lightflow-std",
        ],
    )?;
    assert_eq!(added["global"], true);
    let global_manifest = fs::read_to_string(global_root.join("Cargo.toml"))?;
    assert!(global_manifest.contains("members = [\"*/*\"]"));
    assert!(global_manifest.contains("lightflow-std"));
    let project_manifest = fs::read_to_string(root.join("Cargo.toml"))?;
    assert!(!project_manifest.contains("lightflow-std"));

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_init_plugin_creates_standard_cargo_plugin_crate() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;

    let init = lfw(&root, ["init", "--plugin"])?;
    assert_eq!(init["kind"], "plugin");
    assert!(root.join(".gitignore").exists());
    assert!(root.join("Cargo.toml").exists());
    assert!(root.join("src/lib.rs").exists());
    let plugin_skill_root = root.join(".agent/skills");
    assert!(
        fs::read_dir(&plugin_skill_root)?
            .filter_map(Result::ok)
            .any(|entry| entry.path().join("SKILL.md").exists())
    );
    assert!(!root.join("workflows").exists());
    assert!(!root.join(".test-xdg/config/lightflow/.lfwrc").exists());

    let manifest = fs::read_to_string(root.join("Cargo.toml"))?;
    assert!(manifest.contains("name = \"lightflow-cli-test"));
    assert!(manifest.contains(&format!("lightflow = {:?}", env!("CARGO_PKG_VERSION"))));
    let source = fs::read_to_string(root.join("src/lib.rs"))?;
    assert!(source.contains("pub fn define() -> WorkflowSpec"));

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_update_and_upgrade_delegate_to_cargo() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    fs::write(
        root.join("Cargo.toml"),
        r#"[package]
name = "lfw-update-test"
version = "0.1.0"
edition = "2024"
"#,
    )?;
    fs::create_dir_all(root.join("src"))?;
    fs::write(root.join("src/lib.rs"), "")?;

    let update = lfw(&root, ["update"])?;
    assert_eq!(update["command"], serde_json::json!(["cargo", "fetch"]));
    assert_eq!(update["executed"], true);

    let upgrade = lfw(&root, ["upgrade"])?;
    assert_eq!(upgrade["command"], serde_json::json!(["cargo", "update"]));
    assert_eq!(upgrade["executed"], true);

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_uses_xdg_default_and_lfw_path_environment() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    let xdg_data_workflows = root.join(".test-xdg/data/lightflow/workflows");
    write_workflow_crate_in(
        &xdg_data_workflows,
        "lightflow.xdg_default",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.xdg_default")
        .version("0.1.0")
        .name("XDG Default")
        .input("value", "json")
        .output("value", "json")
        .build()
}
"#,
    )?;

    let default_list = lfw(&root, ["list"])?;
    assert_eq!(default_list["workflows"][0]["id"], "lightflow.xdg_default");

    let custom_workflows = root.join("custom-workflows");
    write_workflow_crate_in(
        &custom_workflows,
        "lightflow.rc",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.rc")
        .version("0.1.0")
        .name("RC Workflow")
        .input("value", "json")
        .output("value", "json")
        .build()
}
"#,
    )?;
    let rc_dir = root.join(".test-xdg/config/lightflow");
    fs::create_dir_all(&rc_dir)?;
    fs::write(
        rc_dir.join(".lfwrc"),
        format!("export LFW_PATH='{}'\n", custom_workflows.display()),
    )?;

    let still_default = lfw(&root, ["list"])?;
    assert_eq!(still_default["workflows"][0]["id"], "lightflow.xdg_default");

    let env_list = lfw_with_env(&root, ["list"], [("LFW_PATH", custom_workflows.as_path())])?;
    assert_eq!(env_list["workflows"][0]["id"], "lightflow.rc");

    write_workflow_crate(
        &root,
        "lightflow.rc",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.rc")
        .version("0.1.0")
        .name("Project Override")
        .input("value", "json")
        .output("value", "json")
        .build()
}
"#,
    )?;
    let project_wins = lfw_with_env(&root, ["list"], [("LFW_PATH", custom_workflows.as_path())])?;
    assert_eq!(project_wins["workflows"][0]["name"], "Project Override");

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_init_installs_fish_source_when_shell_is_fish() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    let output = Command::new(env!("CARGO_BIN_EXE_lfw"))
        .arg("init")
        .current_dir(&root)
        .env("HOME", &root)
        .env("SHELL", "/usr/bin/fish")
        .env("XDG_CONFIG_HOME", root.join(".test-xdg/config"))
        .env("XDG_DATA_HOME", root.join(".test-xdg/data"))
        .env_remove("LFW_PATH")
        .output()?;
    assert!(
        output.status.success(),
        "lfw init failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let init: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(init["config"]["shell"], "fish");
    assert_eq!(init["config"]["source_installed"], true);

    let rc = fs::read_to_string(root.join(".test-xdg/config/lightflow/.lfwrc"))?;
    assert!(rc.contains("set -gx LFW_PATH "));
    assert!(
        root.join(".test-xdg/data/lightflow/workflows/Cargo.toml")
            .exists()
    );
    let fish_config = fs::read_to_string(root.join(".test-xdg/config/fish/config.fish"))?;
    assert!(fish_config.contains("source "));
    assert!(fish_config.contains(".test-xdg/config/lightflow/.lfwrc"));

    let _ = fs::remove_dir_all(root);
    Ok(())
}

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
    assert_eq!(workflow_plan["publishable"], true);
    assert_eq!(workflow_plan["issues"], serde_json::json!([]));
    assert_eq!(
        workflow_plan["command"],
        serde_json::json!([
            "cargo",
            "publish",
            "--manifest-path",
            "workflows/examples/example/Cargo.toml",
            "--dry-run"
        ])
    );

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
#[test]
fn add_writes_git_workflow_dependency() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    lfw(&root, ["init"])?;

    let output = lfw(
        &root,
        [
            "add",
            "lightflow-std",
            "--git",
            "https://github.com/lightjunction/LightFlow",
            "--package",
            "lightflow-std",
        ],
    )?;
    assert_eq!(output["dependency"], "lightflow-std");
    assert_eq!(
        output["source"]["git"],
        "https://github.com/lightjunction/LightFlow"
    );
    assert_eq!(output["package"], "lightflow-std");

    let manifest = fs::read_to_string(root.join("Cargo.toml"))?;
    assert!(manifest.contains(
        "lightflow-std = { git = \"https://github.com/lightjunction/LightFlow\", package = \"lightflow-std\" }"
    ));

    let _ = fs::remove_dir_all(root);
    Ok(())
}
