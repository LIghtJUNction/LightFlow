use crate::cli::mcp;
use crate::server::{blocking, response, types::AppState};
use axum::Json;
use axum::extract::State;
use axum::response::Response;
use serde_json::json;

pub(crate) async fn mcp_info() -> Response {
    response::json_response(json!({
        "endpoint": "/mcp",
        "transport": "http",
        "jsonrpc": "2.0"
    }))
}

pub(crate) async fn mcp_post(
    State(state): State<AppState>,
    Json(request): Json<serde_json::Value>,
) -> Response {
    let service = std::sync::Arc::clone(&state.service);
    match blocking::run(&state, move || Ok(mcp::handle_request(&service, request))).await {
        Ok(value) => response::json_response(value),
        Err(error) => response::error_response(error),
    }
}
