use super::{request_json, temp_root, write_publishable_project_workflow};
use crate::server::{ApiService, router};

#[tokio::test]
async fn publish_endpoint_can_filter_project_workspaces() {
    let test_root = temp_root("publish-projects");
    let _ = std::fs::remove_dir_all(&test_root);
    write_publishable_project_workflow(&test_root);

    let service = ApiService::new(&test_root);
    let app = router(service);
    let response = request_json(&app, "/publish?project=std").await;
    assert_eq!(response["status"], 200);
    assert_eq!(response["body"]["project"], "std");
    assert_eq!(response["body"]["project_filter_matched"], true);
    assert_eq!(
        response["body"]["matched_project_workspace"],
        "lightflow-std"
    );
    assert_eq!(response["body"]["total"], 1);
    assert_eq!(response["body"]["publishable"], true);
    assert_eq!(
        response["body"]["checks"][0]["workspace"],
        "projects/lightflow-std"
    );
    assert_eq!(
        response["body"]["checks"][0]["workflow_id"],
        "lightflow.http_publish"
    );

    let unknown = request_json(&app, "/publish?project=lightflow-typo").await;
    assert_eq!(unknown["status"], 400);
    assert!(
        unknown["body"]["error"]
            .as_str()
            .expect("publish error")
            .contains("project workspace filter matched no workspace: lightflow-typo"),
        "unknown publish response:\n{unknown}"
    );

    let _ = std::fs::remove_dir_all(test_root);
}
