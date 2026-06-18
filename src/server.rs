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
