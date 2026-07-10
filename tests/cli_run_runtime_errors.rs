mod support;

use std::fs;
use std::process::Command;
use support::*;

#[test]
fn lfw_run_rejects_unknown_leaf_runtime() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    write_workflow_crate(
        &root,
        "lightflow.unknown_runtime",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
        .name("Unknown Runtime")
        .input("prompt", "text")
        .output("image", "artifact")
        .runtime("runtime", "lightflow.image.inpaint")
        .build()
}
"#,
    )?;

    let output = Command::new(env!("CARGO_BIN_EXE_lfw"))
        .args(["run", "lightflow.unknown_runtime", "--input", "prompt=test"])
        .current_dir(&root)
        .env("HOME", &root)
        .env("SHELL", "/bin/zsh")
        .env("XDG_CONFIG_HOME", root.join(".test-xdg/config"))
        .env("XDG_DATA_HOME", root.join(".test-xdg/data"))
        .env_remove("LFW_PATH")
        .output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("has no executor"));
    assert!(stderr.contains("lightflow.image.inpaint"));
    assert!(stderr.contains("run_id: run-"));
    assert!(stderr.contains("trace_path:"));

    let trace = lfw(&root, ["trace", "last"])?;
    assert_eq!(trace["manifest"]["status"], "failed");
    assert_eq!(trace["execution"]["status"], "failed");
    assert!(
        trace["execution"]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("lightflow.image.inpaint")
    );
    assert_eq!(trace["events"][0]["event"], "run_started");
    assert_eq!(trace["events"][1]["event"], "run_failed");
    assert!(
        trace["events"][1]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("has no executor")
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}
