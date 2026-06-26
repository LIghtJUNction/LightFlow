use super::{request_json, request_json_post, temp_root};
use crate::server::{ApiService, router};
use axum::http::StatusCode;

#[tokio::test]
async fn run_history_endpoints_return_runs_events_and_artifacts() {
    let root = temp_root("history");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("root");
    crate::api::write_history_fixture(&root).expect("fixture");
    let app = router(ApiService::new(&root));

    let runs = request_json(&app, "/runs").await;
    assert_eq!(runs["status"], StatusCode::OK.as_u16());
    assert_eq!(runs["body"]["last"], "run-test");
    assert_eq!(runs["body"]["runs"][0]["workflow_id"], "lightflow.fixture");
    assert_eq!(runs["body"]["runs"][0]["duration_ms"], 1);
    assert_eq!(
        runs["body"]["runs"][0]["workflow_ids"],
        serde_json::json!(["lightflow.fixture"])
    );

    let run = request_json(&app, "/runs/last").await;
    assert_eq!(run["status"], StatusCode::OK.as_u16());
    assert_eq!(run["body"]["run_id"], "run-test");

    let events = request_json(&app, "/runs/run-test/events").await;
    assert_eq!(events["status"], StatusCode::OK.as_u16());
    assert_eq!(events["body"]["events"][0]["event"], "run_started");

    let artifacts = request_json(&app, "/artifacts").await;
    assert_eq!(artifacts["status"], StatusCode::OK.as_u16());
    assert_eq!(artifacts["body"]["artifacts"][0]["stage_index"], 0);
    assert_eq!(
        artifacts["body"]["artifacts"][0]["artifact"]["kind"],
        "image"
    );
    let filtered_artifacts = request_json(
        &app,
        "/artifacts?run_id=last&workflow_id=lightflow.fixture&kind=image&limit=1",
    )
    .await;
    assert_eq!(filtered_artifacts["status"], StatusCode::OK.as_u16());
    assert_eq!(
        filtered_artifacts["body"]["artifacts"]
            .as_array()
            .expect("filtered artifacts")
            .len(),
        1
    );
    assert_eq!(
        filtered_artifacts["body"]["artifacts"][0]["run_id"],
        "run-test"
    );

    let _ = std::fs::remove_dir_all(&root);
}

