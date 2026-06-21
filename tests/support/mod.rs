#![allow(dead_code)]

use lightflow::api::ApiService;
use lightflow::cli::mcp;
use serde_json::Value;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn lightflow<const N: usize>(
    root: &Path,
    args: [&str; N],
) -> Result<Value, Box<dyn std::error::Error>> {
    run_json(env!("CARGO_BIN_EXE_lightflow"), root, &args)
}

pub fn lfw<const N: usize>(
    root: &Path,
    args: [&str; N],
) -> Result<Value, Box<dyn std::error::Error>> {
    run_json(env!("CARGO_BIN_EXE_lfw"), root, &args)
}

pub fn lfw_command(root: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_lfw"));
    command
        .current_dir(root)
        .env("HOME", root)
        .env("SHELL", "/bin/zsh")
        .env("XDG_CONFIG_HOME", root.join(".test-xdg/config"))
        .env("XDG_DATA_HOME", root.join(".test-xdg/data"))
        .env_remove("LFW_PATH");
    command
}

pub fn lfw_with_env<const N: usize, const M: usize>(
    root: &Path,
    args: [&str; N],
    envs: [(&str, &Path); M],
) -> Result<Value, Box<dyn std::error::Error>> {
    run_json_with_env(env!("CARGO_BIN_EXE_lfw"), root, &args, &envs)
}

pub fn lfw_with_env_values<const N: usize, const M: usize>(
    root: &Path,
    args: [&str; N],
    envs: [(&str, &str); M],
) -> Result<Value, Box<dyn std::error::Error>> {
    run_json_with_env_values(env!("CARGO_BIN_EXE_lfw"), root, &args, &envs)
}

pub fn lfwx<const N: usize>(
    root: &Path,
    args: [&str; N],
) -> Result<Value, Box<dyn std::error::Error>> {
    run_json(env!("CARGO_BIN_EXE_lfwx"), root, &args)
}

pub fn lfx<const N: usize>(
    root: &Path,
    args: [&str; N],
) -> Result<Value, Box<dyn std::error::Error>> {
    run_json(env!("CARGO_BIN_EXE_lfx"), root, &args)
}

pub fn run_json(
    binary: &str,
    root: &Path,
    args: &[&str],
) -> Result<Value, Box<dyn std::error::Error>> {
    run_json_with_env(binary, root, args, &[])
}

