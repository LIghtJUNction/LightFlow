use crate::server::{ApiService, router};
use axum::body::Body;
use axum::http::{HeaderValue, Method, Request, StatusCode, header};
use tower::ServiceExt;

use super::request_json;

#[tokio::test]
async fn error_responses_are_structured_for_clients() {
    let service = ApiService::new(std::env::current_dir().expect("current dir"));
    let app = router(service);

    let response = request_json(&app, "/workflows/lightflow.missing").await;
    assert_eq!(response["status"], StatusCode::NOT_FOUND.as_u16());
    assert_eq!(response["body"]["code"], "not_found");
    assert_eq!(response["body"]["status"], StatusCode::NOT_FOUND.as_u16());
    assert_eq!(response["body"]["message"], "workflow lightflow.missing");
    assert!(
        response["body"]["error"]
            .as_str()
            .expect("error")
            .contains("not found")
    );
}

#[tokio::test]
async fn post_routes_handle_cors_preflight() {
    let service = ApiService::new(std::env::current_dir().expect("current dir"));
    let app = router(service);

    for path in [
        "/workflows",
        "/runs/last",
        "/patches/qa-debug",
        "/patches/validate",
        "/runs/last/replay",
        "/workflows/lightflow.text_plan/run",
        "/workflows/validate",
        "/mcp",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri(path)
                    .header("origin", "http://127.0.0.1:8000")
                    .header("access-control-request-method", "POST")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("response");
        assert_eq!(response.status(), StatusCode::NO_CONTENT, "{path}");
        assert_eq!(
            response.headers().get(header::ACCESS_CONTROL_ALLOW_ORIGIN),
            Some(&HeaderValue::from_static("*")),
            "{path}"
        );
    }
}
