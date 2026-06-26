use crate::api::{ProjectWorkspaceOptions, ReleaseCheckOptions, WorkflowPublishOptions};
use crate::server::{
    response,
    types::{AppState, LoopCheckQuery, LoopProjectsQuery, PublishQuery, ReleaseCheckQuery},
};
use axum::extract::{Path, Query, State};
use axum::response::Response;

pub(crate) async fn loop_check_project(State(state): State<AppState>) -> Response {
    response::api_json(state.service.local_loop_check(None))
}

pub(crate) async fn loop_changes_project(State(state): State<AppState>) -> Response {
    response::api_json(state.service.local_loop_changes())
}

pub(crate) async fn loop_projects(
    State(state): State<AppState>,
    Query(query): Query<LoopProjectsQuery>,
) -> Response {
    response::api_json(
        state
            .service
            .project_workspaces_with_options(ProjectWorkspaceOptions {
                dirty_only: query.dirty || query.changed,
                project: query.project,
            }),
    )
}

pub(crate) async fn release_check_project(
    State(state): State<AppState>,
    Query(query): Query<ReleaseCheckQuery>,
) -> Response {
    response::api_json(
        state.service.release_check(&ReleaseCheckOptions {
            apply: false,
            workflow_id: query
                .workflow_id
                .unwrap_or_else(|| "lightflow.text_plan".to_owned()),
            project: query.project,
            profile: crate::api::CheckProfile::Release,
        }),
    )
}

pub(crate) async fn loop_check_workflow(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
    Query(query): Query<LoopCheckQuery>,
) -> Response {
    response::api_json(state.service.local_loop_check_with_options(
        Some(&workflow_id),
        query.require_replay || query.require_selected_replay,
    ))
}

pub(crate) async fn publish_workflows(
    State(state): State<AppState>,
    Query(query): Query<PublishQuery>,
) -> Response {
    response::api_json(state.service.workflow_publish_checks_with_options(
        &WorkflowPublishOptions {
            project: query.project,
        },
    ))
}
