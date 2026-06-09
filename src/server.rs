//! Axum HTTP gateway for LightFlow's REST, MCP, and runtime stream endpoints.

use crate::api::{ApiError, ApiService, CreateRunRequest};
use crate::mcp;
use crate::stream;
use axum::body::Bytes;
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
use tower_http::services::ServeDir;

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
    let ui_dist = service.repo_root().join("LightFlowUI").join("dist");
    let router = Router::new()
        .route("/health", get(health))
        .route("/workflows", get(list_workflows))
        .route("/nodes", get(list_nodes))
        .route("/compositions", get(list_compositions))
        .route("/models", get(list_models))
        .route("/ctx/abi", get(ctx_abi))
        .route("/runs", get(list_runs).post(create_run))
        .route("/runs/preview", post(preview_run))
        .route("/runs/{run_id}", get(get_run))
        .route("/runs/{run_id}/status", get(run_status))
        .route("/runs/{run_id}/cancel", post(cancel_run))
        .route("/runs/{run_id}/request", get(run_request))
        .route("/runs/{run_id}/workflow", get(run_workflow))
        .route("/runs/{run_id}/steps/{step_id}/submit", post(submit_step))
        .route("/runs/{run_id}/refresh", post(refresh_run))
        .route("/runs/{run_id}/events", get(run_events))
        .route("/runs/{run_id}/trace", get(run_trace))
        .route("/__lightflow_diag", get(ui_diag_get).post(ui_diag_post))
        .route("/", get(ui_index))
        .route("/mcp", get(mcp_info).post(mcp_post).options(mcp_options))
        .route("/runtime/streams", get(runtime_streams))
        .route("/runtime/streams/schema.fbs", get(runtime_stream_schema))
        .route(
            "/runtime/streams/{run_id}/snapshot.fb",
            get(runtime_stream_snapshot),
        )
        .with_state(AppState {
            service: Arc::new(service),
        });

    if ui_dist.is_dir() {
        router
            .nest_service("/LightFlowUI", ServeDir::new(ui_dist.clone()))
            .fallback_service(ServeDir::new(ui_dist))
    } else {
        router.fallback(not_found)
    }
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

async fn list_compositions(State(state): State<AppState>) -> Response {
    api_json(state.service.list_compositions())
}

async fn list_models(State(state): State<AppState>) -> Response {
    api_json(state.service.list_models())
}

async fn ctx_abi(State(state): State<AppState>) -> Response {
    api_json(Ok::<_, ApiError>(state.service.ctx_abi()))
}

async fn list_runs(State(state): State<AppState>) -> Response {
    api_json(state.service.list_runs())
}

async fn preview_run(
    State(state): State<AppState>,
    Json(request): Json<CreateRunRequest>,
) -> Response {
    api_json(state.service.preview_run(request))
}

async fn create_run(
    State(state): State<AppState>,
    Json(request): Json<CreateRunRequest>,
) -> Response {
    api_json_with_status(StatusCode::ACCEPTED, state.service.create_run(request))
}

async fn get_run(State(state): State<AppState>, Path(run_id): Path<String>) -> Response {
    api_json(state.service.get_run(&run_id))
}

async fn run_status(State(state): State<AppState>, Path(run_id): Path<String>) -> Response {
    api_json(state.service.run_status(&run_id))
}

async fn cancel_run(State(state): State<AppState>, Path(run_id): Path<String>) -> Response {
    api_json(state.service.cancel_run(&run_id))
}

async fn run_request(State(state): State<AppState>, Path(run_id): Path<String>) -> Response {
    api_json(state.service.run_request(&run_id))
}

async fn run_workflow(State(state): State<AppState>, Path(run_id): Path<String>) -> Response {
    api_json(state.service.run_workflow(&run_id))
}

async fn submit_step(
    State(state): State<AppState>,
    Path((run_id, step_id)): Path<(String, String)>,
    body: Bytes,
) -> Response {
    let body = if body.is_empty() {
        None
    } else {
        Some(body.as_ref())
    };
    api_json_with_status(
        StatusCode::ACCEPTED,
        state.service.submit_step(&run_id, &step_id, body),
    )
}

