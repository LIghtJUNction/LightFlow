mod support;

use lightflow::api::ApiService;
use std::path::Path;
use support::*;

#[test]
fn repository_text_to_image_declares_runtime_and_gguf_model()
-> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let service = ApiService::new(root);
    let workflow = service.get_workflow("lightflow.text_to_image")?;

    assert!(workflow.category.is_none());
    assert_eq!(workflow.runtimes[0].capability, "lightflow.image.generate");
    assert_eq!(
        workflow.runtimes[0].engine.as_deref(),
        Some("builtin.preview.v1")
    );
    assert_eq!(workflow.models[0].capability, "text-to-image");
    assert_eq!(workflow.models[0].variants[0].format, "gguf");
    let prompt = workflow
        .inputs
        .iter()
        .find(|port| port.name == "prompt")
        .expect("prompt input exists");
    assert_eq!(prompt.required, Some(true));
    assert_eq!(prompt.widget.as_deref(), Some("prompt"));
    let width = workflow
        .inputs
        .iter()
        .find(|port| port.name == "width")
        .expect("width input exists");
    assert_eq!(width.default, Some(serde_json::json!(512)));
    assert_eq!(width.min, Some(64.0));
    assert_eq!(width.max, Some(2048.0));
    assert_eq!(width.step, Some(8.0));
    let model = workflow
        .inputs
        .iter()
        .find(|port| port.name == "model")
        .expect("model input exists");
    assert_eq!(model.widget.as_deref(), Some("model_select"));
    assert_eq!(model.model_requirement.as_deref(), Some("image_model"));
    assert_eq!(
        model.enum_values,
        vec![
            serde_json::json!("sdxl-gguf-q4"),
            serde_json::json!("sdxl-safetensors")
        ]
    );
    let image = workflow
        .outputs
        .iter()
        .find(|port| port.name == "image")
        .expect("image output exists");
    assert_eq!(image.artifact_kind.as_deref(), Some("image"));

    let help = lfw(root, ["help", "lightflow.text_to_image"])?;
    assert_eq!(help["ports"]["inputs"][0]["name"], "prompt");
    assert_eq!(help["ports"]["inputs"][0]["required"], true);
    assert_eq!(help["ports"]["inputs"][0]["widget"], "prompt");
    assert_eq!(help["ports"]["inputs"][2]["default"], 512);
    assert_eq!(help["ports"]["inputs"][2]["min"], 64.0);
    assert_eq!(
        help["ports"]["inputs"][6]["model_requirement"],
        "image_model"
    );
    assert_eq!(
        help["usage"]["inputs_json_shape"]["width"],
        serde_json::json!(512)
    );

    Ok(())
}
