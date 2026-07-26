mod support;

use std::fs;
use std::path::Path;
use support::*;

#[test]
fn lfw_runs_text_to_image_through_invert_pipeline() -> Result<(), Box<dyn std::error::Error>> {
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
    let relative_generated = relative_root.join("cat.png");
    let relative_inverted = relative_root.join("cat-inverted.png");
    let generated_path = repo_root.join(&relative_generated);
    let inverted_path = repo_root.join(&relative_inverted);

    let execution = lfw(
        repo_root,
        [
            "run",
            "lightflow.text_to_image",
            "--prompt",
            "a small cat photo",
            "--input",
            "width=64",
            "--input",
            "height=64",
            "--output",
            relative_generated.to_str().unwrap(),
            "|",
            "lightflow.image_invert",
            "--output",
            relative_inverted.to_str().unwrap(),
        ],
    )?;

    assert_eq!(execution["pipeline"], true);
    assert_eq!(
        execution["outputs"]["image_path"],
        relative_inverted.to_str().unwrap()
    );
    assert_eq!(
        execution["stages"][1]["runtime"]["executor_id"],
        "runner.v1"
    );
    let generated = fs::read(&generated_path)?;
    let inverted = fs::read(&inverted_path)?;
    assert!(generated.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert!(inverted.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert_ne!(generated, inverted);

    let _ = fs::remove_dir_all(root);
    Ok(())
}
