use super::{assert_required_fields, request_json, request_json_delete, request_json_post};

pub(crate) async fn verify_invalid_project_config_contracts(
    app: &axum::Router,
    openapi: &str,
    test_root: &std::path::Path,
) {
    std::fs::create_dir_all(test_root.join("projects")).expect("project config dir");
    std::fs::write(
        test_root.join("projects/lightflow-projects.toml"),
        "[workspaces]\nexpected = [\"../broken\"]\n",
    )
    .expect("invalid project config");
    let release_for_invalid_project_config =
        request_json(app, "/release?workflow_id=lightflow.std").await;
    assert_eq!(release_for_invalid_project_config["status"], 200);
    assert_required_fields(
        openapi,
        "ReleaseCheckReport",
        &release_for_invalid_project_config["body"],
    );
    assert_eq!(
        release_for_invalid_project_config["body"]["project_config_present"],
        true
    );
    assert_eq!(
        release_for_invalid_project_config["body"]["project_config_valid"],
        false
    );
    assert!(
        release_for_invalid_project_config["body"]["project_config_error"]
            .as_str()
            .expect("project config error")
            .contains("entries must be project directory names"),
        "release checks:\n{release_for_invalid_project_config}"
    );
    assert_eq!(
        release_for_invalid_project_config["body"]["project_config_write_command"],
        serde_json::json!(["lfw", "dev", "project-config-template", "--write"])
    );
    assert_eq!(
        release_for_invalid_project_config["body"]["project_submodule_update_command"],
        serde_json::json!([
            "git",
            "submodule",
            "update",
            "--init",
            "--recursive",
            "projects/lightflow-flux",
            "projects/lightflow-rig",
            "projects/lightflow-std"
        ])
    );
    assert!(
        release_for_invalid_project_config["body"]["checks"]
            .as_array()
            .expect("release checks")
            .iter()
            .any(|check| {
                check["id"] == "release.review.project_workspaces" && check["status"] == "failed"
            }),
        "release checks:\n{release_for_invalid_project_config}"
    );
    std::fs::remove_file(test_root.join("projects/lightflow-projects.toml"))
        .expect("remove invalid project config");
}

pub(crate) async fn verify_run_and_patch_contracts(app: &axum::Router, openapi: &str) {
    let run = request_json_post(
        app,
        "/workflows/lightflow.text_plan/run",
        serde_json::json!({ "inputs": { "value": "hello" } }),
    )
    .await;
    assert_eq!(run["status"], 200);
    assert_required_fields(openapi, "RecordedWorkflowExecution", &run["body"]);
    assert!(run["body"]["run_id"].as_str().is_some());

    let patched_run = request_json_post(
        app,
        "/workflows/lightflow.text_plan/run",
        serde_json::json!({
            "inputs": { "value": "patched" },
            "enabled_nodes": ["identity"],
            "patch": {
                "nodes": {
                    "identity": {
                        "replace_with": "lightflow.std",
                        "timeout_ms": 1000
                    }
                }
            }
        }),
    )
    .await;
    assert_eq!(patched_run["status"], 200);
    let patched_run_id = patched_run["body"]["run_id"]
        .as_str()
        .expect("patched run id");
    let patched_trace = request_json(app, &format!("/runs/{patched_run_id}")).await;
    assert_eq!(patched_trace["status"], 200);
    assert_eq!(
        patched_trace["body"]["manifest"]["stages"][0]["execution"]["enabled_nodes"],
        serde_json::json!(["identity"])
    );
    assert_eq!(
        patched_trace["body"]["manifest"]["stages"][0]["execution"]["patch"]["nodes"]["identity"]["replace_with"],
        "lightflow.std"
    );
    assert_eq!(
        patched_trace["body"]["manifest"]["stages"][0]["execution"]["patch"]["nodes"]["identity"]["timeout_ms"],
        1000
    );
    let removed_run = request_json_delete(app, &format!("/runs/{patched_run_id}")).await;
    assert_eq!(removed_run["status"], 200);
    assert_required_fields(openapi, "RemovedRun", &removed_run["body"]);
    assert_eq!(removed_run["body"]["removed"], true);
    assert_eq!(removed_run["body"]["run_id"], patched_run_id);

    let saved_patch = request_json_post(
        app,
        "/patches/qa-debug",
        serde_json::json!({
            "nodes": {
                "identity": {
                    "retry": 2,
                    "timeout_ms": 500
                }
            }
        }),
    )
    .await;
    assert_eq!(saved_patch["status"], 200);
    assert_required_fields(openapi, "SavedPatch", &saved_patch["body"]);
    assert_eq!(saved_patch["body"]["name"], "qa-debug");

    let patch_catalog = request_json(app, "/patches").await;
    assert_eq!(patch_catalog["status"], 200);
    assert_required_fields(openapi, "PatchCatalog", &patch_catalog["body"]);
    assert_eq!(patch_catalog["body"]["patches"][0]["name"], "qa-debug");

    let registered_patch = request_json(app, "/patches/qa-debug").await;
    assert_eq!(registered_patch["status"], 200);
    assert_required_fields(openapi, "RegisteredPatch", &registered_patch["body"]);
    assert_eq!(
        registered_patch["body"]["patch"]["nodes"]["identity"]["retry"],
        2
    );

    let patch_validation = request_json_post(
        app,
        "/patches/validate",
        serde_json::json!({
            "nodes": { "identity": { "enable": true } }
        }),
    )
    .await;
    assert_eq!(patch_validation["status"], 200);
    assert_required_fields(openapi, "PatchValidation", &patch_validation["body"]);
    assert_eq!(patch_validation["body"]["valid"], true);

    let selected_patch_validation = request_json_post(
        app,
        "/patches/validate?workflow_id=lightflow.text_plan",
        serde_json::json!({
            "nodes": { "missing": { "disable": true } }
        }),
    )
    .await;
    assert_eq!(selected_patch_validation["status"], 200);
    assert_required_fields(
        openapi,
        "PatchValidation",
        &selected_patch_validation["body"],
    );
    assert_eq!(selected_patch_validation["body"]["valid"], false);
    assert!(
        selected_patch_validation["body"]["issues"]
            .as_array()
            .expect("selected patch issues")
            .iter()
            .any(|issue| issue.as_str().expect("issue").contains(
                "patch node missing does not match any node in workflow lightflow.text_plan",
            ))
    );

    let removed_patch = request_json_delete(app, "/patches/qa-debug").await;
    assert_eq!(removed_patch["status"], 200);
    assert_required_fields(openapi, "RemovedPatch", &removed_patch["body"]);
    assert_eq!(removed_patch["body"]["removed"], true);

    let missing = request_json(app, "/workflows/lightflow.missing").await;
    assert_eq!(missing["status"], 404);
    assert_required_fields(openapi, "ErrorResponse", &missing["body"]);
}
