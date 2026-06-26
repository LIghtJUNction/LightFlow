use super::request_json;
use crate::server::{ApiService, router};

#[tokio::test]
async fn node_directory_endpoints_return_editor_contracts() {
    let service = ApiService::new(std::env::current_dir().expect("current dir"));
    let app = router(service);

    let nodes = request_json(&app, "/nodes").await;
    assert_eq!(nodes["status"], 200);
    let body = &nodes["body"];
    let text_to_image = body["nodes"]
        .as_array()
        .expect("nodes")
        .iter()
        .find(|node| node["id"] == "lightflow.text_to_image")
        .expect("text_to_image node");
    assert_eq!(text_to_image["kind"], "leaf");
    assert_eq!(text_to_image["inputs"][0]["widget"], "prompt");
    assert_eq!(
        text_to_image["runtimes"][0]["capability"],
        "lightflow.image.generate"
    );
    let preview_executor = text_to_image["runtimes"][0]["executors"]
        .as_array()
        .expect("executors")
        .iter()
        .find(|executor| executor["id"] == "builtin.preview.v1")
        .expect("preview executor");
    assert_eq!(preview_executor["status"], "preview");
    assert_eq!(preview_executor["available"], true);
    assert_eq!(preview_executor["data_policy"], "artifact_handles");
    assert_eq!(preview_executor["plans_models"], true);
    assert_eq!(preview_executor["status_reason"], "available in this build");
    assert_eq!(text_to_image["validation"]["valid"], true);

    let node = request_json(&app, "/nodes/lightflow.text_to_image").await;
    assert_eq!(node["status"], 200);
    assert_eq!(node["body"]["id"], "lightflow.text_to_image");
    assert_eq!(node["body"]["models"][0]["id"], "image_model");

    let executors = request_json(&app, "/executors").await;
    assert_eq!(executors["status"], 200);
    let native = executors["body"]["executors"]
        .as_array()
        .expect("executors")
        .iter()
        .find(|executor| executor["id"] == "diffusion-rs.native.v1")
        .expect("native executor");
    assert_eq!(native["status"], "native");
    assert_eq!(native["data_policy"], "device_resident_preferred");
    assert_eq!(native["plans_models"], true);

    let models = request_json(&app, "/models").await;
    assert_eq!(models["status"], 200);
    let image_model = models["body"]["models"]
        .as_array()
        .expect("models")
        .iter()
        .find(|model| {
            model["workflow_id"] == "lightflow.text_to_image"
                && model["requirement"]["id"] == "image_model"
        })
        .expect("image model");
    assert!(image_model["bindings"].as_array().expect("bindings").len() >= 2);

    let plan = request_json(&app, "/workflows/lightflow.text_to_image/plan").await;
    assert_eq!(plan["status"], 200);
    assert_eq!(plan["body"]["kind"], "leaf");
    assert_eq!(plan["body"]["runtime"]["executor_id"], "builtin.preview.v1");
    assert_eq!(plan["body"]["runtime"]["data_policy"], "artifact_handles");
    assert_eq!(
        plan["body"]["runtime"]["models"][0]["requirement_id"],
        "image_model"
    );
}
