mod comfyui_runtime_support;
mod support;

use std::fs;

use comfyui_runtime_support::{MockComfyUi, MockResponse};
use lightflow::api::ApiService;
use serde_json::{Value, json};
use support::{lfw, lfw_command, unique_temp_root};

const PNG: &[u8] = b"\x89PNG\r\n\x1a\nmock-png";

#[test]
fn lfw_new_comfyui_runtime_generates_api_workflow_contract()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    lfw(&root, ["init"])?;

    let created = lfw(
        &root,
        [
            "new",
            "comfy_run",
            "--category",
            "image",
            "--name",
            "Comfy Run",
            "--runtime",
            "lightflow.comfyui.workflow",
        ],
    )?;

    assert_eq!(created["runtime"], "lightflow.comfyui.workflow");
    let workflow_root = root.join(".lightflow/workflows/image/comfy_run");
    let source = fs::read_to_string(workflow_root.join("src/lib.rs"))?;
    assert!(source.contains(
        ".builtin_runtime(\"comfyui_runtime\", \"lightflow.comfyui.workflow\", \"comfyui.api.v1\")"
    ));
    assert!(source.contains(".input(\"workflow\", \"json\")"));
    assert!(source.contains(".input(\"uploads\", \"json\")"));
    assert!(source.contains(".output(\"artifacts\", \"json\")"));

    let skill =
        fs::read_to_string(workflow_root.join(".agent/skills/lightflow-comfy-run/SKILL.md"))?;
    assert!(skill.contains("ComfyUI Save (API Format)"));
    assert!(skill.contains("--inputs @comfy-run.json"));
    assert!(skill.contains("\"uploads\""));
    assert!(skill.contains("\"bind\""));
    assert!(skill.contains(
        "Shape only: replace `workflow` with a complete Save (API Format) export before running"
    ));
    assert!(skill.contains("node id must come from your complete exported graph"));
    assert!(skill.contains("--data-binary @comfy-http-request.json"));
    assert!(skill.contains("{\"inputs\": <complete run object from comfy-run.json>}"));
    assert!(!skill.to_ascii_lowercase().contains("text to image"));
    assert!(!skill.contains("KSampler"));
    assert!(!skill.contains("CLIPTextEncode"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn text_to_image_runs_through_prompt_history_and_download() -> Result<(), Box<dyn std::error::Error>>
{
    let root = generated_comfy_project()?;
    let server = MockComfyUi::start(vec![
        MockResponse::json(json!({"prompt_id":"prompt-1"})),
        MockResponse::json(json!({})),
        MockResponse::json(json!({
            "prompt-1": {
                "status": {"completed": true, "status_str": "success"},
                "outputs": {
                    "9": {
                        "images": [{"filename":"result.png","subfolder":"outputs/demo","type":"output"}],
                        "text": ["finished"]
                    }
                }
            }
        })),
        MockResponse::bytes("image/png", PNG),
    ])?;
    let inputs = json!({
        "workflow": {
            "3": {"class_type":"KSampler","inputs":{"seed":1,"steps":20}},
            "6": {"class_type":"CLIPTextEncode","inputs":{"text":"placeholder"}}
        },
        "node_inputs": {
            "3": {"seed":42},
            "6": {"text":"a quiet lake"}
        },
        "server_url": server.url,
        "poll_interval_ms": 1,
        "output_node_ids": ["9"]
    });
    fs::write(root.join("comfy-run.json"), serde_json::to_vec(&inputs)?)?;

    let execution = run_generated(&root, "run", "comfy-run.json")?;
    assert_eq!(execution["runtime"]["executor_id"], "comfyui.api.v1");
    assert_eq!(
        execution["runtime"]["replay_fingerprint"]["engine"],
        "comfyui.api.v1"
    );
    assert_eq!(execution["outputs"]["prompt_id"], "prompt-1");
    assert_eq!(
        execution["outputs"]["submitted_workflow"]["3"]["inputs"]["seed"],
        42
    );
    assert_eq!(
        execution["outputs"]["submitted_workflow"]["6"]["inputs"]["text"],
        "a quiet lake"
    );
    assert_eq!(
        execution["outputs"]["remote_outputs"]["9"]["text"][0],
        "finished"
    );
    assert_eq!(execution["artifacts"][0]["kind"], "image");
    assert_eq!(execution["artifacts"][0]["mime_type"], "image/png");
    let image_path = execution["outputs"]["image_path"]
        .as_str()
        .expect("image path");
    assert!(fs::read(image_path)?.starts_with(b"\x89PNG\r\n\x1a\n"));

    let requests = server.finish();
    assert_eq!(requests.len(), 4);
    assert_eq!(
        (requests[0].method.as_str(), requests[0].target.as_str()),
        ("POST", "/prompt")
    );
    let prompt = requests[0].json();
    assert_eq!(prompt["prompt"]["3"]["inputs"]["seed"], 42);
    assert_eq!(prompt["prompt"]["6"]["inputs"]["text"], "a quiet lake");
    assert_eq!(requests[1].target, "/history/prompt-1");
    assert_eq!(requests[2].target, "/history/prompt-1");
    assert!(requests[3].target.starts_with("/view?"));
    assert!(requests[3].target.contains("filename=result.png"));
    assert!(requests[3].target.contains("subfolder=outputs%2Fdemo"));

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn uploads_bind_images_and_download_all_filtered_file_outputs()
-> Result<(), Box<dyn std::error::Error>> {
    let root = generated_comfy_project()?;
    fs::write(root.join("input.png"), b"input-image-bytes")?;
    fs::write(root.join("mask.png"), b"mask-image-bytes")?;
    let server = MockComfyUi::start(vec![
        MockResponse::json(
            json!({"name":"server-image.png","subfolder":"accepted","type":"input"}),
        ),
        MockResponse::json(json!({"name":"server-mask.png","subfolder":"masks","type":"temp"})),
        MockResponse::json(json!({"prompt_id":"prompt-media"})),
        MockResponse::json(json!({
            "prompt-media": {
                "status": {"completed": true, "status_str": "success"},
                "outputs": {
                    "20": {
                        "images": [{"filename":"still.png","subfolder":"","type":"output"}],
                        "gifs": [{"filename":"motion.gif","subfolder":"anim","type":"output"}],
                        "video": {"nested":[{"filename":"clip.mp4","subfolder":"video","type":"output"}]},
                        "audio": [{"filename":"sound.wav","subfolder":"audio","type":"output"}],
                        "text": "non-file output"
                    },
                    "99": {"images":[{"filename":"ignored.png","subfolder":"","type":"output"}]}
                }
            }
        })),
        MockResponse::bytes("audio/wav", b"wave"),
        MockResponse::bytes("image/gif", b"gif"),
        MockResponse::bytes("image/png", PNG),
        MockResponse::bytes("video/mp4", b"video"),
    ])?;
    let inputs = json!({
        "workflow": {
            "10": {"class_type":"LoadImage","inputs":{"image":"old.png"}},
            "11": {"class_type":"LoadImageMask","inputs":{"image":"old-mask.png"}}
        },
        "uploads": [
            {"path":"input.png","bind":[{"node_id":"10","input":"image"}]},
            {"path":"mask.png","type":"temp","bind":[{"node_id":"11","input":"image"}]}
        ],
        "server_url": server.url,
        "poll_interval_ms": 1,
        "output_node_ids": ["20"]
    });
    fs::write(root.join("media-run.json"), serde_json::to_vec(&inputs)?)?;

    let execution = run_generated(&root, "run", "media-run.json")?;
    assert_eq!(
        execution["artifacts"].as_array().expect("artifacts").len(),
        4
    );
    let kinds = execution["artifacts"]
        .as_array()
        .expect("artifacts")
        .iter()
        .map(|artifact| artifact["kind"].as_str().expect("kind"))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        kinds,
        std::collections::BTreeSet::from(["audio", "gif", "image", "video"])
    );
    assert_eq!(
        execution["outputs"]["remote_outputs"]["20"]["text"],
        "non-file output"
    );
    assert_eq!(
        execution["outputs"]["upload_fingerprints"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    assert!(
        execution["artifacts"]
            .as_array()
            .unwrap()
            .iter()
            .all(|artifact| {
                artifact["metadata"]["remote_node"] == "20"
                    && artifact["metadata"]["prompt_id"] == "prompt-media"
            })
    );

    let requests = server.finish();
    assert_eq!(requests.len(), 8);
    assert_eq!(requests[0].target, "/upload/image");
    assert_eq!(requests[1].target, "/upload/image");
    assert!(
        requests[0]
            .header("content-type")
            .is_some_and(|value| value.starts_with("multipart/form-data; boundary="))
    );
    assert!(contains_bytes(&requests[0].body, b"input-image-bytes"));
    assert!(contains_bytes(&requests[1].body, b"mask-image-bytes"));
    let prompt = requests[2].json();
    assert_eq!(
        prompt["prompt"]["10"]["inputs"]["image"],
        "accepted/server-image.png"
    );
    assert_eq!(
        prompt["prompt"]["11"]["inputs"]["image"],
        "masks/server-mask.png"
    );
    assert!(
        requests[4..]
            .iter()
            .all(|request| !request.target.contains("ignored.png"))
    );
    assert!(
        requests[4..]
            .iter()
            .any(|request| request.target.contains("filename=clip.mp4"))
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn replay_fingerprint_ignores_prompt_id_and_detects_upload_content_drift()
-> Result<(), Box<dyn std::error::Error>> {
    let root = generated_comfy_project()?;
    fs::write(root.join("source.png"), b"first-upload")?;
    let server = MockComfyUi::start(
        ["prompt-original", "prompt-same", "prompt-changed"]
            .into_iter()
            .flat_map(|prompt_id| {
                let mut history = serde_json::Map::new();
                history.insert(
                    prompt_id.to_owned(),
                    json!({
                        "status": {"completed": true, "status_str": "success"},
                        "outputs": {"30":{"text":"complete without files"}}
                    }),
                );
                vec![
                    MockResponse::json(
                        json!({"name":"source.png","subfolder":"lightflow","type":"input"}),
                    ),
                    MockResponse::json(json!({"prompt_id":prompt_id})),
                    MockResponse::json(Value::Object(history)),
                ]
            })
            .collect(),
    )?;
    let inputs = json!({
        "workflow": {"10":{"class_type":"LoadImage","inputs":{"image":"old.png"}}},
        "node_inputs": {"10":{"custom_strength":0.75}},
        "uploads": [{"path":"source.png","bind":[{"node_id":"10","input":"image"}]}],
        "server_url": server.url,
        "poll_interval_ms": 1
    });
    fs::write(root.join("replay-run.json"), serde_json::to_vec(&inputs)?)?;

    let original = run_generated(&root, "run", "replay-run.json")?;
    let original_run_id = original["run_id"].as_str().expect("run id").to_owned();
    let manifest: Value = serde_json::from_slice(&fs::read(
        root.join(".lightflow/runs")
            .join(&original_run_id)
            .join("manifest.json"),
    )?)?;
    assert!(manifest["stages"][0]["execution"]["inputs"]["workflow"].is_object());
    assert!(manifest["stages"][0]["execution"]["inputs"]["node_inputs"].is_object());
    assert!(manifest["stages"][0]["execution"]["inputs"]["uploads"].is_array());

    let replay_same = run_replay(&root, &original_run_id)?;
    assert_eq!(replay_same["outputs"]["prompt_id"], "prompt-same");
    assert_eq!(replay_same["replay"]["runtime_changed"], false);
    assert_eq!(
        replay_same["replay"]["original_runtime"],
        replay_same["replay"]["replayed_runtime"]
    );

    fs::write(root.join("source.png"), b"second-upload")?;
    let replay_changed = run_replay(&root, &original_run_id)?;
    assert_eq!(replay_changed["replay"]["runtime_changed"], true);
    let original_hash = &replay_changed["replay"]["original_runtime"][0]["runtime"]["replay_fingerprint"]
        ["uploads"][0]["sha256"];
    let replayed_hash = &replay_changed["replay"]["replayed_runtime"][0]["runtime"]["replay_fingerprint"]
        ["uploads"][0]["sha256"];
    assert_ne!(original_hash, replayed_hash);
    assert_eq!(
        replay_changed["replay"]["original_runtime"][0]["runtime"]["replay_fingerprint"]["server_url"],
        inputs["server_url"]
    );

    assert_eq!(server.finish().len(), 9);
    fs::remove_dir_all(root)?;
    Ok(())
}

#[test]
fn executor_registry_plan_node_card_and_conformance_report_endpoint_at_run()
-> Result<(), Box<dyn std::error::Error>> {
    let root = generated_comfy_project()?;
    let service = ApiService::new(&root);
    let executors = serde_json::to_value(service.list_executors())?;
    let executor = executors["executors"]
        .as_array()
        .expect("executors")
        .iter()
        .find(|executor| executor["id"] == "comfyui.api.v1")
        .expect("ComfyUI executor");
    assert_eq!(executor["available"], true);
    assert_eq!(executor["kind"], "external");
    assert_eq!(
        executor["status_reason"],
        "executor available; endpoint checked at run"
    );
    assert_eq!(executor["plans_models"], false);

    let plan = lfw(&root, ["plan", "lightflow.comfy_run"])?;
    assert_eq!(plan["runtime"]["executor_id"], "comfyui.api.v1");
    assert_eq!(plan["runtime"]["executor_available"], true);
    assert_eq!(
        plan["runtime"]["executor_status_reason"],
        "executor available; endpoint checked at run"
    );
    assert_eq!(plan["runtime"]["recipe"], "comfyui_workflow");
    assert_eq!(plan["runtime"]["data_policy"], "artifact_handles");

    let node = serde_json::to_value(service.get_node("lightflow.comfy_run")?)?;
    assert_eq!(node["runtimes"][0]["engine"], "comfyui.api.v1");
    assert_eq!(node["runtimes"][0]["available"], true);
    assert_eq!(node["runtimes"][0]["executors"][0]["id"], "comfyui.api.v1");
    assert_eq!(
        node["runtimes"][0]["executors"][0]["status_reason"],
        "executor available; endpoint checked at run"
    );

    let conformance = lfw(&root, ["node", "test", "lightflow.comfy_run"])?;
    assert_eq!(conformance["valid"], true);
    let runtime_check = conformance["checks"]
        .as_array()
        .expect("checks")
        .iter()
        .find(|check| check["id"] == "node.runtime")
        .expect("runtime check");
    assert_eq!(runtime_check["status"], "passed");
    assert!(
        runtime_check["message"]
            .as_str()
            .expect("runtime message")
            .contains("endpoint checked at run")
    );

    fs::remove_dir_all(root)?;
    Ok(())
}

fn generated_comfy_project() -> Result<std::path::PathBuf, Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    lfw(&root, ["init"])?;
    lfw(
        &root,
        [
            "new",
            "comfy_run",
            "--category",
            "image",
            "--name",
            "Comfy Run",
            "--runtime",
            "lightflow.comfyui.workflow",
        ],
    )?;
    Ok(root)
}

fn run_generated(
    root: &std::path::Path,
    action: &str,
    input_file: &str,
) -> Result<Value, Box<dyn std::error::Error>> {
    let output = lfw_command(root)
        .args([
            action,
            "lightflow.comfy_run",
            "--inputs",
            &format!("@{input_file}"),
        ])
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "lfw {action} failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

fn run_replay(root: &std::path::Path, run_id: &str) -> Result<Value, Box<dyn std::error::Error>> {
    let output = lfw_command(root).args(["replay", run_id]).output()?;
    if !output.status.success() {
        return Err(format!(
            "lfw replay failed\nstdout:\n{}\nstderr:\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(serde_json::from_slice(&output.stdout)?)
}
