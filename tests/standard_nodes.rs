mod support;

use lightflow::api::ApiService;
use std::fs;
use std::io::BufReader;
use std::path::Path;
use support::*;

#[test]
fn repository_standard_text_nodes_are_discoverable_and_runnable()
-> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let service = ApiService::new(root);

    let concat = service.get_workflow("lightflow.text.concat")?;
    assert_eq!(concat.category.as_deref(), Some("std"));
    assert_eq!(concat.runtimes[0].capability, "lightflow.text.concat");
    assert_eq!(
        concat
            .inputs
            .iter()
            .find(|port| port.name == "separator")
            .and_then(|port| port.default.clone()),
        Some(serde_json::json!(""))
    );

    let template = service.get_workflow("lightflow.text.template")?;
    assert_eq!(template.runtimes[0].capability, "lightflow.text.template");
    assert_eq!(template.inputs[0].widget.as_deref(), Some("textarea"));

    let extract = service.get_workflow("lightflow.json.extract")?;
    assert_eq!(extract.runtimes[0].capability, "lightflow.json.extract");
    assert_eq!(extract.outputs.len(), 3);

    let regex = service.get_workflow("lightflow.text.regex")?;
    assert_eq!(regex.runtimes[0].capability, "lightflow.text.regex");
    assert!(regex.outputs.iter().any(|port| port.name == "matched"));

    let nodes = service.list_nodes()?;
    assert!(nodes.nodes.iter().any(|node| {
        node.id == "lightflow.text.concat"
            && node
                .runtimes
                .iter()
                .any(|runtime| runtime.capability == "lightflow.text.concat" && runtime.available)
    }));

    let concat_run = lfw(
        root,
        [
            "run",
            "lightflow.text.concat",
            "-i",
            "a=hello",
            "-i",
            "b=world",
            "-i",
            "separator=-",
        ],
    )?;
    assert_eq!(concat_run["outputs"]["text"], "hello-world");

    let template_run = lfw(
        root,
        [
            "run",
            "lightflow.text.template",
            "-i",
            "template=Describe {{scene}} in {{style}} style",
            "-i",
            "vars={\"scene\":\"a quiet lake\",\"style\":\"ink\"}",
        ],
    )?;
    assert_eq!(
        template_run["outputs"]["text"],
        "Describe a quiet lake in ink style"
    );

    let extract_run = lfw(
        root,
        [
            "run",
            "lightflow.json.extract",
            "-i",
            "value={\"items\":[{\"title\":\"first\"}]}",
            "-i",
            "path=items.0.title",
        ],
    )?;
    assert_eq!(extract_run["outputs"]["value"], "first");
    assert_eq!(extract_run["outputs"]["text"], "first");
    assert_eq!(extract_run["outputs"]["found"], true);

    let regex_run = lfw(
        root,
        [
            "run",
            "lightflow.text.regex",
            "-i",
            "text=item-42",
            "-i",
            "pattern=(\\d+)",
            "-i",
            "replacement=id:$1",
        ],
    )?;
    assert_eq!(regex_run["outputs"]["text"], "item-id:42");
    assert_eq!(regex_run["outputs"]["matched"], true);
    assert_eq!(regex_run["outputs"]["match_count"], 1);
    assert_eq!(regex_run["outputs"]["first_match"], "42");

    Ok(())
}

#[test]
fn repository_standard_text_nodes_pass_node_conformance() -> Result<(), Box<dyn std::error::Error>>
{
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for workflow_id in [
        "lightflow.text.concat",
        "lightflow.text.template",
        "lightflow.json.extract",
        "lightflow.text.regex",
    ] {
        let report = lfw(root, ["node", "test", workflow_id])?;
        assert_eq!(report["valid"], true, "{workflow_id}");
    }
    Ok(())
}

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

#[test]
fn repository_standard_control_nodes_are_discoverable_and_runnable()
-> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let service = ApiService::new(root);
    for (workflow_id, capability) in [
        ("lightflow.control.if", "lightflow.control.if"),
        ("lightflow.control.switch", "lightflow.control.switch"),
        ("lightflow.control.merge", "lightflow.control.merge"),
        ("lightflow.control.split", "lightflow.control.split"),
    ] {
        let workflow = service.get_workflow(workflow_id)?;
        assert_eq!(workflow.category.as_deref(), Some("std"));
        assert_eq!(workflow.runtimes[0].capability, capability);
    }

    let if_run = lfw(
        root,
        [
            "run",
            "lightflow.control.if",
            "-i",
            "condition=true",
            "-i",
            "then_value=\"yes\"",
            "-i",
            "else_value=\"no\"",
        ],
    )?;
    assert_eq!(if_run["outputs"]["value"], "yes");
    assert_eq!(if_run["outputs"]["selected"], "then");

    let switch_run = lfw(
        root,
        [
            "run",
            "lightflow.control.switch",
            "-i",
            "selector=final",
            "-i",
            "cases={\"draft\":\"loose\",\"final\":\"polished\"}",
            "-i",
            "default=\"loose\"",
        ],
    )?;
    assert_eq!(switch_run["outputs"]["value"], "polished");
    assert_eq!(switch_run["outputs"]["selected"], "final");

    let merge_run = lfw(
        root,
        [
            "run",
            "lightflow.control.merge",
            "-i",
            "a={\"prompt\":\"cat\"}",
            "-i",
            "b={\"seed\":1}",
            "-i",
            "mode=object",
        ],
    )?;
    assert_eq!(merge_run["outputs"]["value"]["prompt"], "cat");
    assert_eq!(merge_run["outputs"]["value"]["seed"], 1);

    let split_run = lfw(
        root,
        [
            "run",
            "lightflow.control.split",
            "-i",
            "value=[\"first\",\"second\",\"third\"]",
        ],
    )?;
    assert_eq!(split_run["outputs"]["first"], "first");
    assert_eq!(
        split_run["outputs"]["rest"],
        serde_json::json!(["second", "third"])
    );
    assert_eq!(
        split_run["outputs"]["items"],
        serde_json::json!(["first", "second", "third"])
    );

    Ok(())
}

#[test]
fn repository_standard_control_nodes_pass_node_conformance()
-> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for workflow_id in [
        "lightflow.control.if",
        "lightflow.control.switch",
        "lightflow.control.merge",
        "lightflow.control.split",
    ] {
        let report = lfw(root, ["node", "test", workflow_id])?;
        assert_eq!(report["valid"], true, "{workflow_id}");
    }
    Ok(())
}

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

fn png_dimensions(path: &Path) -> Result<(u32, u32), Box<dyn std::error::Error>> {
    let file = fs::File::open(path)?;
    let decoder = png::Decoder::new(BufReader::new(file));
    let reader = decoder.read_info()?;
    let info = reader.info();
    Ok((info.width, info.height))
}
