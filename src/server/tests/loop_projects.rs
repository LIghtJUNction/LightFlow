use super::{git_ok, request_json, temp_root};
use crate::server::{ApiService, router};

#[tokio::test]
async fn loop_projects_endpoint_can_filter_dirty_workspaces() {
    let test_root = temp_root("dirty-projects");
    let _ = std::fs::remove_dir_all(&test_root);
    let std = test_root.join("projects/lightflow-std");
    std::fs::create_dir_all(&std).expect("std project");
    std::fs::write(std.join("README.md"), "# std\n").expect("readme");
    git_ok(&std, ["init"]);
    git_ok(&std, ["add", "."]);
    git_ok(
        &std,
        [
            "-c",
            "user.name=LightFlow Test",
            "-c",
            "user.email=lightflow@example.test",
            "commit",
            "-m",
            "initial std",
        ],
    );
    std::fs::write(std.join("README.md"), "# dirty std\n").expect("dirty readme");

    let service = ApiService::new(&test_root);
    let app = router(service);
    let response = request_json(&app, "/loop/projects?dirty=true&project=lightflow-std").await;
    assert_eq!(response["status"], 200);
    assert_eq!(response["body"]["present_count"], 1);
    assert_eq!(response["body"]["workspaces"][0]["name"], "lightflow-std");
    assert_eq!(
        response["body"]["workspaces"][0]["git_changed_paths"],
        serde_json::json!(["README.md"])
    );

    let relative_path = request_json(
        &app,
        "/loop/projects?dirty=true&project=./projects/lightflow-std",
    )
    .await;
    assert_eq!(relative_path["status"], 200);
    assert_eq!(relative_path["body"]["project_filter_matched"], true);
    assert_eq!(
        relative_path["body"]["matched_project_workspace"],
        "lightflow-std"
    );
    assert_eq!(relative_path["body"]["present_count"], 1);
    assert_eq!(
        relative_path["body"]["workspaces"][0]["name"],
        "lightflow-std"
    );

    let unknown = request_json(&app, "/loop/projects?project=lightflow-typo").await;
    assert_eq!(unknown["status"], 200);
    assert_eq!(unknown["body"]["valid"], false);
    assert!(
        unknown["body"]["issues"]
            .as_array()
            .expect("unknown project issues")
            .iter()
            .any(|issue| issue.as_str().is_some_and(|issue| {
                issue.contains("project workspace filter matched no workspace: lightflow-typo")
                    && issue.contains("known aliases:")
                    && issue.contains("std=lightflow-std")
            })),
        "unknown project response:\n{unknown}"
    );

    std::fs::write(
        test_root.join("projects/lightflow-projects.toml"),
        "[workspaces]\nexpected = [\"../broken\"]\n",
    )
    .expect("invalid project config");
    let invalid_config = request_json(&app, "/loop/projects").await;
    assert_eq!(invalid_config["status"], 200);
    assert_eq!(invalid_config["body"]["valid"], false);
    assert_eq!(invalid_config["body"]["project_config_present"], true);
    assert_eq!(invalid_config["body"]["project_config_valid"], false);
    assert!(
        invalid_config["body"]["project_config_error"]
            .as_str()
            .expect("project config error")
            .contains("entries must be project directory names"),
        "invalid config response:\n{invalid_config}"
    );
    assert_eq!(
        invalid_config["body"]["project_config_write_command"],
        serde_json::json!(["lfw", "dev", "project-config-template", "--write"])
    );
    assert_eq!(
        invalid_config["body"]["project_submodule_update_command"],
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
        invalid_config["body"]["issues"]
            .as_array()
            .expect("invalid config issues")
            .iter()
            .any(|issue| issue.as_str().is_some_and(|issue| {
                issue.contains("project config invalid") && issue.contains("../broken")
            })),
        "invalid config response:\n{invalid_config}"
    );

    let _ = std::fs::remove_dir_all(test_root);
}
