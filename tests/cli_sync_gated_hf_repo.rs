mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use support::*;

#[test]
fn sync_apply_reports_hf_repo_url_when_approval_is_required()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    fs::write(
        root.join("Cargo.toml"),
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
        &root,
        "lightflow.gated_image",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
        .name("Gated Image")
        .input("prompt", "text")
        .output("image", "artifact")
        .hf_model(
            "image_model",
            "gated",
            "text-to-image",
            "safetensors",
            "black-forest-labs/FLUX.1-dev",
            "ae.safetensors"
        )
        .build()
}
"#,
    )?;

    let fake_bin = root.join("fake-bin");
    fs::create_dir_all(&fake_bin)?;
    let hf_path = fake_bin.join("python3");
    fs::write(
        &hf_path,
        "#!/bin/sh\nprintf 'Error: Access denied. This repository requires approval.\\n' >&2\nexit 1\n",
    )?;
    let mut permissions = fs::metadata(&hf_path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&hf_path, permissions)?;
    let path = format!(
        "{}:{}",
        fake_bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let output = Command::new(env!("CARGO_BIN_EXE_lfw"))
        .args([
            "sync",
            "lightflow.gated_image",
            "--model",
            "image_model=gated",
            "--apply",
        ])
        .current_dir(&root)
        .env("HOME", &root)
        .env("SHELL", "/bin/zsh")
        .env("XDG_CONFIG_HOME", root.join(".test-xdg/config"))
        .env("XDG_DATA_HOME", root.join(".test-xdg/data"))
        .env("PATH", path)
        .env_remove("LFW_PATH")
        .output()?;
    assert!(
        !output.status.success(),
        "sync unexpectedly succeeded\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("Error: Access denied. This repository requires approval."));
    assert!(stderr.contains("repo: black-forest-labs/FLUX.1-dev"));
    assert!(stderr.contains("repo_url: https://huggingface.co/black-forest-labs/FLUX.1-dev"));
    assert!(stderr.contains("file: ae.safetensors"));
    assert!(stderr.contains(
        "file_url: https://huggingface.co/black-forest-labs/FLUX.1-dev/resolve/main/ae.safetensors"
    ));
    assert!(stderr.contains("accept the access terms"));

    let _ = fs::remove_dir_all(root);
    Ok(())
}
