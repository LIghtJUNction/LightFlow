mod standard_node_support;
mod support;

use lightflow::api::ApiService;
use standard_node_support::png_dimensions;
use std::fs;
use std::path::Path;
use support::*;

#[test]
fn repository_standard_model_diffusion_and_llm_nodes_are_runnable()
-> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let service = ApiService::new(root);
    for (workflow_id, capability) in [
        ("lightflow.model.select", "lightflow.model.select"),
        ("lightflow.model.lock_check", "lightflow.model.lock.check"),
        ("lightflow.image.upscale", "lightflow.image.upscale"),
        ("lightflow.mask.compose", "lightflow.mask.compose"),
        ("lightflow.image.edit", "lightflow.image.edit"),
        ("lightflow.image.inpaint", "lightflow.image.inpaint"),
        ("lightflow.llm.generate", "lightflow.llm.generate"),
        ("lightflow.llm.classify", "lightflow.llm.classify"),
        (
            "lightflow.llm.structured_output",
            "lightflow.llm.structured_output",
        ),
    ] {
        let workflow = service.get_workflow(workflow_id)?;
        assert_eq!(workflow.category.as_deref(), Some("std"));
        assert_eq!(workflow.runtimes[0].capability, capability);
    }

    let selected = lfw(
        root,
        [
            "run",
            "lightflow.model.select",
            "-i",
            "requirement_id=image_model",
            "-i",
            "preferred=gguf",
            "-i",
            "variants=[{\"id\":\"q4\",\"format\":\"gguf\"},{\"id\":\"fp16\",\"format\":\"safetensors\"}]",
        ],
    )?;
    assert_eq!(selected["outputs"]["variant_id"], "q4");
    assert_eq!(selected["outputs"]["model"]["format"], "gguf");

    let lock = lfw(
        root,
        [
            "run",
            "lightflow.model.lock_check",
            "-i",
            "workflow_id=lightflow.text_to_image",
            "-i",
            "requirement_id=image_model",
        ],
    )?;
    assert_eq!(lock["outputs"]["locked"], false);
    assert_eq!(lock["outputs"]["exists"], false);

    let temp = unique_temp_root();
    fs::create_dir_all(&temp)?;
    let source = temp.join("source.png");
    let mask_a = temp.join("mask-a.png");
    let mask_b = temp.join("mask-b.png");
    let mask_composed = temp.join("mask-composed.png");
    let edited = temp.join("edited.png");
    let inpainted = temp.join("inpainted.png");
    let upscaled = temp.join("upscaled.png");
    lfw(
        root,
        [
            "run",
            "lightflow.text_to_image",
            "--prompt",
            "upscale node test",
            "-i",
            "width=32",
            "-i",
            "height=32",
            "--output",
            source.to_str().unwrap(),
        ],
    )?;
    lfw(
        root,
        [
            "run",
            "lightflow.text_to_image",
            "--prompt",
            "mask a",
            "-i",
            "width=32",
            "-i",
            "height=32",
            "--output",
            mask_a.to_str().unwrap(),
        ],
    )?;
    lfw(
        root,
        [
            "run",
            "lightflow.text_to_image",
            "--prompt",
            "mask b",
            "-i",
            "width=32",
            "-i",
            "height=32",
            "--output",
            mask_b.to_str().unwrap(),
        ],
    )?;
    let upscale = lfw(
        root,
        [
            "run",
            "lightflow.image.upscale",
            "-i",
            &format!("image_path={}", source.display()),
            "-i",
            "scale=3",
            "-i",
            &format!("output_path={}", upscaled.display()),
        ],
    )?;
    assert_eq!(upscale["outputs"]["image_path"], upscaled.to_str().unwrap());
    assert_eq!(png_dimensions(&upscaled)?, (192, 192));

    let compose = lfw(
        root,
        [
            "run",
            "lightflow.mask.compose",
            "-i",
            &format!("mask_a_path={}", mask_a.display()),
            "-i",
            &format!("mask_b_path={}", mask_b.display()),
            "-i",
            "mode=max",
            "-i",
            &format!("output_path={}", mask_composed.display()),
        ],
    )?;
    assert_eq!(
        compose["outputs"]["mask_path"],
        mask_composed.to_str().unwrap()
    );
    assert_eq!(compose["artifacts"][0]["kind"], "mask");
    assert_eq!(png_dimensions(&mask_composed)?, (64, 64));

    let edit = lfw(
        root,
        [
            "run",
            "lightflow.image.edit",
            "-i",
            &format!("image_path={}", source.display()),
            "-i",
            "prompt=warmer lighting",
            "-i",
            &format!("output_path={}", edited.display()),
        ],
    )?;
    assert_eq!(edit["outputs"]["image_path"], edited.to_str().unwrap());
    assert_eq!(
        edit["artifacts"][0]["metadata"]["engine"],
        "builtin.preview.edit.v1"
    );
    assert_eq!(png_dimensions(&edited)?, (64, 64));

    let inpaint = lfw(
        root,
        [
            "run",
            "lightflow.image.inpaint",
            "-i",
            &format!("image_path={}", source.display()),
            "-i",
            &format!("mask_path={}", mask_composed.display()),
            "-i",
            "prompt=repair masked region",
            "-i",
            &format!("output_path={}", inpainted.display()),
        ],
    )?;
    assert_eq!(
        inpaint["outputs"]["image_path"],
        inpainted.to_str().unwrap()
    );
    assert_eq!(
        inpaint["artifacts"][0]["metadata"]["engine"],
        "builtin.preview.inpaint.v1"
    );
    assert_eq!(png_dimensions(&inpainted)?, (64, 64));

    let generated = lfw(
        root,
        [
            "run",
            "lightflow.llm.generate",
            "-i",
            "prompt=hello",
            "-i",
            "model=mock-small",
        ],
    )?;
    assert_eq!(generated["outputs"]["text"], "mock:mock-small:hello");

    let classified = lfw(
        root,
        [
            "run",
            "lightflow.llm.classify",
            "-i",
            "text=urgent billing issue",
            "-i",
            "labels=[\"billing\",\"support\"]",
        ],
    )?;
    assert_eq!(classified["outputs"]["label"], "billing");

    let structured = lfw(
        root,
        [
            "run",
            "lightflow.llm.structured_output",
            "-i",
            "text={\"intent\":\"search\"}",
        ],
    )?;
    assert_eq!(structured["outputs"]["object"]["intent"], "search");

    let _ = fs::remove_dir_all(temp);
    Ok(())
}

#[test]
fn repository_standard_model_diffusion_and_llm_nodes_pass_node_conformance()
-> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for workflow_id in [
        "lightflow.model.select",
        "lightflow.model.lock_check",
        "lightflow.image.upscale",
        "lightflow.mask.compose",
        "lightflow.image.edit",
        "lightflow.image.inpaint",
        "lightflow.llm.generate",
        "lightflow.llm.classify",
        "lightflow.llm.structured_output",
    ] {
        let report = lfw(root, ["node", "test", workflow_id])?;
        assert_eq!(report["valid"], true, "{workflow_id}");
    }
    Ok(())
}
