use axum::Router;
use axum::http::StatusCode;

use super::{assert_required_fields, request_json};

pub(crate) async fn verify_live_http_endpoints_contracts(app: &Router, openapi: &str) {
    for (path, schema) in [
        ("/workflows", "WorkflowList"),
        ("/nodes", "NodeCatalog"),
        ("/nodes/lightflow.text_to_image", "NodeCard"),
        ("/executors", "ExecutorCatalog"),
        ("/models", "ModelCatalog"),
        ("/patches", "PatchCatalog"),
        ("/loop", "LocalLoopReport"),
        ("/loop/changes", "LoopChangesReport"),
        ("/loop/projects", "ProjectWorkspaceCatalog"),
        ("/release", "ReleaseCheckReport"),
        ("/publish", "WorkflowPublishCatalog"),
        (
            "/workflows/lightflow.text_plan/dependencies",
            "WorkflowDependencyReport",
        ),
        ("/workflows/lightflow.text_plan/loop", "LocalLoopReport"),
        ("/workflows/lightflow.text_plan/plan", "WorkflowPlan"),
        (
            "/workflows/lightflow.text_plan/publish",
            "WorkflowPublishCheck",
        ),
    ] {
        let response = request_json(app, path).await;
        assert_eq!(response["status"], StatusCode::OK.as_u16(), "{path}");
        assert_required_fields(openapi, schema, &response["body"]);
    }
}
