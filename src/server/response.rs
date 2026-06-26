use crate::api::ApiError;
use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::{Value, json};

pub(crate) fn api_json<T: Serialize>(result: Result<T, ApiError>) -> Response {
    api_json_with_status(StatusCode::OK, result)
}

pub(crate) fn api_json_with_status<T: Serialize>(
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

pub(crate) fn error_response(error: ApiError) -> Response {
    let value = json!({
        "error": error.to_string(),
        "code": error.code(),
        "message": error.message(),
        "status": error.status_code(),
    });
    error_json_response(error.status_code(), value)
}

pub(crate) fn error_response_with_run(error: ApiError, run_id: &str, run_dir: String) -> Response {
    let status = error.status_code();
    error_json_response(
        status,
        json!({
            "error": error.to_string(),
            "code": error.code(),
            "message": error.message(),
            "status": status,
            "run_id": run_id,
            "run_dir": run_dir,
            "trace_path": format!("{run_dir}/execution.json"),
        }),
    )
}

pub(crate) fn error_json_response(status_code: u16, value: Value) -> Response {
    let status = StatusCode::from_u16(status_code).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
    with_cors((status, Json(value)).into_response())
}

pub(crate) fn json_response(value: Value) -> Response {
    with_cors(Json(value).into_response())
}

pub(crate) fn with_cors(mut response: Response) -> Response {
    let headers = response.headers_mut();
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET,POST,DELETE,OPTIONS"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("content-type"),
    );
    response
}

pub(crate) async fn cors_options() -> Response {
    with_cors(StatusCode::NO_CONTENT.into_response())
}

pub(crate) async fn not_found() -> Response {
    with_cors((StatusCode::NOT_FOUND, Json(json!({ "error": "not found" }))).into_response())
}