async fn refresh_run(State(state): State<AppState>, Path(run_id): Path<String>) -> Response {
    api_json(state.service.refresh_run(&run_id))
}

async fn run_events(State(state): State<AppState>, Path(run_id): Path<String>) -> Response {
    api_text_ndjson(state.service.run_events(&run_id))
}

async fn run_trace(State(state): State<AppState>, Path(run_id): Path<String>) -> Response {
    api_text_ndjson(state.service.run_trace(&run_id))
}

async fn ui_diag_get() -> Response {
    let path = std::env::temp_dir().join("lightflow-ui-diag.log");
    match std::fs::read_to_string(path) {
        Ok(body) => {
            let mut response = body.into_response();
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/plain; charset=utf-8"),
            );
            with_cors(response)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            with_cors((StatusCode::NOT_FOUND, "no diagnostics").into_response())
        }
        Err(error) => with_cors(
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to read diagnostics: {error}"),
            )
                .into_response(),
        ),
    }
}

async fn ui_diag_post(body: Bytes) -> Response {
    let path = std::env::temp_dir().join("lightflow-ui-diag.log");
    let mut text = String::from_utf8_lossy(&body).to_string();
    text.retain(|ch| {
        ch.is_ascii_alphanumeric() || matches!(ch, ' ' | '_' | '-' | ':' | '=' | '\n')
    });
    if text.len() > 4096 {
        text.truncate(4096);
    }
    match std::fs::write(path, text) {
        Ok(()) => with_cors((StatusCode::NO_CONTENT, "").into_response()),
        Err(error) => with_cors(
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("failed to write diagnostics: {error}"),
            )
                .into_response(),
        ),
    }
}

async fn ui_index(State(state): State<AppState>) -> Response {
    let path = state
        .service
        .repo_root()
        .join("LightFlowUI")
        .join("dist")
        .join("index.html");
    match std::fs::read_to_string(path) {
        Ok(body) => {
            let mut response = body.into_response();
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("text/html; charset=utf-8"),
            );
            response.headers_mut().insert(
                header::CACHE_CONTROL,
                HeaderValue::from_static("no-store, max-age=0"),
            );
            with_cors(response)
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => with_cors(
            (
                StatusCode::NOT_FOUND,
                Json(json!({ "error": "LightFlowUI dist/index.html not found" })),
            )
                .into_response(),
        ),
        Err(error) => with_cors(
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(json!({ "error": format!("failed to read LightFlowUI index: {error}") })),
            )
                .into_response(),
        ),
    }
}

async fn mcp_info() -> Response {
    json_response(json!({
        "endpoint": "LightFlow MCP",
        "transport": "http",
        "method": "POST",
        "path": "/mcp"
    }))
}

async fn mcp_options() -> Response {
    with_cors(StatusCode::NO_CONTENT.into_response())
}

async fn mcp_post(State(state): State<AppState>, body: Bytes) -> Response {
    let response = match serde_json::from_slice::<Value>(&body) {
        Ok(request) => mcp::handle_request(&state.service, request),
        Err(error) => json!({
            "jsonrpc": "2.0",
            "id": null,
            "error": {
                "code": -32700,
                "message": error.to_string()
            }
        }),
    };
    json_response(response)
}

async fn runtime_streams() -> Response {
    json_response(
        serde_json::to_value(stream::stream_info()).unwrap_or_else(|error| {
            json!({
                "error": format!("failed to encode runtime stream metadata: {error}")
            })
        }),
    )
}

async fn runtime_stream_schema() -> Response {
    let mut response = stream::SCHEMA.to_owned().into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/plain; charset=utf-8"),
    );
    with_cors(response)
}

