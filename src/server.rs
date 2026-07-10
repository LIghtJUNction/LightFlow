//! Axum HTTP gateway for LightFlow's backend API.

mod blocking;
mod catalog;
mod checks;
mod mcp;
mod patches;
mod response;
mod run;
mod static_routes;
#[cfg(test)]
mod tests;
mod types;
mod workflow;

use crate::api::ApiService;
use axum::Router;
use axum::routing::{get, post};
use std::io;
use tokio::net::TcpListener;

/// Run the LightFlow HTTP gateway.
pub async fn serve(service: ApiService, bind: &str) -> io::Result<()> {
    let listener = TcpListener::bind(bind).await?;
    eprintln!("LightFlow backend listening on http://{bind}");
    eprintln!("LightFlow editor available at http://{bind}/ui when LightFlowUI is present");
    eprintln!("MCP endpoint available at http://{bind}/mcp");
    axum::serve(listener, router(service))
        .await
        .map_err(io::Error::other)
}

fn router(service: ApiService) -> Router {
    Router::new()
        .route("/health", get(static_routes::health))
        .route("/ui", get(static_routes::ui_index))
        .route("/ui/", get(static_routes::ui_index))
        .route("/ui/{asset}", get(static_routes::ui_asset))
        .route("/openapi.yaml", get(static_routes::openapi_yaml))
        .route("/loop", get(checks::loop_check_project))
        .route("/loop/changes", get(checks::loop_changes_project))
        .route("/loop/projects", get(checks::loop_projects))
        .route("/release", get(checks::release_check_project))
        .route("/publish", get(checks::publish_workflows))
        .route("/nodes", get(catalog::list_nodes))
        .route("/nodes/{workflow_id}", get(catalog::get_node))
        .route("/executors", get(catalog::list_executors))
        .route("/models", get(catalog::list_models))
        .route("/runs", get(run::list_runs))
        .route(
            "/runs/{run_id}",
            get(run::get_run)
                .delete(run::remove_run)
                .options(response::cors_options),
        )
        .route(
            "/runs/{run_id}/replay",
            post(run::replay_run).options(response::cors_options),
        )
        .route("/runs/{run_id}/events", get(run::get_run_events))
        .route("/artifacts", get(run::list_artifacts))
        .route("/patches", get(patches::list_patches))
        .route(
            "/patches/validate",
            post(patches::validate_patch).options(response::cors_options),
        )
        .route(
            "/patches/{name}",
            get(patches::get_patch)
                .post(patches::save_patch)
                .delete(patches::remove_patch)
                .options(response::cors_options),
        )
        .route(
            "/workflows",
            get(catalog::list_workflows)
                .post(workflow::save_workflow)
                .options(response::cors_options),
        )
        .route("/workflows/{workflow_id}", get(workflow::get_workflow))
        .route(
            "/workflows/{workflow_id}/dependencies",
            get(workflow::workflow_dependencies),
        )
        .route(
            "/workflows/{workflow_id}/loop",
            get(checks::loop_check_workflow),
        )
        .route(
            "/workflows/{workflow_id}/plan",
            get(workflow::plan_workflow),
        )
        .route(
            "/workflows/{workflow_id}/publish",
            get(workflow::publish_workflow),
        )
        .route(
            "/workflows/{workflow_id}/run",
            post(workflow::run_workflow).options(response::cors_options),
        )
        .route(
            "/workflows/validate",
            post(workflow::validate_workflow).options(response::cors_options),
        )
        .route(
            "/mcp",
            get(mcp::mcp_info)
                .post(mcp::mcp_post)
                .options(response::cors_options),
        )
        .fallback(response::not_found)
        .with_state(types::AppState::new(service))
}
