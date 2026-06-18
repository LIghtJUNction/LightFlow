mod support;

use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Stdio};
use support::*;

#[test]
fn cargo_path_dependency_installs_workflow_for_dependency_resolution()
-> Result<(), Box<dyn std::error::Error>> {
    let base = unique_temp_root();
    let project = base.join("project");
    let std_dep = base.join("lightflow-std");
    fs::create_dir_all(&project)?;
    write_external_std_crate(&std_dep)?;

    fs::write(
        project.join("Cargo.toml"),
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
    let added_dep = lfw(
        &project,
        ["add", "lightflow-std", "--path", "../lightflow-std"],
    )?;
    assert_eq!(added_dep["dependency"], "lightflow-std");
    assert_eq!(added_dep["source"]["path"], "../lightflow-std");
    let manifest = fs::read_to_string(project.join("Cargo.toml"))?;
    assert!(manifest.contains("lightflow-std = { path = \"../lightflow-std\" }"));

    write_workflow_crate(
        &project,
        "lightflow.image_prompt",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.image_prompt")
        .version("0.1.0")
        .name("Image Prompt")
        .input("positive", "text")
        .input("negative", "text")
        .output("prompt", "json")
        .depends_on("lightflow.std", "0.1.0")
        .hf_model(
            "image_model",
            "flux2-safetensors",
            "text-to-image",
            "safetensors",
            "black-forest-labs/FLUX.2-dev",
            "flux2-dev.safetensors"
        )
        .hf_model(
            "image_model",
            "flux2-q4-k-m",
            "text-to-image",
            "gguf",
            "unsloth/FLUX.2-klein-9B-GGUF",
            "flux-2-klein-9b-Q4_K_M.gguf"
        )
        .hf_model(
            "image_model",
            "flux2-gguf",
            "text-to-image",
            "gguf",
            "city96/FLUX.2-dev-gguf",
            "flux2-dev-q4.gguf"
        )
        .node("passthrough", "lightflow.std")
        .build()
}
"#,
    )?;

    let list = lfw(&project, ["list"])?;
    let ids = list["workflows"]
        .as_array()
        .expect("workflows list returns an array")
        .iter()
        .map(|workflow| workflow["id"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["lightflow.image_prompt", "lightflow.std"]);

    let deps = lfw(&project, ["deps", "lightflow.image_prompt"])?;
    assert_eq!(deps["complete"], true);
    assert_eq!(
        deps["workflows"],
        serde_json::json!(["lightflow.image_prompt", "lightflow.std"])
    );
    assert_eq!(
        deps["workflow_order"],
        serde_json::json!(["lightflow.std", "lightflow.image_prompt"])
    );

    let sync = lfw(&project, ["sync", "lightflow.image_prompt", "--dry-run"])?;
    assert_eq!(sync["dry_run"], true);
    assert_eq!(sync["hf_downloads"], serde_json::json!([]));
    assert_eq!(sync["unresolved_models"][0]["id"], "image_model");
    assert_eq!(
        sync["unresolved_models"][0]["variants"][0]["id"],
        "flux2-safetensors"
    );
    assert_eq!(
        sync["unresolved_models"][0]["variants"][1]["id"],
        "flux2-q4-k-m"
    );
    assert_eq!(
        sync["unresolved_models"][0]["variants"][2]["id"],
        "flux2-gguf"
    );
    assert_eq!(
        sync["unresolved_models"][0]["variants"][1]["download_url"],
        "https://huggingface.co/unsloth/FLUX.2-klein-9B-GGUF/resolve/main/flux-2-klein-9b-Q4_K_M.gguf"
    );

    let auto_selected = Command::new(env!("CARGO_BIN_EXE_lfw"))
        .args(["sync", "lightflow.image_prompt", "--auto-model"])
        .current_dir(&project)
        .env("HOME", &project)
        .env("SHELL", "/bin/zsh")
        .env("XDG_CONFIG_HOME", project.join(".test-xdg/config"))
        .env("XDG_DATA_HOME", project.join(".test-xdg/data"))
        .env("LFW_GPU_VRAM_MB", "12288")
        .env("LFW_TOTAL_RAM_MB", "32768")
        .env_remove("LFW_PATH")
        .output()?;
    assert!(auto_selected.status.success());
    let auto_selected: serde_json::Value = serde_json::from_slice(&auto_selected.stdout)?;
    assert_eq!(auto_selected["auto_model"]["enabled"], true);
    assert_eq!(
        auto_selected["auto_model"]["selections"][0]["variant_id"],
        "flux2-q4-k-m"
    );
    assert_eq!(auto_selected["unresolved_models"], serde_json::json!([]));
    assert_eq!(
        auto_selected["hf_downloads"][0]["command"],
        serde_json::json!([
            "hf",
            "download",
            "unsloth/FLUX.2-klein-9B-GGUF",
            "flux-2-klein-9b-Q4_K_M.gguf"
        ])
    );

    let mut interactive = Command::new(env!("CARGO_BIN_EXE_lfw"))
        .args(["sync", "lightflow.image_prompt", "--select-model"])
        .current_dir(&project)
        .env("HOME", &project)
        .env("SHELL", "/bin/zsh")
        .env("XDG_CONFIG_HOME", project.join(".test-xdg/config"))
        .env("XDG_DATA_HOME", project.join(".test-xdg/data"))
        .env_remove("LFW_PATH")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()?;
    interactive
        .stdin
        .as_mut()
        .expect("interactive sync stdin")
        .write_all(b"3\n")?;
    let interactive = interactive.wait_with_output()?;
    assert!(
        interactive.status.success(),
        "interactive sync failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&interactive.stdout),
        String::from_utf8_lossy(&interactive.stderr)
    );
    let interactive: serde_json::Value = serde_json::from_slice(&interactive.stdout)?;
    assert_eq!(interactive["hf_downloads"][0]["variant_id"], "flux2-gguf");

    let selected = lfw(
        &project,
        [
            "sync",
            "lightflow.image_prompt",
            "--model",
            "image_model=flux2-gguf",
        ],
    )?;
    assert_eq!(selected["unresolved_models"], serde_json::json!([]));
    assert_eq!(selected["hf_downloads"][0]["format"], "gguf");
    assert_eq!(
        selected["hf_downloads"][0]["command"],
        serde_json::json!([
            "hf",
            "download",
            "city96/FLUX.2-dev-gguf",
            "flux2-dev-q4.gguf"
        ])
    );
    assert_eq!(
        selected["hf_downloads"][0]["download_url"],
        "https://huggingface.co/city96/FLUX.2-dev-gguf/resolve/main/flux2-dev-q4.gguf"
    );

    let custom = lfw(
        &project,
        [
            "sync",
            "lightflow.image_prompt",
            "--hf-model",
            "image_model=gguf:example/custom-image-model:models/q5_k_m.gguf",
        ],
    )?;
    assert_eq!(custom["unresolved_models"], serde_json::json!([]));
    assert_eq!(custom["hf_downloads"][0]["custom"], true);
    assert_eq!(custom["hf_downloads"][0]["variant_id"], "custom");
    assert_eq!(custom["hf_downloads"][0]["format"], "gguf");
    assert_eq!(
        custom["hf_downloads"][0]["command"],
        serde_json::json!([
            "hf",
            "download",
            "example/custom-image-model",
            "models/q5_k_m.gguf"
        ])
    );
    assert_eq!(
        custom["hf_downloads"][0]["download_url"],
        "https://huggingface.co/example/custom-image-model/resolve/main/models/q5_k_m.gguf"
    );

    let custom_url = lfw(
        &project,
        [
            "sync",
            "lightflow.image_prompt",
            "--hf-url",
            "image_model=https://huggingface.co/example/custom-image-model/resolve/main/models/model.safetensors",
        ],
    )?;
    assert_eq!(custom_url["unresolved_models"], serde_json::json!([]));
    assert_eq!(custom_url["hf_downloads"][0]["custom"], true);
    assert_eq!(custom_url["hf_downloads"][0]["format"], "safetensors");
    assert_eq!(
        custom_url["hf_downloads"][0]["repo"],
        "example/custom-image-model"
    );
    assert_eq!(
        custom_url["hf_downloads"][0]["file"],
        "models/model.safetensors"
    );

    let unknown_requirement = Command::new(env!("CARGO_BIN_EXE_lfw"))
        .args([
            "sync",
            "lightflow.image_prompt",
            "--hf-model",
            "other_model=gguf:example/custom-image-model:models/q5_k_m.gguf",
        ])
        .current_dir(&project)
        .env("HOME", &project)
        .env("SHELL", "/bin/zsh")
        .env("XDG_CONFIG_HOME", project.join(".test-xdg/config"))
        .env("XDG_DATA_HOME", project.join(".test-xdg/data"))
        .env_remove("LFW_PATH")
        .output()?;
    assert!(!unknown_requirement.status.success());
    let stderr = String::from_utf8_lossy(&unknown_requirement.stderr);
    assert!(stderr.contains("unknown model requirement: other_model"));
    assert!(stderr.contains("available requirements: image_model"));

    let unknown_variant = Command::new(env!("CARGO_BIN_EXE_lfw"))
        .args([
            "sync",
            "lightflow.image_prompt",
            "--model",
            "image_model=flux2-q8",
        ])
        .current_dir(&project)
        .env("HOME", &project)
        .env("SHELL", "/bin/zsh")
        .env("XDG_CONFIG_HOME", project.join(".test-xdg/config"))
        .env("XDG_DATA_HOME", project.join(".test-xdg/data"))
        .env_remove("LFW_PATH")
        .output()?;
    assert!(!unknown_variant.status.success());
    let stderr = String::from_utf8_lossy(&unknown_variant.stderr);
    assert!(stderr.contains("unknown variant flux2-q8"));
    assert!(stderr.contains("flux2-safetensors"));
    assert!(stderr.contains("flux2-gguf"));
    assert!(
        stderr.contains(
            "https://huggingface.co/city96/FLUX.2-dev-gguf/resolve/main/flux2-dev-q4.gguf"
        )
    );

    let _ = fs::remove_dir_all(base);
    Ok(())
}
#[test]
fn sync_applies_declared_workflow_module_dependencies() -> Result<(), Box<dyn std::error::Error>> {
    let base = unique_temp_root();
    let project = base.join("project");
    let std_dep = base.join("lightflow-std");
    fs::create_dir_all(&project)?;
    write_external_std_crate(&std_dep)?;

    fs::write(
        project.join("Cargo.toml"),
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
        &project,
        "lightflow.image_prompt",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.image_prompt")
        .version("0.1.0")
        .name("Image Prompt")
        .input("positive", "text")
        .input("negative", "text")
        .output("prompt", "json")
        .depends_on_path("lightflow.std", "0.1.0", "lightflow-std", "../lightflow-std")
        .node("passthrough", "lightflow.std")
        .build()
}
"#,
    )?;

    let dry_run = lfw(&project, ["sync", "lightflow.image_prompt"])?;
    assert_eq!(dry_run["dry_run"], true);
    assert_eq!(
        dry_run["module_dependencies"]["installs"][0]["dependency"],
        "lightflow-std"
    );
    assert_eq!(
        dry_run["module_dependencies"]["installs"][0]["source"]["path"],
        "../lightflow-std"
    );
    let manifest = fs::read_to_string(project.join("Cargo.toml"))?;
    assert!(!manifest.contains("lightflow-std = { path = \"../lightflow-std\" }"));

    let applied = lfw(&project, ["sync", "lightflow.image_prompt", "--apply"])?;
    assert_eq!(applied["dry_run"], false);
    assert_eq!(
        applied["executed"][0]["dependency"],
        serde_json::json!("lightflow-std")
    );
    let manifest = fs::read_to_string(project.join("Cargo.toml"))?;
    assert!(
        manifest.contains("lightflow-std = { version = \"0.1.0\", path = \"../lightflow-std\" }")
    );

    let list = lfw(&project, ["list"])?;
    let ids = list["workflows"]
        .as_array()
        .expect("workflows list returns an array")
        .iter()
        .map(|workflow| workflow["id"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["lightflow.image_prompt", "lightflow.std"]);

    let _ = fs::remove_dir_all(base);
    Ok(())
}

#[test]
fn sync_apply_writes_lfw_lock_with_model_hash() -> Result<(), Box<dyn std::error::Error>> {
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

#[test]
fn sync_installs_agent_skills_once_and_locks_choice() -> Result<(), Box<dyn std::error::Error>> {
    let project = unique_temp_root();
    fs::create_dir_all(&project)?;
    fs::write(
        project.join("Cargo.toml"),
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
        &project,
        "lightflow.skillful",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.skillful")
        .version("0.1.0")
        .name("Skillful")
        .input("value", "json")
        .output("value", "json")
        .build()
}
"#,
    )?;
    let skill_dir = project.join("workflows/tests/skillful/.agent/skills/lightflow-skillful");
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
members = ["workflows/*/*"]

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
    workflow("lightflow.gated_image")
        .version("0.1.0")
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
