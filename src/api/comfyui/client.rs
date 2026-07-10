use std::io::{self, Write};
use std::thread;
use std::time::Duration;

use serde_json::{Map, Value};
use url::Url;

use super::artifacts::RemoteFile;
use super::deadline::Deadline;
use super::http_error;
use super::multipart::MultipartBody;
use super::output_files;
use super::paths::OutputDirectory;
use super::response::{self, RemoteUpload};
use super::uploads::Upload;
use crate::api::{ApiError, ApiResult};

pub(super) struct ComfyUiClient {
    agent: ureq::Agent,
    server_url: String,
    authorization: Option<String>,
}

impl ComfyUiClient {
    pub(super) fn new(
        server_url: String,
        authorization: Option<String>,
        timeout: Duration,
    ) -> Self {
        let agent = ureq::AgentBuilder::new()
            .timeout_connect(timeout)
            .timeout_read(timeout)
            .timeout_write(timeout)
            .redirects(0)
            .build();
        Self {
            agent,
            server_url,
            authorization,
        }
    }

    pub(super) fn upload(&self, upload: &Upload, deadline: &Deadline) -> ApiResult<RemoteUpload> {
        let action = "upload image";
        let endpoint = self.endpoint(&["upload", "image"])?;
        let boundary = format!("----lightflow-{}", &upload.sha256[..16]);
        let body = MultipartBody::new(upload, &boundary)?;
        let content_length = body.content_length.to_string();
        let request = self.authorize(
            self.agent
                .post(endpoint.as_str())
                .timeout(deadline.remaining(action)?)
                .set("content-type", &body.content_type)
                .set("content-length", &content_length)
                .set("accept", "application/json"),
        );
        let response = request
            .send(body)
            .map_err(|error| self.request_error(action, &endpoint, error, deadline))?;
        let value =
            http_error::response_json("parse upload response", &endpoint, response, deadline)?;
        response::parse_remote_upload(value, &endpoint)
    }

    pub(super) fn submit(
        &self,
        workflow: &Value,
        client_id: Option<&str>,
        extra_data: Option<&Map<String, Value>>,
        deadline: &Deadline,
    ) -> ApiResult<String> {
        let action = "submit prompt";
        let endpoint = self.endpoint(&["prompt"])?;
        let mut body = Map::new();
        body.insert("prompt".to_owned(), workflow.clone());
        if let Some(client_id) = client_id {
            body.insert("client_id".to_owned(), client_id.into());
        }
        if let Some(extra_data) = extra_data {
            body.insert("extra_data".to_owned(), extra_data.clone().into());
        }
        let request = self.authorize(
            self.agent
                .post(endpoint.as_str())
                .timeout(deadline.remaining(action)?)
                .set("content-type", "application/json")
                .set("accept", "application/json"),
        );
        let response = request
            .send_json(Value::Object(body))
            .map_err(|error| self.request_error(action, &endpoint, error, deadline))?;
        let value =
            http_error::response_json("parse prompt response", &endpoint, response, deadline)?;
        if value.get("error").is_some_and(non_empty_json)
            || value.get("node_errors").is_some_and(non_empty_json)
        {
            let detail = http_error::bounded_json_detail(
                &Value::Object(
                    ["error", "node_errors"]
                        .into_iter()
                        .filter_map(|name| {
                            value
                                .get(name)
                                .cloned()
                                .map(|value| (name.to_owned(), value))
                        })
                        .collect(),
                ),
                self.authorization.as_deref(),
            );
            return invalid(format!(
                "ComfyUI submit prompt at {endpoint} returned validation errors: {detail}"
            ));
        }
        value
            .get("prompt_id")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
            .ok_or_else(|| {
                ApiError::InvalidRequest(format!(
                    "ComfyUI submit prompt at {endpoint} returned no prompt_id"
                ))
            })
    }

