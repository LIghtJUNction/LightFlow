use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use axum::http::StatusCode;
use tokio::sync::Semaphore;

use super::request_json;
use crate::api::ApiService;
use crate::server::{blocking, router, types::AppState};

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn long_comfy_run_does_not_block_health() {
    let service = Arc::new(ApiService::new("."));
    let state = AppState {
        service: Arc::clone(&service),
        blocking_runs: Arc::new(Semaphore::new(1)),
    };
    let app = router((*service).clone());
    let blocking_state = state.clone();
    let started = Instant::now();
    let run = tokio::spawn(async move {
        blocking::run(&blocking_state, || {
            thread::sleep(Duration::from_millis(250));
            Ok(())
        })
        .await
    });
    tokio::time::sleep(Duration::from_millis(15)).await;

    let health = request_json(&app, "/health").await;

    assert_eq!(health["status"], StatusCode::OK.as_u16());
    assert!(
        started.elapsed() < Duration::from_millis(120),
        "health waited behind blocking run: {:?}",
        started.elapsed()
    );
    run.await.expect("run task").expect("blocking run");
}
