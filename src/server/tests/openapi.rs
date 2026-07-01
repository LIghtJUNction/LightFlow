use axum::body::{Body, to_bytes};
use axum::http::{HeaderValue, Method, Request, StatusCode, header};
use tower::ServiceExt;

use crate::server::types::HTTP_PATHS;
use crate::server::{ApiService, router};

use super::openapi_path_keys;

mod schema_contract;

#[tokio::test]
async fn openapi_paths_match_http_routes() {
    let openapi = std::fs::read_to_string("openapi/lightflow.yaml").expect("openapi");
    let mut paths = openapi_path_keys(&openapi);
    paths.sort();

    let mut expected = HTTP_PATHS.to_vec();
    expected.sort();

    assert_eq!(paths, expected);
}

#[tokio::test]
async fn openapi_yaml_endpoint_serves_contract() {
    let service = ApiService::new(std::env::current_dir().expect("current dir"));
    let app = router(service);
    let response = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/openapi.yaml")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE),
        Some(&HeaderValue::from_static("application/yaml"))
    );
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    let text = String::from_utf8(body.to_vec()).expect("utf8");
    assert!(text.starts_with("openapi: 3.1.0"));
    assert!(text.contains("/workflows/{workflow_id}/run:"));
}

#[tokio::test]
async fn ui_routes_serve_static_editor_when_present() {
    let service = ApiService::new(std::env::current_dir().expect("current dir"));
    let app = router(service);

    let index = app
        .clone()
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/ui")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(index.status(), StatusCode::OK);
    assert_eq!(
        index.headers().get(header::CONTENT_TYPE),
        Some(&HeaderValue::from_static("text/html; charset=utf-8"))
    );
    let index_body = to_bytes(index.into_body(), usize::MAX).await.expect("body");
    let index_text = String::from_utf8(index_body.to_vec()).expect("utf8");
    assert!(index_text.contains("<title>LightFlow</title>"));

    let script = app
        .oneshot(
            Request::builder()
                .method(Method::GET)
                .uri("/ui/app.js")
                .body(Body::empty())
                .expect("request"),
        )
        .await
        .expect("response");
    assert_eq!(script.status(), StatusCode::OK);
    assert_eq!(
        script.headers().get(header::CONTENT_TYPE),
        Some(&HeaderValue::from_static("text/javascript; charset=utf-8"))
    );
}
