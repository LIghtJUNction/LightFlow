use crate::cli::mcp;
use crate::server::{response, types::AppState};
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
    response::json_response(mcp::handle_request(&state.service, request))
}