#[tokio::test]
async fn http_workflow_runs_are_recorded_in_history() {
    let root = temp_root("http-run-history");
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("root");
    let checkout = std::env::current_dir().expect("current dir");
    let service =
        ApiService::new(&root).with_workflow_paths(vec![checkout.join("projects/lightflow-std")]);
    let app = router(service);

    let strict_before_run = request_json(
        &app,
        "/workflows/lightflow.text_plan/loop?require_replay=true",
    )
    .await;
    assert_eq!(strict_before_run["status"], StatusCode::OK.as_u16());
    assert!(
        strict_before_run["body"]["checks"]
            .as_array()
            .expect("checks")
            .iter()
            .any(|check| {
                check["id"] == "loop.selected.replay.required" && check["status"] == "failed"
            })
    );

    let run = request_json_post(
        &app,
        "/workflows/lightflow.text_plan/run",
        serde_json::json!({ "inputs": { "value": "hello" } }),
    )
    .await;
    assert_eq!(run["status"], StatusCode::OK.as_u16());
    let run_id = run["body"]["run_id"].as_str().expect("run id");
    assert_eq!(run["body"]["outputs"]["result"], "hello");

    let runs = request_json(&app, "/runs").await;
    assert_eq!(runs["status"], StatusCode::OK.as_u16());
    assert_eq!(runs["body"]["last"], run_id);
    assert_eq!(runs["body"]["runs"][0]["status"], "completed");
    assert_eq!(
        runs["body"]["runs"][0]["workflow_id"],
        "lightflow.text_plan"
    );
    assert!(runs["body"]["runs"][0]["duration_ms"].is_number());
    assert_eq!(runs["body"]["runs"][0]["surface"], "http");

    let trace = request_json(&app, &format!("/runs/{run_id}")).await;
    assert_eq!(trace["status"], StatusCode::OK.as_u16());
    assert_eq!(trace["body"]["execution"]["outputs"]["result"], "hello");
    assert_eq!(trace["body"]["events"][0]["surface"], "http");
    assert_eq!(
        trace["body"]["events"]
            .as_array()
            .expect("events")
            .last()
            .expect("last event")["event"],
        "run_finished"
    );

    let replay = request_json_post(
        &app,
        &format!("/runs/{run_id}/replay"),
        serde_json::json!({}),
    )
    .await;
    assert_eq!(replay["status"], StatusCode::OK.as_u16());
    assert_eq!(replay["body"]["replayed_from"], run_id);
    assert_eq!(replay["body"]["outputs"]["result"], "hello");
    assert_ne!(replay["body"]["run_id"], run_id);
    assert_eq!(replay["body"]["replay"]["runtime_changed"], false);

    let replay_trace = request_json(&app, "/runs/last").await;
    assert_eq!(replay_trace["status"], StatusCode::OK.as_u16());
    assert_eq!(replay_trace["body"]["run_id"], replay["body"]["run_id"]);
    assert_eq!(
        replay_trace["body"]["execution"]["outputs"]["result"],
        "hello"
    );
    assert_eq!(
        replay_trace["body"]["execution"]["replay"]["runtime_changed"],
        false
    );

    let strict_after_run = request_json(
        &app,
        "/workflows/lightflow.text_plan/loop?require_replay=true",
    )
    .await;
    assert_eq!(strict_after_run["status"], StatusCode::OK.as_u16());
    assert!(
        strict_after_run["body"]["checks"]
            .as_array()
            .expect("checks")
            .iter()
            .any(|check| {
                check["id"] == "loop.selected.replay.required" && check["status"] == "passed"
            })
    );

    let failed = request_json_post(
        &app,
        "/workflows/lightflow.missing/run",
        serde_json::json!({ "inputs": {} }),
    )
    .await;
    assert_eq!(failed["status"], StatusCode::NOT_FOUND.as_u16());
    let failed_run_id = failed["body"]["run_id"].as_str().expect("failed run id");
    assert!(failed_run_id.starts_with("run-"));
    assert!(
        failed["body"]["trace_path"]
            .as_str()
            .expect("failed trace path")
            .ends_with("/execution.json")
    );
    let runs_after_failure = request_json(&app, "/runs").await;
    assert_eq!(runs_after_failure["body"]["runs"][0]["status"], "failed");
    assert_eq!(runs_after_failure["body"]["last"], failed_run_id);
    assert_eq!(runs_after_failure["body"]["runs"][0]["surface"], "http");
    let failed_runs = request_json(&app, "/runs?status=failed&limit=1").await;
    assert_eq!(failed_runs["status"], StatusCode::OK.as_u16());
    assert_eq!(failed_runs["body"]["total"], 1);
    assert_eq!(failed_runs["body"]["failed_count"], 1);
    assert_eq!(failed_runs["body"]["runs"][0]["run_id"], failed_run_id);
    let invalid_status = request_json(&app, "/runs?status=offline").await;
    assert_eq!(invalid_status["status"], StatusCode::BAD_REQUEST.as_u16());
    assert!(
        invalid_status["body"]["error"]
            .as_str()
            .expect("run status error")
            .contains("expected completed, failed, or unknown"),
        "invalid status response:\n{invalid_status}"
    );
    let text_plan_runs = request_json(&app, "/runs?workflow_id=lightflow.text_plan").await;
    assert_eq!(text_plan_runs["status"], StatusCode::OK.as_u16());
    assert!(
        text_plan_runs["body"]["runs"]
            .as_array()
            .expect("text plan runs")
            .iter()
            .all(|run| run["workflow_ids"]
                .as_array()
                .expect("workflow ids")
                .iter()
                .any(|id| id == "lightflow.text_plan"))
    );
    let failed_trace = request_json(&app, &format!("/runs/{failed_run_id}")).await;
    assert_eq!(failed_trace["status"], StatusCode::OK.as_u16());
    assert_eq!(failed_trace["body"]["manifest"]["status"], "failed");
    assert_eq!(
        runs_after_failure["body"]["runs"][0]["workflow_id"],
        "lightflow.missing"
    );

    let _ = std::fs::remove_dir_all(&root);
}
