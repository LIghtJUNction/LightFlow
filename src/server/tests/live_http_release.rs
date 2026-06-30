use super::{assert_required_fields, request_json};

pub(crate) async fn verify_live_release_contracts(
    app: &axum::Router,
    openapi: &str,
    test_root: &std::path::Path,
) {
    let release_for_std = request_json(app, "/release?workflow_id=lightflow.std").await;
    assert_eq!(release_for_std["status"], 200);
    assert_required_fields(openapi, "ReleaseCheckReport", &release_for_std["body"]);
    assert_eq!(release_for_std["body"]["workflow_id"], "lightflow.std");
    assert_eq!(
        release_for_std["body"]["project_root"],
        test_root.display().to_string()
    );
    assert!(
        release_for_std["body"]["checks"]
            .as_array()
            .expect("release checks")
            .iter()
            .any(|check| {
                check["id"] == "release.command.selected_workflow_loop"
                    && check["command"]
                        == serde_json::json!([
                            "cargo",
                            "run",
                            "--bin",
                            "lfw",
                            "--",
                            "loop",
                            "check",
                            "lightflow.std",
                            "--require-replay"
                        ])
            }),
        "release checks:\n{release_for_std}"
    );

    let release_for_project = request_json(
        app,
        "/release?workflow_id=lightflow.std&project=lightflow-std",
    )
    .await;
    assert_eq!(release_for_project["status"], 200);
    assert_required_fields(openapi, "ReleaseCheckReport", &release_for_project["body"]);
    assert_eq!(release_for_project["body"]["workflow_id"], "lightflow.std");
    assert_eq!(release_for_project["body"]["project"], "lightflow-std");
    assert_eq!(release_for_project["body"]["project_config_valid"], true);
    assert_eq!(
        release_for_project["body"].get("project_config_error"),
        None
    );
    assert_eq!(release_for_project["body"]["project_filter_matched"], true);
    assert_eq!(
        release_for_project["body"]["matched_project_workspace"],
        "lightflow-std"
    );
    assert!(
        release_for_project["body"]["known_project_workspaces"]
            .as_array()
            .expect("known project workspaces")
            .iter()
            .any(|name| name == "lightflow-std"),
        "release checks:\n{release_for_project}"
    );
    assert_eq!(
        release_for_project["body"]["known_project_aliases"]["std"],
        "lightflow-std"
    );
    assert!(
        release_for_project["body"]["checks"]
            .as_array()
            .expect("release checks")
            .iter()
            .any(|check| {
                check["id"] == "release.command.project_workspaces"
                    && check["command"]
                        == serde_json::json!([
                            "cargo",
                            "run",
                            "--bin",
                            "lfw",
                            "--",
                            "loop",
                            "projects",
                            "--project",
                            "lightflow-std"
                        ])
            }),
        "release checks:\n{release_for_project}"
    );

    let project_path = test_root.join("projects/lightflow-std");
    let release_for_project_path = request_json(
        app,
        &format!(
            "/release?workflow_id=lightflow.std&project={}",
            project_path.display()
        ),
    )
    .await;
    assert_eq!(release_for_project_path["status"], 200);
    assert_required_fields(
        openapi,
        "ReleaseCheckReport",
        &release_for_project_path["body"],
    );
    assert_eq!(
        release_for_project_path["body"]["project"],
        project_path.display().to_string()
    );
    assert_eq!(
        release_for_project_path["body"]["project_filter_matched"],
        true
    );
    assert_eq!(
        release_for_project_path["body"]["matched_project_workspace"],
        "lightflow-std"
    );
    assert!(
        release_for_project_path["body"]["checks"]
            .as_array()
            .expect("release checks")
            .iter()
            .any(|check| {
                check["id"] == "release.command.project_workspaces"
                    && check["command"]
                        == serde_json::json!([
                            "cargo",
                            "run",
                            "--bin",
                            "lfw",
                            "--",
                            "loop",
                            "projects",
                            "--project",
                            project_path.display().to_string()
                        ])
            }),
        "release checks:\n{release_for_project_path}"
    );

    let release_for_relative_project_path = request_json(
        app,
        "/release?workflow_id=lightflow.std&project=./projects/lightflow-std",
    )
    .await;
    assert_eq!(release_for_relative_project_path["status"], 200);
    assert_required_fields(
        openapi,
        "ReleaseCheckReport",
        &release_for_relative_project_path["body"],
    );
    assert_eq!(
        release_for_relative_project_path["body"]["project"],
        "./projects/lightflow-std"
    );
    assert_eq!(
        release_for_relative_project_path["body"]["project_filter_matched"],
        true
    );
    assert_eq!(
        release_for_relative_project_path["body"]["matched_project_workspace"],
        "lightflow-std"
    );
    assert!(
        release_for_relative_project_path["body"]["checks"]
            .as_array()
            .expect("release checks")
            .iter()
            .any(|check| {
                check["id"] == "release.command.project_workspaces"
                    && check["command"]
                        == serde_json::json!([
                            "cargo",
                            "run",
                            "--bin",
                            "lfw",
                            "--",
                            "loop",
                            "projects",
                            "--project",
                            "./projects/lightflow-std"
                        ])
            }),
        "release checks:\n{release_for_relative_project_path}"
    );

    let release_for_unknown_project = request_json(
        app,
        "/release?workflow_id=lightflow.std&project=lightflow-typo",
    )
    .await;
    assert_eq!(release_for_unknown_project["status"], 200);
    assert_required_fields(
        openapi,
        "ReleaseCheckReport",
        &release_for_unknown_project["body"],
    );
    assert_eq!(release_for_unknown_project["body"]["valid"], false);
    assert_eq!(
        release_for_unknown_project["body"]["project"],
        "lightflow-typo"
    );
    assert_eq!(
        release_for_unknown_project["body"]["project_filter_matched"],
        false
    );
    assert_eq!(
        release_for_unknown_project["body"].get("matched_project_workspace"),
        None
    );
    assert!(
        release_for_unknown_project["body"]["known_project_workspaces"]
            .as_array()
            .expect("known project workspaces")
            .iter()
            .any(|name| name == "lightflow-std"),
        "release checks:\n{release_for_unknown_project}"
    );
    assert_eq!(
        release_for_unknown_project["body"]["known_project_aliases"]["std"],
        "lightflow-std"
    );
    assert!(
            release_for_unknown_project["body"]["issues"]
                .as_array()
                .expect("release issues")
                .iter()
                .any(|issue| issue.as_str().is_some_and(|issue| {
                    issue.contains(
                        "project workspace catalog is unavailable for requested project filter: lightflow-typo",
                    )
                })),
            "release checks:\n{release_for_unknown_project}"
        );
    assert!(
        release_for_unknown_project["body"]["checks"]
            .as_array()
            .expect("release checks")
            .iter()
            .any(|check| {
                check["id"] == "release.review.project_workspaces" && check["status"] == "failed"
            }),
        "release checks:\n{release_for_unknown_project}"
    );
}