async fn runtime_stream_snapshot(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Response {
    match stream::encode_run_snapshot(&state.service, &run_id) {
        Ok(frame) => {
            let mut response = frame.into_response();
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static(stream::MIME_TYPE),
            );
            response.headers_mut().insert(
                "x-lightflow-frame-encoding",
                HeaderValue::from_static("flatbuffers"),
            );
            response.headers_mut().insert(
                "x-lightflow-flatbuffers-file-identifier",
                HeaderValue::from_static(stream::FILE_IDENTIFIER),
            );
            with_cors(response)
        }
        Err(error) => with_cors(
            (
                StatusCode::from_u16(error.status_code())
                    .unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                Json(json!({ "error": error.to_string() })),
            )
                .into_response(),
        ),
    }
}

async fn not_found() -> Response {
    with_cors((StatusCode::NOT_FOUND, Json(json!({ "error": "not found" }))).into_response())
}

fn json_response(value: Value) -> Response {
    with_cors(Json(value).into_response())
}

fn api_json<T: Serialize>(result: Result<T, ApiError>) -> Response {
    api_json_with_status(StatusCode::OK, result)
}

fn api_json_with_status<T: Serialize>(status: StatusCode, result: Result<T, ApiError>) -> Response {
    match result {
        Ok(value) => with_cors((status, Json(value)).into_response()),
        Err(error) => api_error_response(error),
    }
}

fn api_text_ndjson(result: Result<String, ApiError>) -> Response {
    match result {
        Ok(body) => {
            let mut response = body.into_response();
            response.headers_mut().insert(
                header::CONTENT_TYPE,
                HeaderValue::from_static("application/x-ndjson; charset=utf-8"),
            );
            with_cors(response)
        }
        Err(error) => api_error_response(error),
    }
}

fn api_error_response(error: ApiError) -> Response {
    with_cors(
        (
            StatusCode::from_u16(error.status_code()).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
            Json(json!({ "error": error.to_string() })),
        )
            .into_response(),
    )
}

fn with_cors(mut response: Response) -> Response {
    let headers = response.headers_mut();
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_ORIGIN,
        HeaderValue::from_static("*"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_HEADERS,
        HeaderValue::from_static("content-type, authorization"),
    );
    headers.insert(
        header::ACCESS_CONTROL_ALLOW_METHODS,
        HeaderValue::from_static("GET, POST, OPTIONS"),
    );
    response
}

