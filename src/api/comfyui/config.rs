use std::collections::BTreeSet;
use std::env;
use std::path::{Path, PathBuf};
use std::time::Duration;

use serde_json::{Map, Value};
use url::Url;

use super::paths;
use crate::api::{ApiError, ApiResult};

const DEFAULT_SERVER_URL: &str = "http://127.0.0.1:8188";
const DEFAULT_TIMEOUT_MS: u64 = 300_000;
const DEFAULT_POLL_INTERVAL_MS: u64 = 250;
const MAX_TIMEOUT_MS: u64 = 3_600_000;
const MAX_POLL_INTERVAL_MS: u64 = 60_000;

pub(super) struct RequestConfig {
    pub(super) project_root: PathBuf,
    pub(super) server_url: String,
    pub(super) authorization: Option<String>,
    pub(super) client_id: Option<String>,
    pub(super) extra_data: Option<Map<String, Value>>,
    pub(super) output_node_ids: Option<BTreeSet<String>>,
    pub(super) output_dir: Option<paths::OutputDirectory>,
    pub(super) default_output_relative: PathBuf,
    pub(super) timeout: Duration,
    pub(super) poll_interval: Duration,
}

pub(super) fn parse(
    root: &Path,
    workflow_id: &str,
    inputs: &Map<String, Value>,
) -> ApiResult<RequestConfig> {
    let project_root = paths::canonical_project_root(root)?;
    let (server_url, authorization) = server_and_authorization(inputs)?;
    let client_id = optional_string(inputs, "client_id")?;
    let extra_data = optional_object(inputs, "extra_data")?;
    let output_node_ids = output_node_ids(inputs)?;
    let default_output_relative =
        PathBuf::from(".lightflow/artifacts/comfyui").join(safe_segment(workflow_id));
    let output_dir = match optional_string(inputs, "output_dir")? {
        Some(value) => Some(paths::prepare_output_dir(
            &project_root,
            Path::new(&value),
            "output_dir",
        )?),
        None => {
            paths::prepare_output_dir(
                &project_root,
                &default_output_relative,
                "default ComfyUI output directory",
            )?;
            None
        }
    };
    let timeout = requested_timeout(inputs)?;
    let timeout_ms = timeout.as_millis() as u64;
    let poll_interval_ms = optional_u64(inputs, "poll_interval_ms")?
        .unwrap_or(DEFAULT_POLL_INTERVAL_MS)
        .clamp(1, MAX_POLL_INTERVAL_MS)
        .min(timeout_ms);
    Ok(RequestConfig {
        project_root,
        server_url,
        authorization,
        client_id,
        extra_data,
        output_node_ids,
        output_dir,
        default_output_relative,
        timeout,
        poll_interval: Duration::from_millis(poll_interval_ms),
    })
}

pub(super) fn requested_timeout(inputs: &Map<String, Value>) -> ApiResult<Duration> {
    let timeout_ms = optional_u64(inputs, "timeout_ms")?.unwrap_or(DEFAULT_TIMEOUT_MS);
    if !(1..=MAX_TIMEOUT_MS).contains(&timeout_ms) {
        return invalid(format!("timeout_ms must be between 1 and {MAX_TIMEOUT_MS}"));
    }
    Ok(Duration::from_millis(timeout_ms))
}

pub(super) fn normalize_server_url(value: &str, context: &str) -> ApiResult<String> {
    let mut parsed = Url::parse(value)
        .map_err(|error| ApiError::InvalidRequest(format!("invalid {context}: {error}")))?;
    if !matches!(parsed.scheme(), "http" | "https") {
        return invalid(format!("{context} must use http or https"));
    }
    if parsed.host_str().is_none() {
        return invalid(format!("{context} must include a host"));
    }
    if parsed.query().is_some() || parsed.fragment().is_some() {
        return invalid(format!("{context} must not include a query or fragment"));
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return invalid(format!("{context} must not include credentials"));
    }
    let default_port = match parsed.scheme() {
        "http" => 80,
        "https" => 443,
        _ => unreachable!("scheme validated above"),
    };
    if parsed.port() == Some(default_port) {
        parsed
            .set_port(None)
            .map_err(|_| ApiError::InvalidRequest(format!("invalid {context} port")))?;
    }
    let path = parsed.path().trim_end_matches('/').to_owned();
    parsed.set_path(&path);
    Ok(parsed.as_str().trim_end_matches('/').to_owned())
}

