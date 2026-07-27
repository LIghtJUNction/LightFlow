mod support;

use std::fs;
use std::path::Path;
use std::process::Command;
use support::*;

#[test]
fn lfx_runs_text_to_image_and_writes_png_artifact() -> Result<(), Box<dyn std::error::Error>> {
    let repo_root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let temp_suffix = unique_temp_root()
        .file_name()
        .expect("temporary root has a file name")
        .to_owned();
    let relative_root = Path::new(".lightflow")
        .join("test-artifacts")
        .join(temp_suffix);
    let root = repo_root.join(&relative_root);
    fs::create_dir_all(&root)?;
    let relative_output = relative_root.join("image.png");
    let output_path = repo_root.join(&relative_output);

    let execution = lfx(
        repo_root,
        [
            "lightflow.text_to_image",
            "--prompt",
            "a quiet lake",
            "--input",
            "width=96",
            "--input",
            "height=64",
            "--output",
            relative_output.to_str().unwrap(),
        ],
    )?;

    assert_eq!(execution["workflow_id"], "lightflow.text_to_image");
    assert_eq!(execution["runtime"]["executor_id"], "runner.v1");
    assert_eq!(execution["runtime"]["data_policy"], "artifact_handles");
    assert_eq!(
        execution["runtime"]["declared"][0]["capability"],
        "lightflow.image.generate"
    );
    assert_eq!(
        execution["outputs"]["image_path"],
        relative_output.to_str().unwrap()
    );
    assert_eq!(execution["artifacts"][0]["kind"], "image");
    assert_eq!(execution["artifacts"][0]["mime_type"], "image/png");
    assert_eq!(execution["artifacts"][0]["metadata"]["width"], 96);
    assert_eq!(execution["artifacts"][0]["metadata"]["height"], 64);

    let bytes = fs::read(&output_path)?;
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn text_to_image_defaults_output_to_project_artifacts() -> Result<(), Box<dyn std::error::Error>> {
    let home = unique_temp_root();
    fs::create_dir_all(home.join(".config"))?;
    fs::write(
        home.join(".config/user-dirs.dirs"),
        r#"XDG_PICTURES_DIR="$HOME/Images"
"#,
    )?;

    let output = Command::new(env!("CARGO_BIN_EXE_lfx"))
        .args([
            "lightflow.text_to_image",
            "--prompt",
            "a quiet lake",
            "--input",
            "width=96",
            "--input",
            "height=64",
            "--input",
            "seed=7",
        ])
        .current_dir(Path::new(env!("CARGO_MANIFEST_DIR")))
        .env("HOME", &home)
        .env("SHELL", "/bin/zsh")
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("XDG_DATA_HOME", home.join(".local/share"))
        .env_remove("LFW_PATH")
        .output()?;
    assert!(
        output.status.success(),
        "lfx failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let execution: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    let expected_path = ".lightflow/artifacts/preview.png";
    assert_eq!(execution["outputs"]["image_path"], expected_path);
    let bytes = fs::read(Path::new(env!("CARGO_MANIFEST_DIR")).join(expected_path))?;
    assert!(bytes.starts_with(b"\x89PNG\r\n\x1a\n"));

    let _ = fs::remove_dir_all(home);
    Ok(())
}
