mod support;

use std::fs;
use support::*;

#[test]
fn sync_applies_declared_workflow_module_dependencies() -> Result<(), Box<dyn std::error::Error>> {
    let base = unique_temp_root();
    let project = base.join("project");
    let text_prompt_dep = base.join("lightflow-text-prompt");
    fs::create_dir_all(&project)?;
    write_external_text_prompt_crate(&text_prompt_dep)?;

    fs::write(
        project.join("Cargo.toml"),
        format!(
            r#"[workspace]
resolver = "3"
members = [".lightflow/workflows/*"]

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
    workflow!()
        .name("Image Prompt")
        .input("positive", "text")
        .input("negative", "text")
        .output("prompt", "json")
        .depends_on_path(
            "lightflow.text_prompt",
            "0.1.0",
            "lightflow-text-prompt",
            "../lightflow-text-prompt"
        )
        .node("passthrough", "lightflow.text_prompt")
        .build()
}
"#,
    )?;

    let dry_run = lfw(&project, ["sync", "lightflow.image_prompt"])?;
    assert_eq!(dry_run["dry_run"], true);
    assert_eq!(
        dry_run["module_dependencies"]["installs"][0]["dependency"],
        "lightflow-text-prompt"
    );
    assert_eq!(
        dry_run["module_dependencies"]["installs"][0]["source"]["path"],
        "../lightflow-text-prompt"
    );
    let manifest = fs::read_to_string(project.join("Cargo.toml"))?;
    assert!(!manifest.contains("lightflow-text-prompt = { path = \"../lightflow-text-prompt\" }"));

    let applied = lfw(&project, ["sync", "lightflow.image_prompt", "--apply"])?;
    assert_eq!(applied["dry_run"], false);
    assert_eq!(
        applied["executed"][0]["dependency"],
        serde_json::json!("lightflow-text-prompt")
    );
    let manifest = fs::read_to_string(project.join("Cargo.toml"))?;
    assert!(manifest.contains(
        "lightflow-text-prompt = { version = \"0.1.0\", path = \"../lightflow-text-prompt\" }"
    ));

    let list = lfw(&project, ["list"])?;
    let ids = list["workflows"]
        .as_array()
        .expect("workflows list returns an array")
        .iter()
        .map(|workflow| workflow["id"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(ids, vec!["lightflow.image_prompt", "lightflow.text_prompt"]);

    let _ = fs::remove_dir_all(base);
    Ok(())
}
