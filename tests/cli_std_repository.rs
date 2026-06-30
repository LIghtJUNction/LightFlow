mod support;

use lightflow::api::ApiService;
use std::fs;
use std::path::Path;
use support::*;

#[test]
fn repository_std_workflow_is_library_only_and_abstract() -> Result<(), Box<dyn std::error::Error>>
{
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let service = ApiService::new(root);
    let workflow = service.get_workflow("lightflow.std")?;

    assert_eq!(workflow.id, "lightflow.std");
    assert_eq!(workflow.version, "0.1.0");
    assert_eq!(workflow.name, "LightFlow Std Identity");
    assert_eq!(workflow.inputs.len(), 1);
    assert_eq!(workflow.outputs.len(), 1);
    assert!(workflow.dependencies.is_empty());
    assert!(workflow.nodes.is_empty());
    assert!(workflow.edges.is_empty());

    assert_eq!(workflow.category.as_deref(), Some("std"));
    let crate_dir = root.join("projects/lightflow-std/workflows/std/std");
    assert!(crate_dir.join("src/lib.rs").exists());
    assert!(!crate_dir.join("src/main.rs").exists());

    let manifest = fs::read_to_string(crate_dir.join("Cargo.toml"))?;
    assert!(manifest.contains("name = \"lightflow-std\""));
    assert!(manifest.contains("lightflow = { workspace = true }"));
    assert!(manifest.contains("repository = \"https://github.com/lightjunction/lightflow-std\""));
    assert!(!manifest.contains("publish = false"));

    Ok(())
}

#[test]
fn repository_std_project_workflows_are_discovered_by_default()
-> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let service = ApiService::new(root);

    assert_eq!(
        service.get_workflow("lightflow.std")?.category.as_deref(),
        Some("std")
    );
    assert!(service.get_workflow("lightflow.text.template").is_ok());

    Ok(())
}

#[test]
fn sibling_project_workflows_are_discovered_from_explicit_paths()
-> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lfw_path = format!(
        "{}:{}",
        root.join("projects/lightflow-flux").display(),
        root.join("projects/lightflow-rig").display()
    );
    let output = lfw_with_env_values(root, ["list", "--brief"], [("LFW_PATH", lfw_path.as_str())])?;
    let workflow_ids = output["workflows"]
        .as_array()
        .expect("workflow list")
        .iter()
        .filter_map(|workflow| workflow["id"].as_str())
        .collect::<Vec<_>>();

    assert!(workflow_ids.contains(&"lightflow.flux.text_to_image"));
    assert!(workflow_ids.contains(&"lightflow.rig.llm"));

    Ok(())
}

#[test]
fn repository_text_plan_dogfoods_std_workflow() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let service = ApiService::new(root);

    let workflow = service.get_workflow("lightflow.text_plan")?;
    assert_eq!(
        workflow
            .dependencies
            .iter()
            .map(|dependency| (
                dependency.workflow_id.as_str(),
                dependency.version.as_deref()
            ))
            .collect::<Vec<_>>(),
        vec![
            ("lightflow.std", Some("0.1.0")),
            ("lightflow.text_prompt", Some("0.1.0")),
            ("lightflow.text_result", Some("0.1.0")),
        ]
    );
    assert!(
        workflow
            .nodes
            .iter()
            .any(|node| node.id == "identity" && node.workflow_id == "lightflow.std")
    );

    let detail = lfw(root, ["ls", "--detail"])?;
    let text_plan = detail["workflows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|workflow| workflow["id"] == "lightflow.text_plan")
        .expect("detailed list includes lightflow.text_plan");
    assert_eq!(text_plan["nodes"][0]["workflow_id"], "lightflow.std");

    let deps = lfw(root, ["deps", "lightflow.text_plan"])?;
    assert_eq!(deps["complete"], true);
    assert_eq!(
        deps["workflows"],
        serde_json::json!([
            "lightflow.std",
            "lightflow.text_plan",
            "lightflow.text_prompt",
            "lightflow.text_result"
        ])
    );
    assert_eq!(
        deps["workflow_order"],
        serde_json::json!([
            "lightflow.std",
            "lightflow.text_prompt",
            "lightflow.text_result",
            "lightflow.text_plan"
        ])
    );

    Ok(())
}
