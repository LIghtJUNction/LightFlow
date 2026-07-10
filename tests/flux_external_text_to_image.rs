mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use support::*;

#[test]
fn flux_text_to_image_uses_external_runner_contract() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(root.join("models"))?;
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
        &root,
        "lightflow.test_flux",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
        .name("Test FLUX")
        .input("prompt", "text")
        .input("negative", "text")
        .input("width", "integer")
        .input("height", "integer")
        .input("seed", "integer")
        .input("steps", "integer")
        .input("guidance", "number")
        .input("output_path", "path")
        .output("image", "artifact")
        .output("image_path", "path")
        .runtime("flux_runtime", "lightflow.image.generate")
        .hf_model("flux_model", "flux-test", "text-to-image", "gguf", "local/flux", "flux.gguf")
        .hf_model("llm_model", "llm-test", "language-model", "gguf", "local/llm", "llm.gguf")
        .hf_model("vae_model", "vae-test", "vae", "safetensors", "local/vae", "vae.safetensors")
        .build()
}
"#,
    )?;

    let flux_model = root.join("models/flux.gguf");
    let llm_model = root.join("models/llm.gguf");
    let vae_model = root.join("models/vae.safetensors");
    fs::write(&flux_model, b"flux")?;
    fs::write(&llm_model, b"llm")?;
    fs::write(&vae_model, b"vae")?;
    fs::write(
        root.join("lfw.lock"),
        serde_json::json!({
            "version": 1,
            "models": {
                "lightflow.test_flux::flux_model": { "local_paths": [flux_model] },
                "lightflow.test_flux::llm_model": { "local_paths": [llm_model] },
                "lightflow.test_flux::vae_model": { "local_paths": [vae_model] }
            }
        })
        .to_string(),
    )?;

    let fixture = root.join("runner-source.png");
    fs::write(&fixture, PNG_FIXTURE)?;
    let runner_log = root.join("runner-args.txt");
    let runner = root.join("flux-runner.sh");
    fs::write(
        &runner,
        format!(
            r#"#!/bin/sh
set -eu
out=""
while [ "$#" -gt 0 ]; do
  printf '%s\n' "$1" >> {log:?}
  case "$1" in
    --output)
      shift
      out="$1"
      printf '%s\n' "$1" >> {log:?}
      ;;
    *)
      ;;
  esac
  shift || true
