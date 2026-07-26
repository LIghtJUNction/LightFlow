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
    let runner = text_to_image["runtimes"]
        .as_array()
        .expect("runtimes")
        .iter()
        .flat_map(|runtime| {
            runtime["executors"]
                .as_array()
                .expect("runtime executors")
                .iter()
        })
        .find(|executor| executor["id"] == "runner.v1")
        .expect("runner");
    assert_eq!(runner["status"], "runner");
    assert_eq!(runner["available"], true);
    assert_eq!(runner["data_policy"], "artifact_handles");
    assert_eq!(runner["plans_models"], false);
    assert_eq!(runner["status_reason"], "available in this build");
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
    assert_eq!(plan["body"]["runtime"]["executor_id"], "runner.v1");
    assert_eq!(plan["body"]["runtime"]["data_policy"], "artifact_handles");
    assert_eq!(
        plan["body"]["runtime"]["models"]
            .as_array()
            .expect("planned models")
            .len(),
        0
    );

    for workflow_id in [
        "lightflow.text_prompt",
        "lightflow.text_result",
        "lightflow.text_concat",
        "lightflow.text_template",
        "lightflow.text_regex",
        "lightflow.json_extract",
        "lightflow.control_if",
        "lightflow.control_switch",
        "lightflow.control_merge",
        "lightflow.control_split",
        "lightflow.model_select",
        "lightflow.model_lock_check",
        "lightflow.llm_generate",
        "lightflow.llm_classify",
        "lightflow.llm_structured_output",
        "lightflow.image_load",
        "lightflow.image_save",
        "lightflow.image_resize",
        "lightflow.image_crop",
        "lightflow.image_upscale",
        "lightflow.image_invert",
        "lightflow.mask_compose",
        "lightflow.text_to_image",
        "lightflow.image_edit",
        "lightflow.image_inpaint",
    ] {
        let node = body["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .find(|node| node["id"] == workflow_id)
            .unwrap_or_else(|| panic!("missing std node {workflow_id}"));
        let runtimes = node["runtimes"].as_array().expect("std runtimes");
        assert_eq!(runtimes.len(), 1, "{workflow_id}");
        assert_eq!(runtimes[0]["engine"], "runner.v1", "{workflow_id}");
        assert_eq!(runtimes[0]["available"], true, "{workflow_id}");
        assert_eq!(
            runtimes[0]["executors"][0]["id"], "runner.v1",
            "{workflow_id}"
        );

        let plan = request_json(&app, &format!("/workflows/{workflow_id}/plan")).await;
        assert_eq!(plan["status"], 200, "{workflow_id}: {plan}");
        assert_eq!(
            plan["body"]["runtime"]["executor_id"], "runner.v1",
            "{workflow_id}"
        );
    }
}
