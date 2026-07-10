#![allow(unused_imports)]

mod support;

use lightflow::api::{ApiService, WorkflowPublishOptions};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;
use support::*;

#[test]
fn lfw_init_plugin_creates_standard_cargo_plugin_crate() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;

    let init = lfw(&root, ["init", "--plugin"])?;
    assert_eq!(init["kind"], "plugin");
    assert!(root.join(".gitignore").exists());
    assert!(root.join("Cargo.toml").exists());
    assert!(root.join("src/lib.rs").exists());
    assert!(root.join("tests/contract.rs").exists());
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
    let skill_path = fs::read_dir(&plugin_skill_root)?
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("SKILL.md"))
        .find(|path| path.exists())
        .expect("plugin skill");
    let skill = fs::read_to_string(skill_path)?;
    assert!(skill.contains("## CLI Usage"));
    assert!(skill.contains("## API Usage"));
    assert!(skill.contains("/workflows/lightflow."));
    let contract = fs::read_to_string(root.join("tests/contract.rs"))?;
    let package_ident = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap()
        .replace('-', "_");
    assert!(contract.contains(&format!("{package_ident}::define()")));
    assert!(!contract.contains("lightflow_lightflow"));

    fs::create_dir_all(root.join(".cargo"))?;
    fs::write(
        root.join(".cargo/config.toml"),
        format!(
            "[patch.crates-io]\nlightflow = {{ path = {:?} }}\n",
            env!("CARGO_MANIFEST_DIR")
        ),
    )?;
    let test_status = Command::new("cargo")
        .arg("test")
        .current_dir(&root)
        .status()?;
    assert!(test_status.success());

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
    let xdg_data_workflows = root.join(".lightflow/workflows");
    write_workflow_crate_in(
        &xdg_data_workflows,
        "lightflow.xdg_default",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
        .name("XDG Default")
        .input("value", "json")
        .output("value", "json")
        .build()
}
"#,
    )?;
    let xdg_skill_dir =
        xdg_data_workflows.join("local/xdg_default/.agent/skills/lightflow-xdg-default");
    fs::create_dir_all(&xdg_skill_dir)?;
    fs::write(
        xdg_skill_dir.join("SKILL.md"),
        r#"---
name: XDG Default
description: Use this skill when working with the lightflow.xdg_default LightFlow workflow.
version: 0.1.0
---

# XDG Default

- Workflow id: `lightflow.xdg_default`

```bash
lfw run lightflow.xdg_default --input value=hello
```

HTTP `/workflows/lightflow.xdg_default/run` example.
"#,
    )?;

    let default_list = lfw(&root, ["list"])?;
    assert_eq!(default_list["workflows"][0]["id"], "lightflow.xdg_default");

    let legacy_default_home = lfw_with_env(
        &root,
        ["home"],
        [("LFW_PATH", xdg_data_workflows.as_path())],
    )?;
    assert_eq!(
        legacy_default_home["lfw_path"],
        root.join(".lightflow").to_str().unwrap()
    );
    let legacy_default_list = lfw_with_env(
        &root,
        ["list"],
        [("LFW_PATH", xdg_data_workflows.as_path())],
    )?;
    assert_eq!(
        legacy_default_list["workflows"][0]["id"],
        "lightflow.xdg_default"
    );

    let custom_workflows = root.join("custom-workflows");
    write_workflow_crate_in(
        &custom_workflows,
        "lightflow.rc",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
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
    let env_loop = lfw_with_env(
        &root,
        ["loop", "check", "lightflow.rc"],
        [("LFW_PATH", custom_workflows.as_path())],
    )?;
    let env_loop_checks = env_loop["checks"].as_array().expect("loop checks");
    assert!(
        env_loop_checks.iter().any(|check| {
            check["id"] == "loop.selected.publish"
                && check["status"] == "warning"
                && check["message"]
                    .as_str()
                    .unwrap()
                    .contains("package.publish is false")
        }),
        "loop checks:\n{env_loop_checks:#?}"
    );
    assert!(
        env_loop_checks
            .iter()
            .any(|check| { check["id"] == "loop.selected.exists" && check["status"] == "passed" }),
        "loop checks:\n{env_loop_checks:#?}"
    );

    write_workflow_crate(
        &root,
        "lightflow.rc",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
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
    assert!(root.join(".lightflow/Cargo.toml").exists());
    let fish_config = fs::read_to_string(root.join(".test-xdg/config/fish/config.fish"))?;
    assert!(fish_config.contains("source "));
    assert!(fish_config.contains(".test-xdg/config/lightflow/.lfwrc"));

    let _ = fs::remove_dir_all(root);
    Ok(())
}
