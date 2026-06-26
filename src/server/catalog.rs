use crate::api::{ApiError, ModelListOptions, ModelStatusFilter};
use crate::server::{
    response,
    types::{AppState, ModelListQuery},
};
use axum::extract::{Path, Query, State};
use axum::response::Response;

pub(crate) async fn list_workflows(State(state): State<AppState>) -> Response {
    response::api_json(state.service.list_workflows())
}

pub(crate) async fn list_nodes(State(state): State<AppState>) -> Response {
    response::api_json(state.service.list_nodes())
}

pub(crate) async fn get_node(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
) -> Response {
    response::api_json(state.service.get_node(&workflow_id))
}

pub(crate) async fn list_executors(State(state): State<AppState>) -> Response {
    response::api_json(Ok::<_, ApiError>(state.service.list_executors()))
}

pub(crate) async fn list_models(
    State(state): State<AppState>,
    Query(query): Query<ModelListQuery>,
) -> Response {
    response::api_json(
        model_list_options(query)
            .and_then(|options| state.service.list_models_with_options(&options)),
    )
}

fn model_list_options(query: ModelListQuery) -> Result<ModelListOptions, ApiError> {
    let status = match query.status.as_deref() {
        Some(value) => ModelStatusFilter::parse(value).ok_or_else(|| {
            ApiError::InvalidRequest(format!(
                "unsupported model status {value}; expected all, available, or blocked"
            ))
        })?,
        None => ModelStatusFilter::All,
    };
    Ok(ModelListOptions {
        workflow_id: query.workflow_id,
        status,
    })
}
