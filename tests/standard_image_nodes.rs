mod standard_node_support;
mod support;

use lightflow::api::ApiService;
use standard_node_support::png_dimensions;
use std::fs;
use std::path::Path;
use support::*;

#[test]
fn repository_standard_image_nodes_are_discoverable_and_runnable()
-> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let service = ApiService::new(root);
    for (workflow_id, capability) in [
        ("lightflow.image.load", "lightflow.image.load"),
        ("lightflow.image.save", "lightflow.image.save"),
        ("lightflow.image.resize", "lightflow.image.resize"),
        ("lightflow.image.crop", "lightflow.image.crop"),
    ] {
        let workflow = service.get_workflow(workflow_id)?;
        assert_eq!(workflow.category.as_deref(), Some("std"));
        assert_eq!(workflow.runtimes[0].capability, capability);
        assert!(workflow.outputs.iter().any(|port| {
            port.name == "image" && port.artifact_kind.as_deref() == Some("image")
        }));
    }

    let temp = unique_temp_root();
    fs::create_dir_all(&temp)?;
    let source = temp.join("source.png");
    let saved = temp.join("saved.png");
    let resized = temp.join("resized.png");
    let cropped = temp.join("cropped.png");

    lfw(
        root,
        [
            "run",
            "lightflow.text_to_image",
            "--prompt",
            "standard image node test",
            "-i",
            "width=64",
            "-i",
            "height=32",
            "--output",
            source.to_str().unwrap(),
        ],
    )?;
    assert_eq!(png_dimensions(&source)?, (64, 64));

    let loaded = lfw(
        root,
        [
            "run",
            "lightflow.image.load",
            "-i",
            &format!("image_path={}", source.display()),
        ],
    )?;
    assert_eq!(loaded["outputs"]["image_path"], source.to_str().unwrap());
    assert_eq!(loaded["artifacts"][0]["metadata"]["width"], 64);

    let save = lfw(
        root,
        [
            "run",
            "lightflow.image.save",
            "-i",
            &format!("image_path={}", source.display()),
            "-i",
            &format!("output_path={}", saved.display()),
        ],
    )?;
    assert_eq!(save["outputs"]["image_path"], saved.to_str().unwrap());
    assert_eq!(fs::read(&source)?, fs::read(&saved)?);

    let resize = lfw(
        root,
        [
            "run",
            "lightflow.image.resize",
            "-i",
            &format!("image_path={}", source.display()),
            "-i",
            "width=16",
            "-i",
            "height=8",
            "-i",
            &format!("output_path={}", resized.display()),
        ],
    )?;
    assert_eq!(resize["outputs"]["image_path"], resized.to_str().unwrap());
    assert_eq!(png_dimensions(&resized)?, (16, 8));

    let crop = lfw(
        root,
        [
            "run",
            "lightflow.image.crop",
            "-i",
            &format!("image_path={}", source.display()),
            "-i",
            "x=4",
            "-i",
            "y=2",
            "-i",
            "width=20",
            "-i",
            "height=10",
            "-i",
            &format!("output_path={}", cropped.display()),
        ],
    )?;
    assert_eq!(crop["outputs"]["image_path"], cropped.to_str().unwrap());
    assert_eq!(png_dimensions(&cropped)?, (20, 10));

    let _ = fs::remove_dir_all(temp);
    Ok(())
}

#[test]
fn repository_standard_image_nodes_pass_node_conformance() -> Result<(), Box<dyn std::error::Error>>
{
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for workflow_id in [
        "lightflow.image.load",
        "lightflow.image.save",
        "lightflow.image.resize",
        "lightflow.image.crop",
    ] {
        let report = lfw(root, ["node", "test", workflow_id])?;
        assert_eq!(report["valid"], true, "{workflow_id}");
    }
    Ok(())
}
