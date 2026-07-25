#![cfg(unix)]

mod support;

use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use support::{lfw_command, unique_temp_root, write_workflow_crate};

#[test]
fn command_runtime_executes_versioned_json_contract() -> Result<(), Box<dyn std::error::Error>> {
    let project = TestProject::new()?;
    write_workflow_crate(
        project.path(),
        "lightflow.command_fixture",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
        .name("Command Fixture")
        .input("value", "json")
        .output("result", "json")
        .output("artifact", "artifact")
        .builtin_runtime(
            "command",
            "lightflow.command.run",
            "process.command.v1",
        )
        .build()
}
"#,
    )?;
    let runner = project.executable(
        "runner",
        r#"request="$PWD/request.json"
cat >"$request"
printf 'artifact' >"$PWD/artifact.bin"
printf '%s\n' '{"outputs":{"result":{"status":"ok"},"artifact":{"id":"fixture","kind":"data","path":"artifact.bin","mime_type":"application/octet-stream","metadata":{}}},"artifacts":[{"id":"fixture","kind":"data","path":"artifact.bin","mime_type":"application/octet-stream","metadata":{}}],"replay_fingerprint":{"runner":"command-runtime-test","version":1}}'"#,
    )?;

    let output = lfw_command(project.path())
        .args([
            "run",
            "lightflow.command_fixture",
            "--input",
            r#"value={"message":"hello"}"#,
        ])
        .env("LIGHTFLOW_COMMAND_RUNNER", runner)
        .output()?;
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let execution: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(execution["outputs"]["result"]["status"], "ok");
    assert_eq!(execution["runtime"]["executor_id"], "process.command.v1");
    assert_eq!(execution["artifacts"][0]["path"], "artifact.bin");

    let request: Value = serde_json::from_slice(&fs::read(project.path().join("request.json"))?)?;
    assert_eq!(request["protocol"], "lightflow.command.v1");
    assert_eq!(request["workflow"]["id"], "lightflow.command_fixture");
    assert_eq!(request["inputs"]["value"]["message"], "hello");
    Ok(())
}

#[test]
fn command_runtime_reports_missing_runner_before_execution()
-> Result<(), Box<dyn std::error::Error>> {
    let project = TestProject::new()?;
    write_workflow_crate(
        project.path(),
        "lightflow.command_fixture",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
        .name("Command Fixture")
        .output("result", "json")
        .builtin_runtime(
            "command",
            "lightflow.command.run",
            "process.command.v1",
        )
        .build()
}
"#,
    )?;

    let output = lfw_command(project.path())
        .args(["run", "lightflow.command_fixture"])
        .env_remove("LIGHTFLOW_COMMAND_RUNNER")
        .output()?;
    assert!(!output.status.success());
    assert!(
        String::from_utf8_lossy(&output.stderr)
            .contains("set LIGHTFLOW_COMMAND_RUNNER to enable this executor")
    );
    Ok(())
}

struct TestProject {
    root: PathBuf,
}

impl TestProject {
    fn new() -> Result<Self, Box<dyn std::error::Error>> {
        let root = unique_temp_root();
        fs::create_dir_all(&root)?;
        fs::write(
            root.join("Cargo.toml"),
            format!(
                r#"[workspace]
resolver = "3"
members = [".lightflow/workflows/*"]

[workspace.dependencies]
lightflow = {{ path = {:?} }}
"#,
                env!("CARGO_MANIFEST_DIR")
            ),
        )?;
        Ok(Self { root })
    }

    fn path(&self) -> &Path {
        &self.root
    }

    fn executable(&self, name: &str, body: &str) -> std::io::Result<PathBuf> {
        let path = self.root.join(name);
        fs::write(&path, format!("#!/bin/sh\nset -eu\n{body}\n"))?;
        fs::set_permissions(&path, fs::Permissions::from_mode(0o700))?;
        Ok(path)
    }
}

impl Drop for TestProject {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.root);
    }
}
