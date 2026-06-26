use crate::api::ApiError;
use crate::server::{response, types::AppState};
use crate::workflow::{WorkflowExecutionOptions, WorkflowSpec};
use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Response;

pub(crate) async fn get_workflow(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
) -> Response {
    response::api_json(state.service.get_workflow(&workflow_id))
}

pub(crate) async fn workflow_dependencies(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
) -> Response {
    response::api_json(state.service.workflow_dependencies(&workflow_id))
}

pub(crate) async fn plan_workflow(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
) -> Response {
    response::api_json(state.service.plan_workflow(&workflow_id))
}

pub(crate) async fn publish_workflow(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
) -> Response {
    response::api_json(state.service.workflow_publish_check(&workflow_id))
}

pub(crate) async fn run_workflow(
    State(state): State<AppState>,
    Path(workflow_id): Path<String>,
    Json(options): Json<WorkflowExecutionOptions>,
) -> Response {
    let started_at_ms = crate::api::ApiService::now_ms();
    let result = state
        .service
        .execute_workflow(&workflow_id, options.clone());
    let completed_at_ms = crate::api::ApiService::now_ms();
    match result {
        Ok(execution) => {
            let mut value = match state.service.execution_with_model_locks(&execution) {
                Ok(value) => value,
                Err(error) => return response::error_response(error),
            };
            let record = state.service.record_completed_workflow_run(
                &workflow_id,
                &options,
                &value,
                started_at_ms,
                completed_at_ms,
            );
            match record {
                Ok(record) => {
                    let Some(object) = value.as_object_mut() else {
                        return response::error_response(ApiError::InvalidRequest(
                            "workflow execution output must be a JSON object".to_owned(),
                        ));
                    };
                    object.insert("run_id".to_owned(), record.run_id.into());
                    object.insert(
                        "run_dir".to_owned(),
                        record.run_dir.display().to_string().into(),
                    );
                    object.insert(
                        "trace_path".to_owned(),
                        record
                            .run_dir
                            .join("execution.json")
                            .display()
                            .to_string()
                            .into(),
                    );
                    response::api_json(Ok::<_, ApiError>(value))
                }
                Err(error) => response::error_response(error),
            }
        }
        Err(error) => {
            let error_json = serde_json::json!({
                "code": error.code(),
                "message": error.message(),
            });
            let record = match state.service.record_failed_workflow_run(
                &workflow_id,
                &options,
                &error_json,
                started_at_ms,
                completed_at_ms,
            ) {
                Ok(record) => record,
                Err(record_error) => return response::error_response(record_error),
            };
            response::error_response_with_run(
                error,
                &record.run_id,
                record.run_dir.display().to_string(),
            )
        }
    }
}

pub(crate) async fn save_workflow(
    State(state): State<AppState>,
    Json(workflow): Json<WorkflowSpec>,
) -> Response {
    response::api_json_with_status(StatusCode::CREATED, state.service.save_workflow(workflow))
}

pub(crate) async fn validate_workflow(
    State(state): State<AppState>,
    Json(workflow): Json<WorkflowSpec>,
) -> Response {
    response::api_json(Ok::<_, ApiError>(
        state.service.validate_workflow(&workflow),
    ))
}
