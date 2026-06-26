use super::{assert_required_fields, request_json};
use crate::server::{ApiService, router};

pub(crate) async fn verify_history_fixture_contracts(openapi: &str) {
    let root = std::env::temp_dir().join(format!(
        "lightflow-server-schema-history-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&root);
    std::fs::create_dir_all(&root).expect("root");
    crate::api::write_history_fixture(&root).expect("fixture");
    let history_app = router(ApiService::new(&root));

    for (path, schema) in [
        ("/runs", "RunCatalog"),
        ("/runs/last", "RunTrace"),
        ("/runs/run-test/events", "RunEvents"),
        ("/artifacts", "ArtifactCatalog"),
    ] {
        let response = request_json(&history_app, path).await;
        assert_eq!(response["status"], 200, "{path}");
        assert_required_fields(openapi, schema, &response["body"]);
    }

    let _ = std::fs::remove_dir_all(root);
}