done
test -n "$out"
mkdir -p "$(dirname "$out")"
cp {fixture:?} "$out"
"#,
            log = runner_log,
            fixture = fixture,
        ),
    )?;
    let mut permissions = fs::metadata(&runner)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&runner, permissions)?;

    let output_path = root.join("out/flux.png");
    let execution = lfw_with_env_values(
        &root,
        [
            "run",
            "lightflow.test_flux",
            "--prompt",
            "a red cabin",
            "-i",
            "negative=\"blur\"",
            "-i",
            "width=128",
            "-i",
            "height=96",
            "-i",
            "seed=77",
            "-i",
            "steps=2",
            "-i",
            "guidance=3.25",
            "--output",
            output_path.to_str().unwrap(),
        ],
        [
            ("LIGHTFLOW_FLUX_BACKEND", "external"),
            ("LIGHTFLOW_FLUX_RUNNER", runner.to_str().unwrap()),
        ],
    )?;

    assert_eq!(execution["workflow_id"], "lightflow.test_flux");
    assert_eq!(
        execution["outputs"]["image_path"],
        output_path.to_str().unwrap()
    );
    assert_eq!(
        execution["artifacts"][0]["metadata"]["engine"],
        "flux2-klein.gguf.runner.v1"
    );
    assert!(fs::read(&output_path)?.starts_with(b"\x89PNG\r\n\x1a\n"));
    let runner_args = fs::read_to_string(&runner_log)?;
    assert!(runner_args.contains("--task\ntext-to-image\n"));
    assert!(runner_args.contains("--prompt\na red cabin\n"));
    assert!(runner_args.contains("--width\n128\n"));
    assert!(runner_args.contains("--height\n96\n"));
    assert!(runner_args.contains("--seed\n77\n"));
    assert!(runner_args.contains("--steps\n2\n"));
    assert!(runner_args.contains("--guidance\n3.25\n"));
    assert!(runner_args.contains("--flux-model\n"));
    assert!(runner_args.contains("models/flux.gguf"));
    assert!(runner_args.contains("--llm-model\n"));
    assert!(runner_args.contains("models/llm.gguf"));
    assert!(runner_args.contains("--vae-model\n"));
    assert!(runner_args.contains("models/vae.safetensors"));

    let templated = lfw_with_env_values(
        &root,
        [
            "run",
            "lightflow.test_flux",
            "--prompt",
            "five cats",
            "-i",
            "seed=80",
            "-i",
            "count=3",
            "-i",
            "output_template=\"out/cat-{index:03}-{seed}.png\"",
        ],
        [
            ("LIGHTFLOW_FLUX_BACKEND", "external"),
            ("LIGHTFLOW_FLUX_RUNNER", runner.to_str().unwrap()),
        ],
    )?;
    assert_eq!(templated["artifacts"].as_array().unwrap().len(), 3);
    assert_eq!(
        templated["outputs"]["image_paths"],
        serde_json::json!([
            "out/cat-001-80.png",
            "out/cat-002-81.png",
            "out/cat-003-82.png"
        ])
    );
    assert_eq!(templated["artifacts"][2]["metadata"]["index"], 3);
    assert_eq!(templated["artifacts"][2]["metadata"]["count"], 3);
    for path in [
        "out/cat-001-80.png",
        "out/cat-002-81.png",
        "out/cat-003-82.png",
    ] {
        assert!(fs::read(root.join(path))?.starts_with(b"\x89PNG\r\n\x1a\n"));
    }
    let runner_args = fs::read_to_string(&runner_log)?;
    assert!(runner_args.contains("--seed\n80\n"));
    assert!(runner_args.contains("--seed\n81\n"));
    assert!(runner_args.contains("--seed\n82\n"));

    fs::create_dir_all(root.join(".test-xdg/config"))?;
    fs::write(
        root.join(".test-xdg/config/user-dirs.dirs"),
        r#"XDG_PICTURES_DIR="$HOME/Images"
"#,
    )?;
    let xdg_default = lfw_with_env_values(
        &root,
        [
            "run",
            "lightflow.test_flux",
            "--prompt",
            "two cats",
            "-i",
            "seed=90",
            "-i",
            "count=2",
        ],
        [
            ("LIGHTFLOW_FLUX_BACKEND", "external"),
            ("LIGHTFLOW_FLUX_RUNNER", runner.to_str().unwrap()),
        ],
    )?;
    let expected_default_paths = [
        root.join("Images/lightflow/lightflow_test_flux/90-001.png"),
        root.join("Images/lightflow/lightflow_test_flux/91-002.png"),
    ];
    assert_eq!(
        xdg_default["outputs"]["image_paths"],
        serde_json::Value::Array(
            expected_default_paths
                .iter()
                .map(|path| path.display().to_string().into())
                .collect()
        )
    );
    for path in expected_default_paths {
        assert!(fs::read(path)?.starts_with(b"\x89PNG\r\n\x1a\n"));
    }

    let _ = fs::remove_dir_all(root);
    Ok(())
}

const PNG_FIXTURE: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x04, 0x00, 0x00, 0x00, 0xb5, 0x1c, 0x0c,
    0x02, 0x00, 0x00, 0x00, 0x0b, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xfc, 0xff, 0x1f, 0x00,
    0x03, 0x03, 0x02, 0x00, 0xef, 0xbf, 0xa7, 0xdb, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44,
    0xae, 0x42, 0x60, 0x82,
];
