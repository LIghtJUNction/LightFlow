use axum::body::{Body, to_bytes};
use axum::http::{HeaderValue, Method, Request, StatusCode, header};
use tower::ServiceExt;

use crate::server::types::HTTP_PATHS;
use crate::server::{ApiService, router};

use super::{assert_openapi_parameter_enum, assert_schema_property, openapi_path_keys};

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

#[tokio::test]
async fn openapi_documents_patchable_workflow_run_options() {
    let openapi = std::fs::read_to_string("openapi/lightflow.yaml").expect("openapi");
    assert_schema_property(&openapi, "WorkflowExecutionOptions", "inputs");
    assert_schema_property(&openapi, "WorkflowExecutionOptions", "disabled_nodes");
    assert_schema_property(&openapi, "WorkflowExecutionOptions", "enabled_nodes");
    assert_schema_property(&openapi, "WorkflowExecutionOptions", "patch");
    assert_schema_property(&openapi, "WorkflowPatch", "nodes");
    assert_schema_property(&openapi, "WorkflowNodePatch", "disable");
    assert_schema_property(&openapi, "WorkflowNodePatch", "enable");
    assert_schema_property(&openapi, "WorkflowNodePatch", "replace_with");
    assert_schema_property(&openapi, "WorkflowNodePatch", "fallback_workflow_id");
    assert_schema_property(&openapi, "WorkflowNodePatch", "retry");
    assert_schema_property(&openapi, "WorkflowNodePatch", "timeout_ms");
    assert_schema_property(&openapi, "WorkflowExecution", "runtime");
    assert_schema_property(&openapi, "NodeExecution", "runtime");
    assert_schema_property(&openapi, "ExecutionRuntime", "executor_id");
    assert_schema_property(&openapi, "ExecutionRuntime", "data_policy");
    assert_schema_property(&openapi, "ExecutorInfo", "status_reason");
    assert_schema_property(&openapi, "ExecutorInfo", "data_policy");
    assert_schema_property(&openapi, "ExecutorInfo", "plans_models");
    assert_schema_property(&openapi, "RunCatalog", "issues");
    assert_schema_property(&openapi, "RunSummary", "workflow_ids");
    assert_schema_property(&openapi, "RunSummary", "duration_ms");
    assert_schema_property(&openapi, "RunSummary", "surface");
    assert_schema_property(&openapi, "RunEvent", "runtime");
    assert_schema_property(&openapi, "RunEvent", "artifacts");
    assert_schema_property(&openapi, "RunTrace", "manifest");
    assert_schema_property(&openapi, "RunTrace", "execution");
    assert_schema_property(&openapi, "RunManifest", "stage_input_resolution");
    assert_schema_property(&openapi, "RunStageRecord", "execution");
    assert_schema_property(&openapi, "RunCatalog", "total");
    assert_schema_property(&openapi, "RunCatalog", "completed_count");
    assert_schema_property(&openapi, "RunCatalog", "failed_count");
    assert_schema_property(&openapi, "RunCatalog", "unknown_count");
    assert_schema_property(&openapi, "RunCatalog", "unknown_run_ids");
    assert_openapi_parameter_enum(
        &openapi,
        "/runs",
        "status",
        &["completed", "failed", "unknown"],
    );
    assert_openapi_parameter_enum(
        &openapi,
        "/models",
        "status",
        &["all", "available", "blocked"],
    );
    assert_schema_property(&openapi, "FailedRunExecution", "partial_execution");
    assert_schema_property(&openapi, "FailedRunExecution", "stages");
    assert_schema_property(&openapi, "PipelineExecution", "stages");
    assert_schema_property(&openapi, "PipelineExecution", "model_locks");
    assert_schema_property(&openapi, "ReplayReport", "runtime_changed");
    assert_schema_property(&openapi, "ReplayRuntimeFingerprint", "runtime");
    assert_schema_property(&openapi, "RunArtifact", "stage_index");
    assert_schema_property(&openapi, "RunArtifact", "node_index");
    assert_schema_property(&openapi, "WorkflowPlan", "runtime");
    assert_schema_property(&openapi, "WorkflowRuntimePlan", "executor_status");
    assert_schema_property(&openapi, "WorkflowRuntimePlan", "models");
    assert_schema_property(&openapi, "ModelCatalog", "total");
    assert_schema_property(&openapi, "ModelCatalog", "available_count");
    assert_schema_property(&openapi, "ModelCatalog", "blocked_count");
    assert_schema_property(&openapi, "ModelCatalog", "issues");
    assert_schema_property(&openapi, "NodeModelCard", "sync_command");
    assert_schema_property(&openapi, "NodeModelCard", "verify_command");
    assert_schema_property(&openapi, "RemovedRun", "removed");
    assert_schema_property(&openapi, "LocalLoopReport", "checks");
    assert_schema_property(&openapi, "LocalLoopReport", "issues");
    assert_schema_property(&openapi, "LocalLoopReport", "warning_messages");
    assert_schema_property(&openapi, "LocalLoopReport", "project_config_path");
    assert_schema_property(&openapi, "LocalLoopReport", "project_config_present");
    assert_schema_property(&openapi, "LocalLoopReport", "project_config_valid");
    assert_schema_property(&openapi, "LocalLoopReport", "project_config_error");
    assert_schema_property(
        &openapi,
        "LocalLoopReport",
        "project_config_template_command",
    );
    assert_schema_property(&openapi, "LocalLoopReport", "project_config_write_command");
    assert_schema_property(
        &openapi,
        "LocalLoopReport",
        "project_submodule_update_command",
    );
    assert_schema_property(&openapi, "LocalLoopReport", "replay_run_id");
    assert_schema_property(&openapi, "LocalLoopReport", "passed");
    assert_schema_property(&openapi, "LocalLoopReport", "warnings");
    assert_schema_property(&openapi, "LocalLoopReport", "failed");
    assert_schema_property(&openapi, "LocalLoopCheck", "status");
    assert_schema_property(&openapi, "LocalLoopCheck", "details");
    assert_schema_property(&openapi, "LoopChangesReport", "warning_messages");
    assert_schema_property(&openapi, "ProjectWorkspaceCatalog", "workspaces");
    assert_schema_property(&openapi, "ProjectWorkspaceCatalog", "linked_count");
    assert_schema_property(&openapi, "ProjectWorkspaceCatalog", "optional_count");
    assert_schema_property(&openapi, "ProjectWorkspaceCatalog", "workflow_crate_count");
    assert_schema_property(&openapi, "ProjectWorkspaceCatalog", "directory_count");
    assert_schema_property(&openapi, "ProjectWorkspaceCatalog", "symlink_count");
    assert_schema_property(&openapi, "ProjectWorkspaceCatalog", "submodule_count");
    assert_schema_property(&openapi, "ProjectWorkspaceCatalog", "project_config_path");
    assert_schema_property(
        &openapi,
        "ProjectWorkspaceCatalog",
        "project_config_present",
    );
    assert_schema_property(&openapi, "ProjectWorkspaceCatalog", "project_config_valid");
    assert_schema_property(&openapi, "ProjectWorkspaceCatalog", "project_config_error");
    assert_schema_property(
        &openapi,
        "ProjectWorkspaceCatalog",
        "project_config_template_command",
    );
    assert_schema_property(
        &openapi,
        "ProjectWorkspaceCatalog",
        "project_config_write_command",
    );
    assert_schema_property(
        &openapi,
        "ProjectWorkspaceCatalog",
        "project_submodule_update_command",
    );
    assert_schema_property(&openapi, "ProjectWorkspaceCatalog", "project_filter");
    assert_schema_property(
        &openapi,
        "ProjectWorkspaceCatalog",
        "project_filter_matched",
    );
    assert_schema_property(
        &openapi,
        "ProjectWorkspaceCatalog",
        "matched_project_workspace",
    );
    assert_schema_property(&openapi, "ProjectWorkspaceCatalog", "dirty_filter");
    assert_schema_property(&openapi, "ProjectWorkspaceCatalog", "known_workspace_names");
    assert_schema_property(
        &openapi,
        "ProjectWorkspaceCatalog",
        "known_workspace_aliases",
    );
    assert_schema_property(
        &openapi,
        "ProjectWorkspaceCatalog",
        "known_project_workspaces",
    );
    assert_schema_property(&openapi, "ProjectWorkspaceCatalog", "known_project_aliases");
    assert_schema_property(
        &openapi,
        "ProjectWorkspaceCatalog",
        "known_optional_workspace_names",
    );
    assert_schema_property(
        &openapi,
        "ProjectWorkspaceCatalog",
        "optional_workspace_names",
    );
    assert_schema_property(
        &openapi,
        "ProjectWorkspaceCatalog",
        "default_workflow_sources",
    );
    assert_schema_property(&openapi, "ProjectWorkspaceSummary", "target");
    assert_schema_property(&openapi, "ProjectWorkspaceSummary", "aliases");
    assert_schema_property(&openapi, "ProjectWorkspaceSummary", "optional");
    assert_schema_property(&openapi, "ProjectWorkspaceSummary", "resolved_path");
    assert_schema_property(&openapi, "ProjectWorkspaceSummary", "git_dirty");
    assert_schema_property(&openapi, "ProjectWorkspaceSummary", "git_changed_count");
    assert_schema_property(&openapi, "ProjectWorkspaceSummary", "git_changed_paths");
    assert_schema_property(&openapi, "ProjectWorkspaceSummary", "git_branch");
    assert_schema_property(&openapi, "ProjectWorkspaceSummary", "git_upstream");
    assert_schema_property(&openapi, "ProjectWorkspaceSummary", "git_remote_url");
    assert_schema_property(&openapi, "ProjectWorkspaceSummary", "git_head");
    assert_schema_property(&openapi, "ProjectWorkspaceSummary", "parent_gitlink_head");
    assert_schema_property(&openapi, "ProjectWorkspaceSummary", "git_stage_command");
    assert_schema_property(&openapi, "ProjectWorkspaceSummary", "git_commit_command");
    assert_schema_property(&openapi, "ProjectWorkspaceSummary", "git_push_command");
    assert_schema_property(
        &openapi,
        "ProjectWorkspaceSummary",
        "parent_gitlink_changed",
    );
    assert_schema_property(&openapi, "ProjectWorkspaceSummary", "git_status_command");
    assert_schema_property(
        &openapi,
        "ProjectWorkspaceSummary",
        "parent_gitlink_stage_command",
    );
    assert_schema_property(&openapi, "ProjectWorkspaceSummary", "git_status_error");
    assert_schema_property(&openapi, "LoopChangesReport", "issues");
    assert_schema_property(&openapi, "LoopChangesReport", "blockers");
    assert_schema_property(&openapi, "LoopChangesReport", "passed");
    assert_schema_property(&openapi, "LoopChangesReport", "warnings");
    assert_schema_property(&openapi, "LoopChangesReport", "failed");
    assert_schema_property(&openapi, "LoopChangesReport", "changed_workflows");
    assert_schema_property(&openapi, "WorkflowChangeSummary", "skill_paths");
    assert_schema_property(&openapi, "ReleaseCheckReport", "checks");
    assert_schema_property(&openapi, "ReleaseCheckReport", "project_root");
    assert_schema_property(&openapi, "ReleaseCheckReport", "workflow_id");
    assert_schema_property(&openapi, "ReleaseCheckReport", "project_config_path");
    assert_schema_property(&openapi, "ReleaseCheckReport", "project_config_present");
    assert_schema_property(&openapi, "ReleaseCheckReport", "project_config_valid");
    assert_schema_property(&openapi, "ReleaseCheckReport", "project_config_error");
    assert_schema_property(
        &openapi,
        "ReleaseCheckReport",
        "project_config_template_command",
    );
    assert_schema_property(
        &openapi,
        "ReleaseCheckReport",
        "project_config_write_command",
    );
    assert_schema_property(
        &openapi,
        "ReleaseCheckReport",
        "project_submodule_update_command",
    );
    assert_schema_property(&openapi, "ReleaseCheckReport", "default_workflow_sources");
    assert_schema_property(
        &openapi,
        "ReleaseCheckReport",
        "known_optional_workspace_names",
    );
    assert_schema_property(&openapi, "ReleaseCheckReport", "project");
    assert_schema_property(&openapi, "ReleaseCheckReport", "project_filter_matched");
    assert_schema_property(&openapi, "ReleaseCheckReport", "matched_project_workspace");
    assert_schema_property(&openapi, "ReleaseCheckReport", "known_project_workspaces");
    assert_schema_property(&openapi, "ReleaseCheckReport", "known_project_aliases");
    assert_schema_property(&openapi, "ReleaseCheckReport", "issues");
    assert_schema_property(&openapi, "ReleaseCheckReport", "warnings");
    assert_schema_property(&openapi, "ReleaseCheckReport", "passed");
    assert_schema_property(&openapi, "ReleaseCheckReport", "warning_count");
    assert_schema_property(&openapi, "ReleaseCheckReport", "failed");
    assert_schema_property(&openapi, "ReleaseCheckReport", "planned");
    assert_schema_property(&openapi, "ReleaseCheckReport", "skipped");
    assert_schema_property(&openapi, "ReleaseCheck", "command");
    assert_schema_property(&openapi, "ReleaseCheck", "count");
    assert_schema_property(&openapi, "ReleaseCheck", "details");
    assert_schema_property(&openapi, "ReleaseCheck", "stdout_tail");
    assert_schema_property(&openapi, "WorkflowPublishCatalog", "checks");
    assert_schema_property(&openapi, "WorkflowPublishCatalog", "issues");
    assert_schema_property(&openapi, "WorkflowPublishCatalog", "project");
    assert_schema_property(&openapi, "WorkflowPublishCatalog", "project_filter_matched");
    assert_schema_property(
        &openapi,
        "WorkflowPublishCatalog",
        "matched_project_workspace",
    );
    assert_schema_property(&openapi, "WorkflowPublishCatalog", "total");
    assert_schema_property(&openapi, "WorkflowPublishCatalog", "publishable_count");
    assert_schema_property(&openapi, "WorkflowPublishCatalog", "blocked_count");
    assert_schema_property(&openapi, "WorkflowPublishCatalog", "commands");
    assert_schema_property(&openapi, "WorkflowPublishCheck", "workspace");
    assert_schema_property(&openapi, "PatchValidation", "issues");
}
