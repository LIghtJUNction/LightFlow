mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::process::Command;
use support::*;

#[test]
fn sync_apply_writes_lfw_lock_with_model_hash() -> Result<(), Box<dyn std::error::Error>> {
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
        "lightflow.locked_image",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.locked_image")
        .version("0.1.0")
        .name("Locked Image")
        .input("prompt", "text")
        .output("image", "artifact")
        .hf_model(
            "image_model",
            "tiny-q4",
            "text-to-image",
            "gguf",
            "example/tiny",
            "tiny.gguf"
        )
        .build()
}
"#,
    )?;

    let cache = root.join("cache");
    let model_path = cache
        .join("models--example--tiny")
        .join("snapshots")
        .join("abc123")
        .join("tiny.gguf");
    fs::create_dir_all(model_path.parent().unwrap())?;
    fs::write(&model_path, b"tiny model bytes\n")?;
    let fake_bin = root.join("fake-bin");
    fs::create_dir_all(&fake_bin)?;
    let hf_path = fake_bin.join("python3");
    fs::write(
        &hf_path,
        format!(
            "#!/bin/sh\nprintf '{{\"path\":\"{}\"}}\\n'\nexit 1\n",
            model_path.display()
        ),
    )?;
    let mut permissions = fs::metadata(&hf_path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&hf_path, permissions)?;
    let path = format!(
        "{}:{}",
        fake_bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );

    let missing_lock = Command::new(env!("CARGO_BIN_EXE_lfw"))
        .args([
            "sync",
            "lightflow.locked_image",
            "--model",
            "image_model=tiny-q4",
            "--locked",
            "--apply",
        ])
        .current_dir(&root)
        .env("HOME", &root)
        .env("SHELL", "/bin/zsh")
        .env("XDG_CONFIG_HOME", root.join(".test-xdg/config"))
        .env("XDG_DATA_HOME", root.join(".test-xdg/data"))
        .env("PATH", &path)
        .env_remove("LFW_PATH")
        .output()?;
    assert!(!missing_lock.status.success());
    assert!(
        String::from_utf8_lossy(&missing_lock.stderr).contains("sync --locked requires"),
        "stderr:\n{}",
        String::from_utf8_lossy(&missing_lock.stderr)
    );

    let output = Command::new(env!("CARGO_BIN_EXE_lfw"))
        .args([
            "sync",
            "lightflow.locked_image",
            "--model",
            "image_model=tiny-q4",
            "--apply",
        ])
        .current_dir(&root)
        .env("HOME", &root)
        .env("SHELL", "/bin/zsh")
        .env("XDG_CONFIG_HOME", root.join(".test-xdg/config"))
        .env("XDG_DATA_HOME", root.join(".test-xdg/data"))
        .env("PATH", &path)
        .env_remove("LFW_PATH")
        .output()?;
    assert!(
        output.status.success(),
        "sync failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let sync: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(sync["dry_run"], false);
    assert_eq!(sync["executed"][1]["sha256"].as_str().unwrap().len(), 64);
    assert_eq!(
        sync["executed"][1]["local_paths"][0],
        model_path.to_str().unwrap()
    );

    let lock: serde_json::Value = serde_json::from_slice(&fs::read(root.join("lfw.lock"))?)?;
    let entry = &lock["models"]["lightflow.locked_image::image_model"];
    assert_eq!(entry["repo"], "example/tiny");
    assert_eq!(entry["file"], "tiny.gguf");
    assert_eq!(entry["variant_id"], "tiny-q4");
    assert_eq!(entry["sha256"].as_str().unwrap().len(), 64);
    assert_eq!(entry["hash_algorithm"], "sha256");
    assert_eq!(entry["size_bytes"], 17);
    assert_eq!(entry["snapshot_revision"], "abc123");

    fs::write(&hf_path, "#!/bin/sh\nexit 99\n")?;
    let locked = Command::new(env!("CARGO_BIN_EXE_lfw"))
        .args([
            "sync",
            "lightflow.locked_image",
            "--model",
            "image_model=tiny-q4",
            "--locked",
            "--apply",
        ])
        .current_dir(&root)
        .env("HOME", &root)
        .env("SHELL", "/bin/zsh")
        .env("XDG_CONFIG_HOME", root.join(".test-xdg/config"))
        .env("XDG_DATA_HOME", root.join(".test-xdg/data"))
        .env("PATH", &path)
        .env_remove("LFW_PATH")
        .output()?;
    assert!(
        locked.status.success(),
        "locked sync failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&locked.stdout),
        String::from_utf8_lossy(&locked.stderr)
    );
    let locked: serde_json::Value = serde_json::from_slice(&locked.stdout)?;
    assert_eq!(locked["dry_run"], false);
    assert_eq!(locked["executed"], serde_json::json!([]));
    assert_eq!(locked["locked"]["checks"][0]["status"], "verified");

    let _ = fs::remove_dir_all(root);
    Ok(())
}
