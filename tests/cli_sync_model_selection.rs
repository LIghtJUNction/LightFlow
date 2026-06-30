mod support;

use std::fs;
use std::io::Write;
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
members = [".lightflow/workflows/*/*"]

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
