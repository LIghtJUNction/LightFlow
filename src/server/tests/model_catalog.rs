use crate::server::{ApiService, router};
use axum::http::StatusCode;
use serde_json::json;

use super::{request_json, std_project_path, temp_root};

#[tokio::test]
async fn model_catalog_reports_lock_status() {
    let root = temp_root("model-lock");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("root");
    let model_path = root.join("models").join("image.gguf");
    std::fs::create_dir_all(model_path.parent().expect("model parent")).expect("models");
    std::fs::write(&model_path, b"tiny").expect("model");
    std::fs::write(
        root.join("lfw.lock"),
        serde_json::to_vec_pretty(&json!({
            "version": 2,
            "models": {
                "lightflow.text_to_image::image_model": {
                    "requirement_id": "image_model",
                    "variant_id": "tiny-q4",
                    "repo": "example/tiny",
                    "file": "image.gguf",
                    "format": "gguf",
                    "sha256": "abc123",
                    "hash_algorithm": "sha256",
                    "size_bytes": 4,
                    "snapshot_revision": "rev1",
                    "local_paths": [model_path],
                }
            }
        }))
        .expect("lock json"),
    )
    .expect("lock");
    let service = ApiService::new(&root).with_workflow_paths(vec![std_project_path()]);
    let app = router(service);

    let models = request_json(&app, "/models").await;
    assert_eq!(models["status"], StatusCode::OK.as_u16());
    let total = models["body"]["total"]
        .as_u64()
        .expect("model catalog total");
    let available = models["body"]["available_count"]
        .as_u64()
        .expect("model catalog available count");
    let blocked = models["body"]["blocked_count"]
        .as_u64()
        .expect("model catalog blocked count");
    assert_eq!(available + blocked, total);
    assert!(available >= 1);
    assert_eq!(
        models["body"]["issues"]
            .as_array()
            .expect("model catalog issues")
            .len() as u64,
        blocked
    );
    assert!(
        !models["body"]["issues"]
            .as_array()
            .expect("model catalog issues")
            .iter()
            .any(|issue| issue
                .as_str()
                .unwrap_or_default()
                .contains("lightflow.text_to_image::image_model"))
    );
    let image_model = models["body"]["models"]
        .as_array()
        .expect("models")
        .iter()
        .find(|model| {
            model["workflow_id"] == "lightflow.text_to_image"
                && model["requirement"]["id"] == "image_model"
        })
        .expect("image model");
    assert_eq!(image_model["lock"]["status"], "available");
    assert_eq!(
        image_model["lock"]["key"],
        "lightflow.text_to_image::image_model"
    );
    assert_eq!(image_model["lock"]["sha256"], "abc123");
    assert_eq!(
        image_model["lock"]["local_paths"][0],
        model_path.display().to_string()
    );
    assert_eq!(
        image_model["sync_command"],
        json!([
            "lfw",
            "sync",
            "lightflow.text_to_image",
            "--auto-model",
            "--apply"
        ])
    );
    assert_eq!(
        image_model["verify_command"],
        json!([
            "lfw",
            "sync",
            "lightflow.text_to_image",
            "--locked",
            "--apply"
        ])
    );
    assert_eq!(
        image_model["lock"]["missing_paths"]
            .as_array()
            .expect("missing paths")
            .len(),
        0
    );
    let available_models = request_json(
        &app,
        "/models?workflow_id=lightflow.text_to_image&status=available",
    )
    .await;
    assert_eq!(available_models["status"], StatusCode::OK.as_u16());
    assert_eq!(available_models["body"]["total"], 1);
    assert_eq!(available_models["body"]["available_count"], 1);
    assert_eq!(available_models["body"]["blocked_count"], 0);
    assert_eq!(
        available_models["body"]["models"][0]["lock"]["key"],
        "lightflow.text_to_image::image_model"
    );
    let blocked_models = request_json(
        &app,
        "/models?workflow_id=lightflow.text_to_image&status=blocked",
    )
    .await;
    assert_eq!(blocked_models["status"], StatusCode::OK.as_u16());
    assert_eq!(blocked_models["body"]["total"], 0);
    assert_eq!(
        blocked_models["body"]["models"]
            .as_array()
            .expect("models")
            .len(),
        0
    );
    let invalid_status = request_json(&app, "/models?status=offline").await;
    assert_eq!(invalid_status["status"], StatusCode::BAD_REQUEST.as_u16());
    assert_eq!(invalid_status["body"]["code"], "invalid_request");

    let _ = std::fs::remove_dir_all(root);
}

#[tokio::test]
async fn model_catalog_reports_missing_locked_model_paths() {
    let root = temp_root("model-lock-missing-path");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("root");
    let model_path = root.join("models").join("missing.gguf");
    std::fs::write(
        root.join("lfw.lock"),
        serde_json::to_vec_pretty(&json!({
            "version": 2,
            "models": {
                "lightflow.text_to_image::image_model": {
                    "requirement_id": "image_model",
                    "variant_id": "missing-q4",
                    "repo": "example/missing",
                    "file": "missing.gguf",
                    "format": "gguf",
                    "sha256": "def456",
                    "hash_algorithm": "sha256",
                    "local_paths": [model_path],
                }
            }
        }))
        .expect("lock json"),
    )
    .expect("lock");
    let service = ApiService::new(&root).with_workflow_paths(vec![std_project_path()]);
    let app = router(service);

    let models = request_json(&app, "/models").await;
    assert_eq!(models["status"], StatusCode::OK.as_u16());
    assert_eq!(
        models["body"]["issues"]
            .as_array()
            .expect("model catalog issues")
            .len() as u64,
        models["body"]["blocked_count"]
            .as_u64()
            .expect("model catalog blocked count")
    );
    assert!(
        models["body"]["issues"]
            .as_array()
            .expect("issues")
            .iter()
            .any(|issue| issue
                .as_str()
                .unwrap_or_default()
                .contains("lightflow.text_to_image::image_model: model lock is missing_path"))
    );
    let image_model = models["body"]["models"]
        .as_array()
        .expect("models")
        .iter()
        .find(|model| {
            model["workflow_id"] == "lightflow.text_to_image"
                && model["requirement"]["id"] == "image_model"
        })
        .expect("image model");
    assert_eq!(image_model["lock"]["status"], "missing_path");
    assert_eq!(
        image_model["lock"]["missing_paths"][0],
        model_path.display().to_string()
    );
    assert_eq!(
        image_model["sync_command"],
        json!([
            "lfw",
            "sync",
            "lightflow.text_to_image",
            "--auto-model",
            "--apply"
        ])
    );
    assert_eq!(
        image_model["verify_command"],
        json!([
            "lfw",
            "sync",
            "lightflow.text_to_image",
            "--locked",
            "--apply"
        ])
    );

    let _ = std::fs::remove_dir_all(root);
}