pub fn run_json_with_env(
    binary: &str,
    root: &Path,
    args: &[&str],
    envs: &[(&str, &Path)],
) -> Result<Value, Box<dyn std::error::Error>> {
    let mut command = Command::new(binary);
    command
        .args(args)
        .current_dir(root)
        .env("HOME", root)
        .env("SHELL", "/bin/zsh")
        .env("XDG_CONFIG_HOME", root.join(".test-xdg/config"))
        .env("XDG_DATA_HOME", root.join(".test-xdg/data"))
        .env_remove("LFW_PATH");
    for (key, value) in envs {
        command.env(key, value);
    }
    let output = command.output()?;
    if !output.status.success() {
        return Err(format!(
            "{binary} failed with status {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    Ok(serde_json::from_slice(&output.stdout)?)
}

pub fn run_json_with_env_values(
    binary: &str,
    root: &Path,
    args: &[&str],
    envs: &[(&str, &str)],
) -> Result<Value, Box<dyn std::error::Error>> {
    let mut command = Command::new(binary);
    command
        .args(args)
        .current_dir(root)
        .env("HOME", root)
        .env("SHELL", "/bin/zsh")
        .env("XDG_CONFIG_HOME", root.join(".test-xdg/config"))
        .env("XDG_DATA_HOME", root.join(".test-xdg/data"))
        .env_remove("LFW_PATH");
    for (key, value) in envs {
        command.env(key, value);
    }
    let output = command.output()?;
    if !output.status.success() {
        return Err(format!(
            "{binary} failed with status {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }

    Ok(serde_json::from_slice(&output.stdout)?)
}

pub fn mcp_tool(service: &ApiService, name: &str, arguments: Value) -> Value {
    mcp_result(
        service,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 99,
            "method": "tools/call",
            "params": {
                "name": name,
                "arguments": arguments
            }
        }),
    )["structuredContent"]
        .clone()
}

pub fn mcp_result(service: &ApiService, request: Value) -> Value {
    let response = mcp::handle_request(service, request);
    assert!(response.get("error").is_none(), "MCP error: {response}");
    response["result"].clone()
}

pub fn write_project_specs(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(root.join("workflows"))?;
    fs::write(
        root.join("Cargo.toml"),
        r#"[workspace]
resolver = "3"
members = ["workflows/*/*"]

[workspace.dependencies]
lightflow = { path = "." }
"#,
    )?;
    write_workflow_crate(
        root,
        "lightflow.child",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.child")
        .version("0.1.0")
        .name("Child")
        .input("in", "json")
        .output("out", "json")
        .build()
}
"#,
    )?;
    write_workflow_crate(
        root,
        "lightflow.sink",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.sink")
        .version("0.1.0")
        .name("Sink")
        .input("in", "json")
        .build()
}
"#,
    )?;
    write_workflow_crate(
        root,
        "lightflow.parent",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.parent")
        .version("0.1.0")
        .name("Parent")
        .input("in", "json")
        .output("out", "json")
        .depends_on("lightflow.child", "0.1.0")
        .node("nested", "lightflow.child")
        .node("sink", "lightflow.sink")
        .edge("nested", "out", "sink", "in")
        .build()
}
"#,
    )?;
    Ok(())
}

pub fn write_external_std_crate(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(root.join("src"))?;
    fs::write(
        root.join("Cargo.toml"),
        format!(
            r#"[package]
name = "lightflow-std"
version = "0.1.0"
edition = "2024"

[dependencies]
lightflow = {{ path = {:?} }}
"#,
            env!("CARGO_MANIFEST_DIR")
        ),
    )?;
    fs::write(
        root.join("src/lib.rs"),
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.std")
        .version("0.1.0")
        .name("LightFlow Std Identity")
        .input("value", "json")
        .output("value", "json")
        .build()
}
"#,
    )?;
    Ok(())
}

pub fn write_publishable_extension_crate(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::create_dir_all(root.join("src"))?;
    fs::write(
        root.join("Cargo.toml"),
        r#"[package]
name = "lightflow-extension"
version = "0.1.0"
edition = "2024"
description = "Test LightFlow extension crate."
license = "MIT OR Apache-2.0"

[dependencies]
"#,
    )?;
    fs::write(
        root.join("src/lib.rs"),
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.extension")
        .version("0.1.0")
        .name("Extension")
        .input("value", "json")
        .output("value", "json")
        .build()
}
"#,
    )?;
    Ok(())
}

pub fn write_workflow_crate(
    root: &Path,
    workflow_id: &str,
    source: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let crate_dir = root
        .join("workflows")
        .join("tests")
        .join(workflow_dir_name(workflow_id));
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

pub fn write_workflow_crate_in(
    collection: &Path,
    workflow_id: &str,
    source: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let crate_dir = collection
        .join("local")
        .join(workflow_dir_name(workflow_id));
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
lightflow = {{ path = {:?} }}
"#,
            workflow_id.replace('.', "-"),
            env!("CARGO_MANIFEST_DIR")
        ),
    )?;
    fs::write(crate_dir.join("src/lib.rs"), source)?;
    Ok(())
}

fn workflow_dir_name(workflow_id: &str) -> String {
    workflow_id
        .strip_prefix("lightflow.")
        .unwrap_or(workflow_id)
        .replace('.', "_")
}

pub fn unique_temp_root() -> PathBuf {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system clock must be after unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("lightflow-cli-test-{}-{nanos}", std::process::id()))
}