    pub(super) fn wait_for_history(
        &self,
        prompt_id: &str,
        poll_interval: Duration,
        deadline: &Deadline,
    ) -> ApiResult<Value> {
        let action = "history poll";
        let endpoint = self.endpoint(&["history", prompt_id])?;
        loop {
            let request = self.authorize(
                self.agent
                    .get(endpoint.as_str())
                    .timeout(deadline.remaining(action)?)
                    .set("accept", "application/json"),
            );
            let response = request
                .call()
                .map_err(|error| self.request_error(action, &endpoint, error, deadline))?;
            let history =
                http_error::response_json("parse history response", &endpoint, response, deadline)?;
            let Some(entry) = history.get(prompt_id) else {
                sleep_until_next_poll(deadline, action, poll_interval)?;
                continue;
            };
            if history_failed(entry) {
                return invalid(format!(
                    "ComfyUI history poll at {endpoint} reported execution_error"
                ));
            }
            if entry
                .get("status")
                .and_then(|status| status.get("completed"))
                .and_then(Value::as_bool)
                == Some(true)
            {
                return Ok(entry.clone());
            }
            sleep_until_next_poll(deadline, action, poll_interval)?;
        }
    }

    pub(super) fn download(
        &self,
        remote: &RemoteFile,
        output_dir: &OutputDirectory,
        name: &str,
        deadline: &Deadline,
    ) -> ApiResult<()> {
        let action = "download output";
        let mut endpoint = self.endpoint(&["view"])?;
        endpoint.query_pairs_mut().clear().extend_pairs([
            ("filename", remote.filename.as_str()),
            ("subfolder", remote.subfolder.as_str()),
            ("type", remote.file_type.as_str()),
        ]);
        let request = self.authorize(
            self.agent
                .get(endpoint.as_str())
                .timeout(deadline.remaining(action)?),
        );
        let response = request
            .call()
            .map_err(|error| self.request_error(action, &endpoint, error, deadline))?;
        let (temporary, mut file) = output_files::create_unique_temporary(output_dir, name)?;
        let result = io::copy(&mut response.into_reader(), &mut file)
            .and_then(|_| file.flush())
            .map_err(ApiError::Io)
            .and_then(|_| deadline.check(action))
            .and_then(|_| output_files::persist_no_clobber(output_dir, &temporary, name))
            .and_then(|_| output_files::verify_and_finalize(output_dir, &temporary, name));
        drop(file);
        if result.is_err() {
            output_files::cleanup_temporary(output_dir, &temporary);
        }
        result.map_err(|error| match deadline.remaining(action) {
            Err(timeout) => timeout,
            Ok(_) => error,
        })
    }

    fn authorize(&self, request: ureq::Request) -> ureq::Request {
        match self.authorization.as_deref() {
            Some(value) => request.set("authorization", value),
            None => request,
        }
    }

    fn endpoint(&self, segments: &[&str]) -> ApiResult<Url> {
        let mut endpoint = Url::parse(&self.server_url).map_err(|error| {
            ApiError::InvalidRequest(format!("invalid normalized ComfyUI server URL: {error}"))
        })?;
        endpoint
            .path_segments_mut()
            .map_err(|_| {
                ApiError::InvalidRequest("ComfyUI server URL cannot be a base".to_owned())
            })?
            .pop_if_empty()
            .extend(segments.iter().copied());
        Ok(endpoint)
    }

    fn request_error(
        &self,
        action: &str,
        endpoint: &Url,
        error: ureq::Error,
        deadline: &Deadline,
    ) -> ApiError {
        http_error::request_error(
            action,
            endpoint,
            error,
            deadline,
            self.authorization.as_deref(),
        )
    }
}

fn history_failed(entry: &Value) -> bool {
    if entry.get("error").is_some_and(non_empty_json) {
        return true;
    }
    let status = entry.get("status").unwrap_or(&Value::Null);
    if status
        .get("status_str")
        .and_then(Value::as_str)
        .is_some_and(|status| status.eq_ignore_ascii_case("error"))
        || status.get("error").is_some_and(non_empty_json)
    {
        return true;
    }
    contains_execution_error(status)
}

fn contains_execution_error(value: &Value) -> bool {
    match value {
        Value::String(value) => matches!(
            value.as_str(),
            "execution_error" | "execution_interrupted" | "execution_failed"
        ),
        Value::Array(values) => values.iter().any(contains_execution_error),
        Value::Object(values) => values.values().any(contains_execution_error),
        _ => false,
    }
}

fn non_empty_json(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
        Value::Number(_) => true,
    }
}

fn sleep_until_next_poll(deadline: &Deadline, action: &str, interval: Duration) -> ApiResult<()> {
    thread::sleep(interval.min(deadline.remaining(action)?));
    deadline.check(action)
}

fn invalid<T>(message: impl Into<String>) -> ApiResult<T> {
    Err(ApiError::InvalidRequest(message.into()))
}
