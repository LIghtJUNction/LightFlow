use crate::api::{ArtifactListOptions, RunListOptions};
use crate::server::{
    response,
    types::{AppState, ArtifactListQuery, RunListQuery},
};
use axum::extract::{Path, Query, State};
use axum::response::Response;

pub(crate) async fn list_runs(
    State(state): State<AppState>,
    Query(query): Query<RunListQuery>,
) -> Response {
    response::api_json(
        run_list_options(query).and_then(|options| state.service.list_runs_with_options(&options)),
    )
}

pub(crate) async fn get_run(State(state): State<AppState>, Path(run_id): Path<String>) -> Response {
    response::api_json(state.service.get_run(&run_id))
}

pub(crate) async fn remove_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Response {
    response::api_json(state.service.remove_run(&run_id))
}

pub(crate) async fn get_run_events(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Response {
    response::api_json(state.service.get_run_events(&run_id))
}

pub(crate) async fn replay_run(
    State(state): State<AppState>,
    Path(run_id): Path<String>,
) -> Response {
    response::api_json(state.service.replay_run(&run_id))
}

pub(crate) async fn list_artifacts(
    State(state): State<AppState>,
    Query(query): Query<ArtifactListQuery>,
) -> Response {
    response::api_json(
        state
            .service
            .list_artifacts_with_options(&ArtifactListOptions {
                limit: query.limit,
                run_id: query.run_id,
                workflow_id: query.workflow_id,
                kind: query.kind,
            }),
    )
}

fn run_list_options(query: RunListQuery) -> Result<RunListOptions, crate::api::ApiError> {
    if let Some(status) = query.status.as_deref()
        && !matches!(status, "completed" | "failed" | "unknown")
    {
        return Err(crate::api::ApiError::InvalidRequest(format!(
            "unsupported run status {status}; expected completed, failed, or unknown"
        )));
    }
    Ok(RunListOptions {
        limit: query.limit,
        workflow_id: query.workflow_id,
        status: query.status,
    })
}
