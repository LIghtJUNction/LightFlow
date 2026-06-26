use crate::server::{response, types::AppState, types::OPENAPI_YAML};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde_json::json;
use std::fs;

pub(crate) async fn health() -> Response {
    response::json_response(json!({ "status": "ok" }))
}

pub(crate) async fn ui_index(State(state): State<AppState>) -> Response {
    ui_file(&state, "index.html")
}

pub(crate) async fn ui_asset(State(state): State<AppState>, Path(asset): Path<String>) -> Response {
    match asset.as_str() {
        "app.js" | "styles.css" | "smoke.mjs" => ui_file(&state, &asset),
        _ => response::not_found().await,
    }
}

fn ui_file(state: &AppState, asset: &str) -> Response {
    let path = state.service.repo_root().join("LightFlowUI").join(asset);
    let Ok(bytes) = fs::read(&path) else {
        return response::with_cors(
            (
                StatusCode::NOT_FOUND,
                Json(json!({
                    "error": "ui not found",
                    "message": format!("LightFlowUI asset is missing: {}", path.display()),
                })),
            )
                .into_response(),
        );
    };
    let mut response = bytes.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static(ui_content_type(asset)),
    );
    response::with_cors(response)
}

fn ui_content_type(asset: &str) -> &'static str {
    match asset {
        "index.html" => "text/html; charset=utf-8",
        "styles.css" => "text/css; charset=utf-8",
        "app.js" | "smoke.mjs" => "text/javascript; charset=utf-8",
        _ => "application/octet-stream",
    }
}

pub(crate) async fn openapi_yaml() -> Response {
    let mut response = OPENAPI_YAML.into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/yaml"),
    );
    response::with_cors(response)
}
