use std::path::Path;

use serde_json::Value;
use url::Url;

use crate::api::{ApiError, ApiResult};

#[derive(Debug, Clone)]
pub(super) struct RemoteUpload {
    pub(super) name: String,
    pub(super) subfolder: String,
    pub(super) upload_type: String,
}

impl RemoteUpload {
    pub(super) fn reference(&self) -> String {
        if self.subfolder.is_empty() {
            self.name.clone()
        } else {
            format!("{}/{}", self.subfolder.trim_end_matches('/'), self.name)
        }
    }
}

pub(super) fn parse_remote_upload(value: Value, endpoint: &Url) -> ApiResult<RemoteUpload> {
    let name = response_string(&value, "name", endpoint)?;
    let subfolder = response_string(&value, "subfolder", endpoint)?;
    let upload_type = response_string(&value, "type", endpoint)?;
    if name.contains(['/', '\\', '"', '\r', '\n'])
        || Path::new(&name)
            .file_name()
            .and_then(|value| value.to_str())
            != Some(&name)
    {
        return invalid(format!(
            "ComfyUI upload image at {endpoint} returned unsafe name"
        ));
    }
    if !matches!(upload_type.as_str(), "input" | "temp") {
        return invalid(format!(
            "ComfyUI upload image at {endpoint} returned invalid type"
        ));
    }
    if Path::new(&subfolder).is_absolute()
        || subfolder.split('/').any(|part| part == "..")
        || subfolder.contains(['\\', '\r', '\n'])
    {
        return invalid(format!(
            "ComfyUI upload image at {endpoint} returned unsafe subfolder"
        ));
    }
    Ok(RemoteUpload {
        name,
        subfolder,
        upload_type,
    })
}

fn response_string(value: &Value, field: &str, endpoint: &Url) -> ApiResult<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            ApiError::InvalidRequest(format!(
                "ComfyUI upload image at {endpoint} returned no {field}"
            ))
        })
}

fn invalid<T>(message: impl Into<String>) -> ApiResult<T> {
    Err(ApiError::InvalidRequest(message.into()))
}
