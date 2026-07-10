use std::env;
use std::ffi::OsString;
use std::sync::Arc;

use tokio::sync::Semaphore;

use super::types::AppState;
use crate::api::{ApiError, ApiResult};

const DEFAULT_MAX_BLOCKING_RUNS: usize = 4;
const MAX_BLOCKING_RUNS: usize = 64;

pub(super) fn configured_semaphore() -> Arc<Semaphore> {
    Arc::new(Semaphore::new(blocking_run_limit(env::var_os(
        "LIGHTFLOW_MAX_BLOCKING_RUNS",
    ))))
}

pub(super) async fn run<T, F>(state: &AppState, task: F) -> ApiResult<T>
where
    T: Send + 'static,
    F: FnOnce() -> ApiResult<T> + Send + 'static,
{
    let permit = Arc::clone(&state.blocking_runs)
        .acquire_owned()
        .await
        .map_err(|_| ApiError::InvalidRequest("blocking run semaphore is closed".to_owned()))?;
    tokio::task::spawn_blocking(move || {
        let _permit = permit;
        task()
    })
    .await
    .map_err(|error| ApiError::InvalidRequest(format!("blocking run task failed: {error}")))?
}

fn blocking_run_limit(value: Option<OsString>) -> usize {
    value
        .and_then(|value| value.into_string().ok())
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| (1..=MAX_BLOCKING_RUNS).contains(value))
        .unwrap_or(DEFAULT_MAX_BLOCKING_RUNS)
}

#[cfg(test)]
mod tests {
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use super::*;
    use crate::api::ApiService;

    #[test]
    fn blocking_limit_defaults_for_missing_or_invalid_values() {
        assert_eq!(blocking_run_limit(None), 4);
        assert_eq!(blocking_run_limit(Some("0".into())), 4);
        assert_eq!(blocking_run_limit(Some("65".into())), 4);
        assert_eq!(blocking_run_limit(Some("invalid".into())), 4);
        assert_eq!(blocking_run_limit(Some("1".into())), 1);
        assert_eq!(blocking_run_limit(Some("64".into())), 64);
    }

    #[tokio::test]
    async fn semaphore_caps_concurrent_blocking_tasks() {
        let state = AppState {
            service: Arc::new(ApiService::new(".")),
            blocking_runs: Arc::new(Semaphore::new(2)),
        };
        let active = Arc::new(AtomicUsize::new(0));
        let maximum = Arc::new(AtomicUsize::new(0));
        let mut tasks = Vec::new();
        for _ in 0..8 {
            let state = state.clone();
            let active = Arc::clone(&active);
            let maximum = Arc::clone(&maximum);
            tasks.push(tokio::spawn(async move {
                run(&state, move || {
                    let now = active.fetch_add(1, Ordering::SeqCst) + 1;
                    maximum.fetch_max(now, Ordering::SeqCst);
                    std::thread::sleep(Duration::from_millis(20));
                    active.fetch_sub(1, Ordering::SeqCst);
                    Ok(())
                })
                .await
            }));
        }
        for task in tasks {
            task.await.expect("task").expect("blocking run");
        }
        assert_eq!(maximum.load(Ordering::SeqCst), 2);
    }
}
