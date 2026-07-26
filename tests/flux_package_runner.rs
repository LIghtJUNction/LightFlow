mod support;

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use support::*;

#[test]
fn flux_package_runners_cover_generate_edit_and_inpaint() -> Result<(), Box<dyn std::error::Error>>
{
    let root = unique_temp_root();
    fs::create_dir_all(root.join("models"))?;
    let flux_model = root.join("models/flux.gguf");
    let llm_model = root.join("models/llm.gguf");
    let vae_model = root.join("models/vae.safetensors");
    fs::write(&flux_model, b"flux")?;
    fs::write(&llm_model, b"llm")?;
    fs::write(&vae_model, b"vae")?;
    write_model_lock(&root, &flux_model, &llm_model, &vae_model)?;

    let fixture = root.join("runner-source.png");
    let input = root.join("input.png");
    let mask = root.join("mask.png");
    fs::write(&fixture, PNG_FIXTURE)?;
    fs::write(&input, PNG_FIXTURE)?;
    fs::write(&mask, PNG_FIXTURE)?;

    let runner_log = root.join("runner-args.txt");
    let runner = root.join("flux-backend.sh");
    write_backend_fixture(&runner, &runner_log, &fixture)?;

    let flux_project = Path::new(env!("CARGO_MANIFEST_DIR")).join("projects/lightflow-flux");
    let envs = [
        ("LFW_PATH", flux_project.to_str().unwrap()),
        ("LIGHTFLOW_FLUX_BACKEND", "external"),
        ("LIGHTFLOW_FLUX_RUNNER", runner.to_str().unwrap()),
    ];

    let generated = lfw_with_env_values(
        &root,
        [
            "run",
            "lightflow.flux_text_to_image",
            "--prompt",
            "a red cabin",
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
            "out/generated.png",
        ],
        envs,
    )?;
    assert_runner_execution(&generated, "text-to-image", "lightflow.image.generate");

    let edit_image = format!("image_path={:?}", input.display().to_string());
    let edited = lfw_with_env_values(
        &root,
        [
            "run",
            "lightflow.flux_image_edit",
            "-i",
            &edit_image,
            "--prompt",
            "make it dusk",
            "-i",
            "strength=0.55",
            "--output",
            "out/edited.png",
        ],
        envs,
    )?;
    assert_runner_execution(&edited, "image-edit", "lightflow.image.edit");

    let mask_input = format!("mask_path={:?}", mask.display().to_string());
    let inpainted = lfw_with_env_values(
        &root,
        [
            "run",
            "lightflow.flux_inpaint",
            "-i",
            &edit_image,
            "-i",
            &mask_input,
            "--prompt",
            "repair the center",
            "-i",
            "strength=0.8",
            "--output",
            "out/inpainted.png",
        ],
        envs,
    )?;
    assert_runner_execution(&inpainted, "inpaint", "lightflow.image.inpaint");

    for path in ["out/generated.png", "out/edited.png", "out/inpainted.png"] {
        assert!(fs::read(root.join(path))?.starts_with(b"\x89PNG\r\n\x1a\n"));
    }
    let runner_args = fs::read_to_string(&runner_log)?;
    for expected in [
        "--task\ntext-to-image\n",
        "--width\n128\n",
        "--height\n96\n",
        "--seed\n77\n",
        "--steps\n2\n",
        "--guidance\n3.25\n",
        "--task\nimage-edit\n",
        "--image\n",
        "--strength\n0.55\n",
        "--task\ninpaint\n",
        "--mask\n",
        "--strength\n0.8\n",
    ] {
        assert!(
            runner_args.contains(expected),
            "missing backend arguments {expected:?} in {runner_args}"
        );
    }

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn flux_native_backend_is_compiled_into_the_product_runner()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(root.join("models"))?;
    let flux_model = root.join("models/flux.gguf");
    let llm_model = root.join("models/llm.gguf");
    let vae_model = root.join("models/vae.safetensors");
    fs::write(&flux_model, b"not a real gguf")?;
    fs::write(&llm_model, b"not a real gguf")?;
    fs::write(&vae_model, b"not a real safetensors file")?;
    write_model_lock(&root, &flux_model, &llm_model, &vae_model)?;
    let flux_project = Path::new(env!("CARGO_MANIFEST_DIR")).join("projects/lightflow-flux");

    let output = lfw_command(&root)
        .args([
            "run",
            "lightflow.flux_text_to_image",
            "--prompt",
            "native reachability smoke",
            "--output",
            "out/native.png",
        ])
        .env("LFW_PATH", &flux_project)
        .env("LIGHTFLOW_FLUX_BACKEND", "native")
        .env_remove("LIGHTFLOW_FLUX_RUNNER")
        .output()?;
    assert!(!output.status.success(), "dummy model must fail closed");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("was not built with --features native"),
        "native feature was not forwarded to the product runner: {stderr}"
    );
    assert!(
        stderr.contains("model")
            || stderr.contains("GGUF")
            || stderr.contains("gguf")
            || stderr.contains("load"),
        "expected a model-load failure from compiled native code: {stderr}"
    );
    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn flux_preview_keeps_xdg_output_and_returns_managed_artifact()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(root.join(".test-xdg/config"))?;
    fs::write(
        root.join(".test-xdg/config/user-dirs.dirs"),
        "XDG_PICTURES_DIR=\"$HOME/Images\"\n",
    )?;
    let xdg_pictures = root.with_extension("xdg-pictures");
    fs::create_dir_all(&xdg_pictures)?;
    let flux_project = Path::new(env!("CARGO_MANIFEST_DIR")).join("projects/lightflow-flux");

    let execution = lfw_with_env_values(
        &root,
        [
            "run",
            "lightflow.flux_preview_text_to_image",
            "--prompt",
            "managed XDG artifact",
        ],
        [
            ("LFW_PATH", flux_project.to_str().unwrap()),
            ("XDG_PICTURES_DIR", xdg_pictures.to_str().unwrap()),
        ],
    )?;

    let xdg_output = xdg_pictures.join("lightflow/lightflow_flux_preview_text_to_image/42.png");
    let artifact = Path::new(
        execution["artifacts"][0]["path"]
            .as_str()
            .expect("artifact path"),
    );
    assert_eq!(
        execution["outputs"]["image_path"],
        xdg_output.display().to_string()
    );
    assert_eq!(
        artifact,
        Path::new(".lightflow/artifacts/flux/lightflow_flux_preview_text_to_image-42-001.png")
    );
    assert_eq!(
        execution["outputs"]["image"]["path"],
        artifact.to_str().unwrap()
    );
    assert_eq!(fs::read(&xdg_output)?, fs::read(root.join(artifact))?);
    assert!(fs::read(&xdg_output)?.starts_with(b"\x89PNG\r\n\x1a\n"));

    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(xdg_pictures);
    Ok(())
}