#[cfg(test)]
mod tests {
    use super::router;
    use crate::api::ApiService;
    use crate::cortex::CortexHome;
    use crate::runs::{RunStore, RuntimeDirs};
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode, header};
    use serde_json::{Value, json};
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::time::{SystemTime, UNIX_EPOCH};
    use tower::ServiceExt;

    #[tokio::test]
    async fn rest_routes_cover_openapi_run_lifecycle() -> Result<(), Box<dyn std::error::Error>> {
        let root = unique_temp_root();
        let service = service_for_root(&root);
        let app = router(service);

        let workflows = request_json(app.clone(), "GET", "/workflows", None).await?;
        let workflow_ids = workflows["assets"]
            .as_array()
            .expect("assets array")
            .iter()
            .map(|asset| asset["meta"]["id"].as_str().unwrap_or_default())
            .collect::<Vec<_>>();
        assert!(workflow_ids.contains(&"workflow.text_plan"));

        let abi = request_json(app.clone(), "GET", "/ctx/abi", None).await?;
        assert_eq!(abi["kernel"], "fuse");
        assert_eq!(abi["kernel_tree"], false);
        assert_eq!(abi["upstream"], "generic kernel primitives only");

        let preview = request_json(
            app.clone(),
            "POST",
            "/runs/preview",
            Some(json!({
                "workflow_asset_id": "workflow.text_plan",
                "run_id": "http-run",
                "inputs": { "prompt": "draft an HTTP test plan" }
            })),
        )
        .await?;
        assert_eq!(preview["run_id"], "http-run");
        assert_eq!(preview["ready"], true);
        assert!(!root.join("state/lightflow/runs/http-run").exists());

        let created = request_json(
            app.clone(),
            "POST",
            "/runs",
            Some(json!({
                "workflow_asset_id": "workflow.text_plan",
                "run_id": "http-run",
                "inputs": { "prompt": "draft an HTTP test plan" }
            })),
        )
        .await?;
        assert_eq!(created["run_id"], "http-run");
        assert_eq!(created["steps"][0]["status"], "planned");

        let status = request_json(app.clone(), "GET", "/runs/http-run/status", None).await?;
        assert_eq!(status["status"], "planned");
        assert_eq!(status["planned_steps"], 1);

        let submitted = request_json(
            app.clone(),
            "POST",
            "/runs/http-run/steps/draft/submit",
            None,
        )
        .await?;
        assert_eq!(submitted["steps"][0]["status"], "submitted");

        let request_body =
            fs::read_to_string(root.join("ctx/home/1000/api/openai.chat/inbox/draft.req.json"))?;
        assert_eq!(
            serde_json::from_str::<Value>(&request_body)?,
            json!({
                "messages": [
                    {
                        "role": "user",
                        "content": "draft an HTTP test plan"
                    }
                ]
            })
        );

        let outbox = root.join("ctx/home/1000/api/openai.chat/outbox");
        fs::create_dir_all(&outbox)?;
        fs::write(outbox.join("draft.resp.json"), "{\"ok\":true}\n")?;
        fs::write(outbox.join("draft.fingerprint"), "fnv1a64:http\n")?;
        fs::write(
            outbox.join("draft.route.json"),
            "{\"provider\":\"local\",\"model\":\"http-model\",\"reason\":\"server_test\"}\n",
        )?;

        let refreshed = request_json(app.clone(), "POST", "/runs/http-run/refresh", None).await?;
        assert_eq!(refreshed["steps"][0]["status"], "succeeded");
        assert_eq!(refreshed["steps"][0]["provider_id"], "local");
        assert_eq!(refreshed["steps"][0]["model_id"], "http-model");

        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/runs/http-run/events")
                    .body(Body::empty())?,
            )
            .await?;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/x-ndjson; charset=utf-8"
        );
        let body = to_bytes(response.into_body(), usize::MAX).await?;
        let events = String::from_utf8(body.to_vec())?;
        assert!(events.contains("\"event\":\"run.created\""));
        assert!(events.contains("\"event\":\"step.submitted\""));
        assert!(events.contains("\"event\":\"step.succeeded\""));

        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[tokio::test]
    async fn rest_routes_map_service_errors_to_json_statuses()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = unique_temp_root();
        let app = router(service_for_root(&root));

        let response = app
            .oneshot(
                Request::builder()
                    .method("GET")
                    .uri("/runs/missing")
                    .body(Body::empty())?,
            )
            .await?;

        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = to_bytes(response.into_body(), usize::MAX).await?;
        let body: Value = serde_json::from_slice(&body)?;
        assert!(
            body["error"]
                .as_str()
                .unwrap_or_default()
                .contains("not found")
        );

        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    async fn request_json(
        app: axum::Router,
        method: &str,
        uri: &str,
        body: Option<Value>,
    ) -> Result<Value, Box<dyn std::error::Error>> {
        let body = match body {
            Some(value) => Body::from(serde_json::to_vec(&value)?),
            None => Body::empty(),
        };
        let response = app
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(uri)
                    .header(header::CONTENT_TYPE, "application/json")
                    .body(body)?,
            )
            .await?;
        assert!(
            response.status().is_success(),
            "{method} {uri} failed with {}",
            response.status()
        );
        let body = to_bytes(response.into_body(), usize::MAX).await?;
        Ok(serde_json::from_slice(&body)?)
    }

    fn service_for_root(root: &Path) -> ApiService {
        ApiService::with_cortex_home(
            Path::new(env!("CARGO_MANIFEST_DIR")),
            RunStore::new(RuntimeDirs::new(
                root.join("cfg"),
                root.join("state"),
                root.join("cache"),
                root.join("runtime"),
            )),
            CortexHome::new(root.join("ctx"), 1000),
        )
    }

    fn unique_temp_root() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "lightflow-server-test-{}-{nanos}",
            std::process::id()
        ))
    }
}
