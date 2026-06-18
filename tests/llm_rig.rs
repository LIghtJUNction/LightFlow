mod support;

use std::fs;
#[cfg(not(feature = "rig"))]
use std::process::Command;
#[cfg(feature = "rig")]
use support::lfw;
use support::{unique_temp_root, write_workflow_crate};

#[cfg(feature = "rig")]
fn write_rig_workflow(root: &std::path::Path) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(
        root.join("Cargo.toml"),
        format!(
            r#"[workspace]
resolver = "3"
members = ["workflows/*/*"]

[workspace.dependencies]
lightflow = {{ path = {:?}, features = ["rig"] }}
"#,
            env!("CARGO_MANIFEST_DIR")
        ),
    )?;
    write_workflow_crate(
        root,
        "lightflow.test_rig_llm",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.test_rig_llm")
        .version("0.1.0")
        .name("Test RIG LLM")
        .description("Generate text through the LightFlow RIG LLM runtime.")
        .input("prompt", "text")
        .input("system", "text")
        .input("provider", "text")
        .input("model", "text")
        .input("api_key", "text")
        .input("base_url", "text")
        .input("temperature", "number")
        .input("max_tokens", "integer")
        .input("additional_params", "json")
        .output("text", "text")
        .output("response", "text")
        .output("provider", "text")
        .output("model", "text")
        .runtime("rig_runtime", "lightflow.llm.generate")
        .build()
}
"#,
    )?;
    Ok(())
}

#[test]
#[cfg(feature = "rig")]
fn rig_llm_runtime_runs_mock_provider() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    write_rig_workflow(&root)?;

    let execution = lfw(
        &root,
        [
            "run",
            "lightflow.test_rig_llm",
            "-i",
            "provider=\"mock\"",
            "-i",
            "model=\"fake-llm\"",
            "-i",
            "prompt=\"hello\"",
        ],
    )?;
    assert_eq!(execution["outputs"]["text"], "mock:fake-llm:hello");
    assert_eq!(execution["outputs"]["response"], "mock:fake-llm:hello");
    assert_eq!(execution["outputs"]["provider"], "mock");
    assert_eq!(execution["outputs"]["model"], "fake-llm");

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
#[cfg(not(feature = "rig"))]
fn rig_llm_runtime_requires_rig_feature() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    fs::write(
        root.join("Cargo.toml"),
        format!(
            r#"[workspace]
resolver = "3"
members = ["workflows/*/*"]

[workspace.dependencies]
lightflow = {{ path = {:?} }}
"#,
            env!("CARGO_MANIFEST_DIR")
        ),
    )?;
    write_workflow_crate(
        &root,
        "lightflow.test_rig_llm",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.test_rig_llm")
        .version("0.1.0")
        .name("Test RIG LLM")
        .input("prompt", "text")
        .input("provider", "text")
        .input("model", "text")
        .output("text", "text")
        .runtime("rig_runtime", "lightflow.llm.generate")
        .build()
}
"#,
    )?;

    let output = Command::new(env!("CARGO_BIN_EXE_lfw"))
        .args([
            "run",
            "lightflow.test_rig_llm",
            "-i",
            "provider=\"mock\"",
            "-i",
            "model=\"fake-llm\"",
            "-i",
            "prompt=\"hello\"",
        ])
        .current_dir(&root)
        .env("HOME", &root)
        .env("SHELL", "/bin/zsh")
        .env("XDG_CONFIG_HOME", root.join(".test-xdg/config"))
        .env("XDG_DATA_HOME", root.join(".test-xdg/data"))
        .env_remove("LFW_PATH")
        .output()?;
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--features rig"),
        "stderr:\n{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}
