mod support;

use std::fs;
use std::path::Path;
use support::*;

#[test]
fn lfw_runs_text_to_image_through_invert_pipeline() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    let generated_path = root.join("out/cat.png");
    let inverted_path = root.join("out/cat-inverted.png");

    let execution = lfw(
        Path::new(env!("CARGO_MANIFEST_DIR")),
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
            generated_path.to_str().unwrap(),
            "|",
            "lightflow.image_invert",
            "--output",
            inverted_path.to_str().unwrap(),
        ],
    )?;

    assert_eq!(execution["pipeline"], true);
    assert_eq!(
        execution["outputs"]["image_path"],
        inverted_path.to_str().unwrap()
    );
    assert_eq!(
        execution["stages"][1]["artifacts"][0]["metadata"]["capability"],
        "lightflow.image.invert"
    );
    let generated = fs::read(&generated_path)?;
    let inverted = fs::read(&inverted_path)?;
    assert!(generated.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert!(inverted.starts_with(b"\x89PNG\r\n\x1a\n"));
    assert_ne!(generated, inverted);

    let _ = fs::remove_dir_all(root);
    Ok(())
}
