mod support;

use std::fs;
use std::io::Write;
use std::process::{Command, Stdio};
use support::*;

#[test]
fn sync_installs_agent_skills_once_and_locks_choice() -> Result<(), Box<dyn std::error::Error>> {
    let project = unique_temp_root();
    fs::create_dir_all(&project)?;
    fs::write(
        project.join("Cargo.toml"),
        format!(
            r#"[workspace]
resolver = "3"
members = [".lightflow/workflows/*/*"]

[workspace.dependencies]
lightflow = {{ path = {:?} }}
"#,
            env!("CARGO_MANIFEST_DIR")
        ),
    )?;
    write_workflow_crate(
        &project,
        "lightflow.skillful",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
        .name("Skillful")
        .input("value", "json")
        .output("value", "json")
        .build()
}
"#,
    )?;
    let skill_dir =
        project.join(".lightflow/workflows/tests/skillful/.agent/skills/lightflow-skillful");
    fs::create_dir_all(&skill_dir)?;
    fs::write(
        skill_dir.join("SKILL.md"),
        r#"---
name: LightFlow Skillful
description: This skill should be used when working with lightflow.skillful.
version: 0.1.0
---

# LightFlow Skillful
"#,
    )?;

    let mut first = Command::new(env!("CARGO_BIN_EXE_lfw"))
        .args(["sync", "lightflow.skillful", "--apply"])
        .current_dir(&project)
        .env("HOME", &project)
        .env("SHELL", "/bin/zsh")
        .env("XDG_CONFIG_HOME", project.join(".test-xdg/config"))
        .env("XDG_DATA_HOME", project.join(".test-xdg/data"))
        .env_remove("LFW_PATH")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    first
        .stdin
        .as_mut()
        .expect("sync skill stdin")
        .write_all(b"p\n")?;
    let first = first.wait_with_output()?;
    assert!(
        first.status.success(),
        "first sync failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&first.stdout),
        String::from_utf8_lossy(&first.stderr)
    );
    let first: serde_json::Value = serde_json::from_slice(&first.stdout)?;
    assert_eq!(first["agent_skills"]["installed"][0]["scope"], "project");
    let link = project.join(".agents/skills/lightflow-skillful");
    assert!(fs::symlink_metadata(&link)?.file_type().is_symlink());
    assert_eq!(fs::read_link(&link)?, skill_dir.canonicalize()?);
    let lock: serde_json::Value = serde_json::from_slice(&fs::read(project.join("lfw.lock"))?)?;
    assert_eq!(
        lock["skills"].as_object().unwrap().values().next().unwrap()["choice"],
        "project"
    );

    let second = Command::new(env!("CARGO_BIN_EXE_lfw"))
        .args(["sync", "lightflow.skillful", "--apply"])
        .current_dir(&project)
        .env("HOME", &project)
        .env("SHELL", "/bin/zsh")
        .env("XDG_CONFIG_HOME", project.join(".test-xdg/config"))
        .env("XDG_DATA_HOME", project.join(".test-xdg/data"))
        .env_remove("LFW_PATH")
        .output()?;
    assert!(
        second.status.success(),
        "second sync failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&second.stdout),
        String::from_utf8_lossy(&second.stderr)
    );
    let second: serde_json::Value = serde_json::from_slice(&second.stdout)?;
    assert_eq!(second["agent_skills"]["installed"], serde_json::json!([]));
    assert_eq!(second["agent_skills"]["locked"][0]["choice"], "project");

    let _ = fs::remove_dir_all(project);
    Ok(())
}
