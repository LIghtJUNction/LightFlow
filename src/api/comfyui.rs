use std::path::Path;

use serde_json::{Map, Value, json};

use crate::api::plan::COMFYUI_API_ENGINE;
use crate::api::{ApiError, ApiResult};
use crate::workflow::{WorkflowArtifact, WorkflowSpec};

mod artifacts;
mod client;
mod config;
mod contract;
mod deadline;
mod http_error;
mod multipart;
mod output_files;
mod paths;
mod response;
mod uploads;

pub(super) struct ComfyUiExecution {
    pub(super) outputs: Map<String, Value>,
    pub(super) artifacts: Vec<WorkflowArtifact>,
    pub(super) replay_fingerprint: Value,
}

pub(super) fn execute(
    root: &Path,
    workflow_spec: &WorkflowSpec,
    inputs: &Map<String, Value>,
) -> ApiResult<ComfyUiExecution> {
    let deadline = deadline::Deadline::new(config::requested_timeout(inputs)?);
    let mut request = contract::parse(root, workflow_spec, inputs, &deadline)?;
    deadline.check("prepare request")?;
    let client = client::ComfyUiClient::new(
        request.server_url.clone(),
        request.authorization.take(),
        request.timeout,
    );
    let upload_fingerprints = upload_and_bind(&client, &mut request, &deadline)?;
    let workflow_sha256 = contract::workflow_sha256(&request.workflow)?;
    let replay_fingerprint =
        replay_fingerprint(&request.server_url, &workflow_sha256, &upload_fingerprints);
    let prompt_id = client.submit(
        &request.workflow,
        request.client_id.as_deref(),
        request.extra_data.as_ref(),
        &deadline,
    )?;
    let history = client.wait_for_history(&prompt_id, request.poll_interval, &deadline)?;
    let remote_outputs = history
        .get("outputs")
        .cloned()
        .unwrap_or_else(|| Value::Object(Map::new()));
    let remote_files =
        artifacts::extract_remote_files(&remote_outputs, request.output_node_ids.as_ref())?;
    let output_dir = match request.output_dir {
        Some(output_dir) => output_dir,
        None => paths::prepare_output_dir(
            &request.project_root,
            &request
                .default_output_relative
                .join(safe_segment(&prompt_id)),
            "default ComfyUI prompt output directory",
        )?,
    };
    deadline.check("prepare output directory")?;
    let artifact_context = artifacts::ArtifactContext {
        prompt_id: &prompt_id,
        server_url: &request.server_url,
        workflow_sha256: &workflow_sha256,
    };
    let artifacts = artifacts::download_artifacts(
        &client,
        &output_dir,
        &remote_files,
        &artifact_context,
        &deadline,
    )?;
    let outputs = workflow_outputs(
        workflow_spec,
        OutputValues {
            prompt_id: &prompt_id,
            artifacts: &artifacts,
            history: &history,
            remote_outputs: &remote_outputs,
            submitted_workflow: &request.workflow,
            workflow_sha256: &workflow_sha256,
            upload_fingerprints: &upload_fingerprints,
        },
    )?;

    Ok(ComfyUiExecution {
        outputs,
        artifacts,
        replay_fingerprint,
    })
}

fn upload_and_bind(
    client: &client::ComfyUiClient,
    request: &mut contract::RequestContract,
    deadline: &deadline::Deadline,
) -> ApiResult<Vec<Value>> {
    let mut fingerprints = Vec::with_capacity(request.uploads.len());
    for (index, upload) in request.uploads.iter().enumerate() {
        let remote = client.upload(upload, deadline)?;
        contract::apply_upload_binding(
            &mut request.workflow,
            &upload.bindings,
            &remote.reference(),
        )?;
        let bindings = upload
            .bindings
            .iter()
            .map(|binding| {
                json!({
                    "node_id": binding.node_id,
                    "input": binding.input,
                })
            })
            .collect::<Vec<_>>();
        fingerprints.push(json!({
            "index": index,
            "target": {
                "name": remote.name,
                "subfolder": remote.subfolder,
                "type": remote.upload_type,
            },
            "sha256": upload.sha256,
            "bind": bindings,
        }));
    }
    Ok(fingerprints)
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
        "prompt".to_owned()
    } else {
        value
    }
}

fn replay_fingerprint(server_url: &str, workflow_sha256: &str, uploads: &[Value]) -> Value {
    json!({
        "engine": COMFYUI_API_ENGINE,
        "server_url": server_url,
        "submitted_workflow_sha256": workflow_sha256,
        "uploads": uploads,
    })
}

struct OutputValues<'a> {
    prompt_id: &'a str,
    artifacts: &'a [WorkflowArtifact],
    history: &'a Value,
    remote_outputs: &'a Value,
    submitted_workflow: &'a Value,
    workflow_sha256: &'a str,
    upload_fingerprints: &'a [Value],
}

fn workflow_outputs(
    workflow: &WorkflowSpec,
    values: OutputValues<'_>,
) -> ApiResult<Map<String, Value>> {
    let artifact_values = values
        .artifacts
        .iter()
        .map(serde_json::to_value)
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            ApiError::InvalidRequest(format!("serialize ComfyUI artifact: {error}"))
        })?;
    let first_image = values
        .artifacts
        .iter()
        .zip(artifact_values.iter())
        .find(|(artifact, _)| artifact.kind == "image");
    let mut outputs = Map::new();
    for output in &workflow.outputs {
        let value = match output.name.as_str() {
            "prompt_id" => values.prompt_id.into(),
            "artifacts" | "files" => Value::Array(artifact_values.clone()),
            "image" => first_image
                .map(|(_, value)| value.clone())
                .unwrap_or(Value::Null),
            "image_path" => first_image
                .map(|(artifact, _)| artifact.path.clone().into())
                .unwrap_or(Value::Null),
            "history" => values.history.clone(),
            "remote_outputs" => values.remote_outputs.clone(),
            "submitted_workflow" => values.submitted_workflow.clone(),
            "workflow_sha256" => values.workflow_sha256.into(),
            "upload_fingerprints" => Value::Array(values.upload_fingerprints.to_vec()),
            other => values
                .remote_outputs
                .get(other)
                .cloned()
                .unwrap_or(Value::Null),
        };
        outputs.insert(output.name.clone(), value);
    }
    Ok(outputs)
}
