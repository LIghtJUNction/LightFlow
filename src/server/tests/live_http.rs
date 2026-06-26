use super::{
    live_http_endpoints::verify_live_http_endpoints_contracts,
    live_http_history::verify_history_fixture_contracts,
    live_http_patch::{verify_invalid_project_config_contracts, verify_run_and_patch_contracts},
    live_http_release::verify_live_release_contracts,
    temp_root,
};
use crate::server::{ApiService, router};

#[tokio::test]
async fn live_http_responses_match_openapi_required_fields() {
    let openapi = std::fs::read_to_string("openapi/lightflow.yaml").expect("openapi");
    let test_root = temp_root("schema-service");
    let _ = std::fs::remove_dir_all(&test_root);
    std::fs::create_dir_all(&test_root).expect("test root");
    let current_dir = std::env::current_dir().expect("current dir");
    let service = ApiService::new(&test_root)
        .with_workflow_paths(vec![current_dir.join("projects/lightflow-std/workflows")]);
    let app = router(service);

    verify_live_http_endpoints_contracts(&app, &openapi).await;
    verify_live_release_contracts(&app, &openapi, &test_root).await;
    verify_invalid_project_config_contracts(&app, &openapi, &test_root).await;
    verify_run_and_patch_contracts(&app, &openapi).await;

    let _ = std::fs::remove_dir_all(test_root);

    verify_history_fixture_contracts(&openapi).await;
}
