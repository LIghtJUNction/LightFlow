//! Axum HTTP gateway for LightFlow's backend API.

use crate::api::{ApiError, ApiService};
use crate::cli::mcp;
use crate::workflow::{WorkflowExecutionOptions, WorkflowSpec};
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use serde::Serialize;
use serde_json::{Value, json};
use std::io;
use std::sync::Arc;
use tokio::net::TcpListener;

#[derive(Clone)]
struct AppState {
    service: Arc<ApiService>,
}

/// Run the LightFlow HTTP gateway.
pub async fn serve(service: ApiService, bind: &str) -> io::Result<()> {
    let listener = TcpListener::bind(bind).await?;
    eprintln!("LightFlow backend listening on http://{bind}");
    eprintln!("MCP endpoint available at http://{bind}/mcp");
    axum::serve(listener, router(service))
        .await
        .map_err(io::Error::other)
}

fn router(service: ApiService) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/nodes", get(list_nodes))
        .route("/nodes/{workflow_id}", get(get_node))
        .route("/models", get(list_models))
        .route("/runs", get(list_runs))
        .route("/runs/{run_id}", get(get_run))
        .route("/runs/{run_id}/events", get(get_run_events))
        .route("/artifacts", get(list_artifacts))
        .route("/workflows", get(list_workflows).post(save_workflow))
        .route("/workflows/{workflow_id}", get(get_workflow))
        .route(
            "/workflows/{workflow_id}/dependencies",
            get(workflow_dependencies),
        )
        .route("/workflows/{workflow_id}/run", post(run_workflow))
        .route("/workflows/validate", post(validate_workflow))
        .route("/mcp", get(mcp_info).post(mcp_post).options(mcp_options))
        .fallback(not_found)
        .with_state(AppState {
            service: Arc::new(service),
        })
}

async fn health() -> Response {
    json_response(json!({ "status": "ok" }))
}

async fn list_workflows(State(state): State<AppState>) -> Response {
    api_json(state.service.list_workflows())
}

async fn list_nodes(State(state): State<AppState>) -> Response {
    api_json(state.service.list_nodes())
}

async fn get_node(State(state): State<AppState>, Path(workflow_id): Path<String>) -> Response {
    api_json(state.service.get_node(&workflow_id))
}

async fn list_models(State(state): State<AppState>) -> Response {
    api_json(state.service.list_models())
}

async fn list_runs(State(state): State<AppState>) -> Response {
    api_json(state.service.list_runs())
}

async fn get_run(State(state): State<AppState>, Path(run_id): Path<String>) -> Response {
    api_json(state.service.get_run(&run_id))
}

async fn get_run_events(State(state): State<AppState>, Path(run_id): Path<String>) -> Response {
    api_json(state.service.get_run_events(&run_id))
}

async fn list_artifacts(State(state): State<AppState>) -> Response {
    api_json(state.service.list_artifacts())
}

async fn get_workflow(State(state): State<AppState>, Path(workflow_id): Path<String>) -> Response {
    api_json(state.service.get_workflow(&workflow_id))
}

async fn workflow_dependencies(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
) -> Response {
    api_json(state.service.workflow_dependencies(&workflow_id))
}

async fn run_workflow(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
    Json(options): Json<WorkflowExecutionOptions>,
) -> Response {
    api_json(state.service.execute_workflow(&workflow_id, options))
}

async fn save_workflow(
    State(state): State<AppState>,
    Json(workflow): Json<WorkflowSpec>,
) -> Response {
    api_json_with_status(StatusCode::CREATED, state.service.save_workflow(workflow))
}

async fn validate_workflow(
    State(state): State<AppState>,
    Json(workflow): Json<WorkflowSpec>,
) -> Response {
    api_json(Ok::<_, ApiError>(
        state.service.validate_workflow(&workflow),
    ))
}

async fn mcp_info() -> Response {
    json_response(json!({
        "endpoint": "/mcp",
        "transport": "http",
        "jsonrpc": "2.0"
    }))
}

async fn mcp_post(State(state): State<AppState>, Json(request): Json<Value>) -> Response {
    json_response(mcp::handle_request(&state.service, request))
}

async fn mcp_options() -> Response {
    with_cors(StatusCode::NO_CONTENT.into_response())
}

async fn not_found() -> Response {
    with_cors((StatusCode::NOT_FOUND, Json(json!({ "error": "not found" }))).into_response())
}

fn api_json<T: Serialize>(result: Result<T, ApiError>) -> Response {
    api_json_with_status(StatusCode::OK, result)
}

fn api_json_with_status<T: Serialize>(
    success_status: StatusCode,
    result: Result<T, ApiError>,
) -> Response {
    match result {
        Ok(value) => {
            let mut response = Json(value).into_response();
            *response.status_mut() = success_status;
            with_cors(response)
        }
        Err(error) => error_response(error),
    }
}

fn error_response(error: ApiError) -> Response {
    let status =
        StatusCode::from_u16(error.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    with_cors((status, Json(json!({ "error": error.to_string() }))).into_response())
}

fn json_response(value: Value) -> Response {
    with_cors(Json(value).into_response())
}

fn with_cors(mut response: Response) -> Response {
    let headers = response.headers_mut();
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET,POST,OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("content-type"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;

    #[tokio::test]
    async fn node_directory_endpoints_return_editor_contracts() {
        let service = ApiService::new(std::env::current_dir().expect("current dir"));
        let app = router(service);

        let nodes = request_json(&app, "/nodes").await;
        assert_eq!(nodes["status"], StatusCode::OK.as_u16());
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
        assert_eq!(text_to_image["validation"]["valid"], true);

        let node = request_json(&app, "/nodes/lightflow.text_to_image").await;
        assert_eq!(node["status"], StatusCode::OK.as_u16());
        assert_eq!(node["body"]["id"], "lightflow.text_to_image");
        assert_eq!(node["body"]["models"][0]["id"], "image_model");

        let models = request_json(&app, "/models").await;
        assert_eq!(models["status"], StatusCode::OK.as_u16());
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
    }

    #[tokio::test]
    async fn run_history_endpoints_return_runs_events_and_artifacts() {
        let root =
            std::env::temp_dir().join(format!("lightflow-server-test-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);
        std::fs::create_dir_all(&root).expect("root");
        crate::api::write_history_fixture(&root).expect("fixture");
        let app = router(ApiService::new(&root));

        let runs = request_json(&app, "/runs").await;
        assert_eq!(runs["status"], StatusCode::OK.as_u16());
        assert_eq!(runs["body"]["last"], "run-test");
        assert_eq!(runs["body"]["runs"][0]["workflow_id"], "lightflow.fixture");

        let run = request_json(&app, "/runs/last").await;
        assert_eq!(run["status"], StatusCode::OK.as_u16());
        assert_eq!(run["body"]["run_id"], "run-test");

        let events = request_json(&app, "/runs/run-test/events").await;
        assert_eq!(events["status"], StatusCode::OK.as_u16());
        assert_eq!(events["body"]["events"][0]["event"], "run_started");

        let artifacts = request_json(&app, "/artifacts").await;
        assert_eq!(artifacts["status"], StatusCode::OK.as_u16());
        assert_eq!(
            artifacts["body"]["artifacts"][0]["artifact"]["kind"],
            "image"
        );

        let _ = std::fs::remove_dir_all(&root);
    }

    async fn request_json(app: &Router, path: &str) -> serde_json::Value {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(path)
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("body");
        json!({
            "status": status.as_u16(),
            "body": serde_json::from_slice::<serde_json::Value>(&body).expect("json"),
        })
    }
}