fn assert_runner_execution(execution: &serde_json::Value, task: &str, capability: &str) {
    assert_eq!(execution["runtime"]["executor_id"], "runner.v1");
    assert_eq!(
        execution["runtime"]["replay_fingerprint"]["engine"],
        "runner.v1"
    );
    assert!(
        execution["runtime"]["replay_fingerprint"]["runner"]["implementation"]
            .as_str()
            .is_some_and(|identity| !identity.is_empty())
    );
    assert!(
        execution["runtime"]["replay_fingerprint"]["runner"]["resolved_models"]["flux_model"]
            ["sha256"]
            .as_str()
            .is_some_and(|sha256| sha256.len() == 64)
    );
    assert_eq!(execution["artifacts"][0]["metadata"]["task"], task);
    assert_eq!(
        execution["artifacts"][0]["metadata"]["capability"],
        capability
    );
}

fn write_model_lock(
    root: &Path,
    flux: &Path,
    llm: &Path,
    vae: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut models = serde_json::Map::new();
    for workflow in [
        "lightflow.flux_text_to_image",
        "lightflow.flux_image_edit",
        "lightflow.flux_inpaint",
    ] {
        for (requirement, variant, path) in [
            ("flux_model", "flux2-klein-q4-k-m", flux),
            ("llm_model", "qwen3-8b-q4-k-m", llm),
            ("vae_model", "flux2-vae", vae),
        ] {
            models.insert(
                format!("{workflow}::{requirement}"),
                serde_json::json!({
                    "variant_id": variant,
                    "local_paths": [path],
                }),
            );
        }
    }
    fs::write(
        root.join("lfw.lock"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "version": 2,
            "models": models,
        }))?,
    )?;
    Ok(())
}

fn write_backend_fixture(
    runner: &Path,
    log: &Path,
    fixture: &Path,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(
        runner,
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
  esac
  shift || true
done
test -n "$out"
mkdir -p "$(dirname "$out")"
cp {fixture:?} "$out"
"#,
            log = log,
            fixture = fixture,
        ),
    )?;
    let mut permissions = fs::metadata(runner)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(runner, permissions)?;
    Ok(())
}

const PNG_FIXTURE: &[u8] = &[
    0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0x00, 0x00, 0x00, 0x0d, 0x49, 0x48, 0x44, 0x52,
    0x00, 0x00, 0x00, 0x01, 0x00, 0x00, 0x00, 0x01, 0x08, 0x04, 0x00, 0x00, 0x00, 0xb5, 0x1c, 0x0c,
    0x02, 0x00, 0x00, 0x00, 0x0b, 0x49, 0x44, 0x41, 0x54, 0x78, 0xda, 0x63, 0xfc, 0xff, 0x1f, 0x00,
    0x03, 0x03, 0x02, 0x00, 0xef, 0xbf, 0xa7, 0xdb, 0x00, 0x00, 0x00, 0x00, 0x49, 0x45, 0x4e, 0x44,
    0xae, 0x42, 0x60, 0x82,
];