fn server_and_authorization(inputs: &Map<String, Value>) -> ApiResult<(String, Option<String>)> {
    let input_url = optional_string(inputs, "server_url")?;
    let configured_url = env::var("LIGHTFLOW_COMFYUI_URL").ok();
    let authorization = env::var("LIGHTFLOW_COMFYUI_AUTHORIZATION")
        .ok()
        .filter(|value| !value.is_empty());
    if authorization
        .as_deref()
        .is_some_and(|value| value.contains(['\r', '\n']))
    {
        return invalid("LIGHTFLOW_COMFYUI_AUTHORIZATION contains invalid header characters");
    }
    if authorization.is_some() {
        let configured_url = configured_url.ok_or_else(|| {
            ApiError::InvalidRequest(
                "LIGHTFLOW_COMFYUI_AUTHORIZATION requires LIGHTFLOW_COMFYUI_URL".to_owned(),
            )
        })?;
        let trusted = normalize_server_url(&configured_url, "LIGHTFLOW_COMFYUI_URL")?;
        let selected = match input_url {
            Some(value) => normalize_server_url(&value, "server_url")?,
            None => trusted.clone(),
        };
        if selected != trusted {
            return invalid(
                "refusing to send Authorization across origin: server_url must exactly match LIGHTFLOW_COMFYUI_URL",
            );
        }
        return Ok((trusted, authorization));
    }
    let value = input_url
        .or(configured_url)
        .unwrap_or_else(|| DEFAULT_SERVER_URL.to_owned());
    Ok((normalize_server_url(&value, "server_url")?, None))
}

fn output_node_ids(inputs: &Map<String, Value>) -> ApiResult<Option<BTreeSet<String>>> {
    let Some(value) = inputs.get("output_node_ids") else {
        return Ok(None);
    };
    let Some(values) = value.as_array() else {
        return invalid("output_node_ids must be a string array");
    };
    let mut output = BTreeSet::new();
    for value in values {
        let Some(value) = value.as_str().filter(|value| !value.is_empty()) else {
            return invalid("output_node_ids must contain non-empty strings");
        };
        output.insert(value.to_owned());
    }
    Ok(Some(output))
}

fn optional_object(
    inputs: &Map<String, Value>,
    name: &str,
) -> ApiResult<Option<Map<String, Value>>> {
    inputs
        .get(name)
        .map(|value| {
            value
                .as_object()
                .cloned()
                .ok_or_else(|| ApiError::InvalidRequest(format!("{name} must be an object")))
        })
        .transpose()
}

fn optional_string(inputs: &Map<String, Value>, name: &str) -> ApiResult<Option<String>> {
    inputs
        .get(name)
        .map(|value| {
            value
                .as_str()
                .map(ToOwned::to_owned)
                .ok_or_else(|| ApiError::InvalidRequest(format!("{name} must be a string")))
        })
        .transpose()
}

fn optional_u64(inputs: &Map<String, Value>, name: &str) -> ApiResult<Option<u64>> {
    inputs
        .get(name)
        .map(|value| {
            value.as_u64().ok_or_else(|| {
                ApiError::InvalidRequest(format!("{name} must be a non-negative integer"))
            })
        })
        .transpose()
}

fn safe_segment(value: &str) -> String {
    let value = value
        .chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => character,
            _ => '_',
        })
        .collect::<String>();
    if value.is_empty() {
        "output".to_owned()
    } else {
        value
    }
}

fn invalid<T>(message: impl Into<String>) -> ApiResult<T> {
    Err(ApiError::InvalidRequest(message.into()))
}
