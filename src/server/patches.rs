use crate::server::{
    response,
    types::{AppState, PatchValidationQuery},
};
use crate::workflow::WorkflowPatch;
use axum::Json;
use axum::extract::{Path, Query, State};
use axum::response::Response;

pub(crate) async fn list_patches(State(state): State<AppState>) -> Response {
    response::api_json(state.service.list_patches())
}

pub(crate) async fn get_patch(State(state): State<AppState>, Path(name): Path<String>) -> Response {
    response::api_json(state.service.get_patch(&name))
}

pub(crate) async fn save_patch(
    State(state): State<AppState>,
    Path(name): Path<String>,
    Json(patch): Json<WorkflowPatch>,
) -> Response {
    response::api_json(state.service.save_patch(&name, &patch))
}

pub(crate) async fn remove_patch(
    State(state): State<AppState>,
    Path(name): Path<String>,
) -> Response {
    response::api_json(state.service.remove_patch(&name))
}

pub(crate) async fn validate_patch(
    State(state): State<AppState>,
    Query(query): Query<PatchValidationQuery>,
    Json(patch): Json<WorkflowPatch>,
) -> Response {
    let validation = if let Some(workflow_id) = query.workflow_id {
        state
            .service
            .validate_patch_for_workflow(&workflow_id, patch)
    } else {
        state.service.validate_patch(patch)
    };
    response::api_json(Ok::<_, crate::api::ApiError>(validation))
}
