//! Lightweight runtime trace hooks for code-first workflows.

use std::env;
use std::time::{Duration, Instant};

/// Record node start and return the timestamp used by end/error events.
pub fn node_start(name: &str) -> Instant {
    let now = Instant::now();
    if trace_enabled() {
        eprintln!(
            "{{\"event\":\"node_start\",\"node\":{}}}",
            serde_json::to_string(name).unwrap_or_else(|_| "\"<invalid>\"".to_owned())
        );
    }
    now
}

/// Record successful node completion.
pub fn node_end(name: &str, started_at: Instant) {
    if trace_enabled() {
        trace_duration("node_end", name, started_at.elapsed(), None);
    }
}

/// Record node failure.
pub fn node_error(name: &str, started_at: Instant, error: &anyhow::Error) {
    if trace_enabled() {
        trace_duration("node_error", name, started_at.elapsed(), Some(error));
    }
}

/// Record that a typed node was disabled and routed to its fallback.
pub fn node_disabled(name: &str) {
    if trace_enabled() {
        eprintln!(
            "{{\"event\":\"node_disabled\",\"node\":{}}}",
            serde_json::to_string(name).unwrap_or_else(|_| "\"<invalid>\"".to_owned())
        );
    }
}

/// Whether a typed node should use its disabled fallback.
pub fn node_is_disabled(name: &str) -> bool {
    env::var("LIGHTFLOW_DISABLED_NODES")
        .ok()
        .is_some_and(|value| {
            value
                .split(',')
                .map(str::trim)
                .any(|candidate| candidate == name)
        })
}

fn trace_enabled() -> bool {
    env::var("LIGHTFLOW_TRACE")
        .ok()
        .is_some_and(|value| matches!(value.as_str(), "1" | "true" | "yes" | "on"))
}

fn trace_duration(event: &str, name: &str, duration: Duration, error: Option<&anyhow::Error>) {
    let node = serde_json::to_string(name).unwrap_or_else(|_| "\"<invalid>\"".to_owned());
    let error = error
        .map(|error| {
            format!(
                ",\"error\":{}",
                serde_json::to_string(&error.to_string())
                    .unwrap_or_else(|_| "\"<invalid>\"".to_owned())
            )
        })
        .unwrap_or_default();
    eprintln!(
        "{{\"event\":\"{}\",\"node\":{},\"duration_ms\":{}{} }}",
        event,
        node,
        duration.as_millis(),
        error
    );
}
