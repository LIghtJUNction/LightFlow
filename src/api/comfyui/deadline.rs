use std::time::{Duration, Instant};

use crate::api::{ApiError, ApiResult};

#[derive(Debug, Clone)]
pub(super) struct Deadline {
    started: Instant,
    timeout: Duration,
}

impl Deadline {
    pub(super) fn new(timeout: Duration) -> Self {
        Self {
            started: Instant::now(),
            timeout,
        }
    }

    pub(super) fn remaining(&self, action: &str) -> ApiResult<Duration> {
        let remaining = self.timeout.saturating_sub(self.started.elapsed());
        if remaining.is_zero() {
            return self.exceeded(action);
        }
        Ok(remaining)
    }

    pub(super) fn check(&self, action: &str) -> ApiResult<()> {
        self.remaining(action).map(|_| ())
    }

    pub(super) fn exceeded<T>(&self, action: &str) -> ApiResult<T> {
        Err(self.error(action))
    }

    pub(super) fn error(&self, action: &str) -> ApiError {
        ApiError::InvalidRequest(format!(
            "ComfyUI {action} exceeded total timeout of {}ms",
            self.timeout.as_millis()
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deadline_above_thirty_seconds_is_not_capped() {
        let deadline = Deadline::new(Duration::from_secs(45));
        assert!(deadline.remaining("test").expect("remaining") > Duration::from_secs(44));
    }
}
