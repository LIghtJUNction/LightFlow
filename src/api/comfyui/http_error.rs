use std::io::Read;

use serde_json::Value;
use url::Url;

use super::deadline::Deadline;
use crate::api::{ApiError, ApiResult};

const MAX_ERROR_BODY: usize = 16 * 1024;

pub(super) fn response_json(
    action: &str,
    endpoint: &Url,
    response: ureq::Response,
    deadline: &Deadline,
) -> ApiResult<Value> {
    let value = response.into_json().map_err(|error| {
        ApiError::InvalidRequest(format!("ComfyUI {action} at {endpoint} failed: {error}"))
    })?;
    deadline.check(action)?;
    Ok(value)
}

pub(super) fn request_error(
    action: &str,
    endpoint: &Url,
    error: ureq::Error,
    deadline: &Deadline,
    authorization: Option<&str>,
) -> ApiError {
    if deadline.remaining(action).is_err() {
        return deadline.error(action);
    }
    match error {
        ureq::Error::Status(status, response) => {
            let detail = response_error_detail(response, authorization);
            ApiError::InvalidRequest(format!(
                "ComfyUI {action} at {endpoint} returned HTTP {status}: {detail}"
            ))
        }
        ureq::Error::Transport(error) => {
            ApiError::InvalidRequest(format!("ComfyUI {action} at {endpoint} failed: {error}"))
        }
    }
}

pub(super) fn bounded_json_detail(value: &Value, authorization: Option<&str>) -> String {
    let detail = serde_json::to_string(value).unwrap_or_else(|_| "{}".to_owned());
    redact_and_bound(detail, false, authorization)
}

fn response_error_detail(response: ureq::Response, authorization: Option<&str>) -> String {
    let mut bytes = Vec::new();
    let mut reader = response.into_reader().take((MAX_ERROR_BODY + 1) as u64);
    if reader.read_to_end(&mut bytes).is_err() {
        return "[unreadable response body]".to_owned();
    }
    let truncated = bytes.len() > MAX_ERROR_BODY;
    bytes.truncate(MAX_ERROR_BODY);
    let detail = serde_json::from_slice::<Value>(&bytes)
        .ok()
        .and_then(|value| serde_json::to_string(&value).ok())
        .unwrap_or_else(|| String::from_utf8_lossy(&bytes).into_owned());
    redact_and_bound(detail, truncated, authorization)
}

fn redact_and_bound(
    mut detail: String,
    already_truncated: bool,
    authorization: Option<&str>,
) -> String {
    if let Some(secret) = authorization {
        detail = detail.replace(secret, "[redacted]");
    }
    let mut truncated = already_truncated;
    if detail.len() > MAX_ERROR_BODY {
        detail.truncate(detail.floor_char_boundary(MAX_ERROR_BODY));
        truncated = true;
    }
    if truncated {
        detail.push_str(" [truncated]");
    }
    if detail.is_empty() {
        "[empty response body]".to_owned()
    } else {
        detail
    }
}
