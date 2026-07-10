use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde_json::{Map, Value};

use super::client::ComfyUiClient;
use super::deadline::Deadline;
use super::paths::OutputDirectory;
use crate::api::{ApiError, ApiResult};
use crate::workflow::WorkflowArtifact;

#[derive(Debug, Clone)]
pub(super) struct RemoteFile {
    pub(super) node_id: String,
    pub(super) field: String,
    pub(super) filename: String,
    pub(super) subfolder: String,
    pub(super) file_type: String,
    pub(super) descriptor: Value,
}

pub(super) struct ArtifactContext<'a> {
    pub(super) prompt_id: &'a str,
    pub(super) server_url: &'a str,
    pub(super) workflow_sha256: &'a str,
}

pub(super) fn extract_remote_files(
    outputs: &Value,
    output_node_ids: Option<&BTreeSet<String>>,
) -> ApiResult<Vec<RemoteFile>> {
    let Some(nodes) = outputs.as_object() else {
        return Ok(Vec::new());
    };
    let mut files = Vec::new();
    for (node_id, value) in nodes {
        if output_node_ids.is_some_and(|ids| !ids.contains(node_id)) {
            continue;
        }
        collect_descriptors(node_id, "", value, &mut files)?;
    }
    Ok(files)
}

pub(super) fn download_artifacts(
    client: &ComfyUiClient,
    output_dir: &OutputDirectory,
    files: &[RemoteFile],
    context: &ArtifactContext<'_>,
    deadline: &Deadline,
) -> ApiResult<Vec<WorkflowArtifact>> {
    files
        .iter()
        .enumerate()
        .map(|(index, remote)| {
            let name = local_output_name(remote, index, context.prompt_id);
            let path = output_dir.path().join(&name);
            client.download(remote, output_dir, &name, deadline)?;
            Ok(build_artifact(remote, path, index, context))
        })
        .collect()
}

fn collect_descriptors(
    node_id: &str,
    field: &str,
    value: &Value,
    files: &mut Vec<RemoteFile>,
) -> ApiResult<()> {
    match value {
        Value::Object(object) => {
            if let Some(remote) = descriptor(node_id, field, object)? {
                files.push(remote);
            }
            for (name, child) in object {
                let child_field = join_field(field, name);
                collect_descriptors(node_id, &child_field, child, files)?;
            }
        }
        Value::Array(values) => {
            for (index, child) in values.iter().enumerate() {
                let child_field = format!("{field}[{index}]");
                collect_descriptors(node_id, &child_field, child, files)?;
            }
        }
        _ => {}
    }
    Ok(())
}

fn descriptor(
    node_id: &str,
    field: &str,
    object: &Map<String, Value>,
) -> ApiResult<Option<RemoteFile>> {
    if !(object.contains_key("filename")
        && object.contains_key("subfolder")
        && object.contains_key("type"))
    {
        return Ok(None);
    }
    let filename = descriptor_string(object, "filename", node_id, field)?;
    let subfolder = descriptor_string(object, "subfolder", node_id, field)?;
    let file_type = descriptor_string(object, "type", node_id, field)?;
    if filename.is_empty() || file_type.is_empty() {
        return Err(ApiError::InvalidRequest(format!(
            "ComfyUI output descriptor {node_id}.{field} has empty filename or type"
        )));
    }
    Ok(Some(RemoteFile {
        node_id: node_id.to_owned(),
        field: field.to_owned(),
        filename,
        subfolder,
        file_type,
        descriptor: Value::Object(object.clone()),
    }))
}

fn descriptor_string(
    object: &Map<String, Value>,
    name: &str,
    node_id: &str,
    field: &str,
) -> ApiResult<String> {
    object
        .get(name)
        .and_then(Value::as_str)
        .map(ToOwned::to_owned)
        .ok_or_else(|| {
            ApiError::InvalidRequest(format!(
                "ComfyUI output descriptor {node_id}.{field}.{name} must be a string"
            ))
        })
}

fn local_output_name(remote: &RemoteFile, index: usize, prompt_id: &str) -> String {
    let normalized = remote.filename.replace('\\', "/");
    let basename = Path::new(&normalized)
        .file_name()
        .and_then(|value| value.to_str())
        .filter(|value| !value.is_empty())
        .map(safe_filename)
        .unwrap_or_else(|| "output.bin".to_owned());
    format!(
        "{}_{index:04}_{}_{}_{}",
        safe_segment(prompt_id),
        safe_segment(&remote.node_id),
        safe_segment(&remote.field),
        basename
    )
}

fn build_artifact(
    remote: &RemoteFile,
    path: PathBuf,
    index: usize,
    context: &ArtifactContext<'_>,
) -> WorkflowArtifact {
    let (kind, mime_type) = artifact_type(remote);
    let mut metadata = Map::new();
    metadata.insert("remote_node".to_owned(), remote.node_id.clone().into());
    metadata.insert("remote_field".to_owned(), remote.field.clone().into());
    metadata.insert("remote_descriptor".to_owned(), remote.descriptor.clone());
    metadata.insert("prompt_id".to_owned(), context.prompt_id.into());
    metadata.insert("server_url".to_owned(), context.server_url.into());
    metadata.insert("workflow_sha256".to_owned(), context.workflow_sha256.into());
    WorkflowArtifact {
        id: format!("comfyui-{index:04}"),
        kind: kind.to_owned(),
        path: path.display().to_string(),
        mime_type: mime_type.to_owned(),
        metadata,
    }
}

fn artifact_type(remote: &RemoteFile) -> (&'static str, &'static str) {
    let extension = Path::new(&remote.filename)
        .extension()
        .and_then(|value| value.to_str())
        .unwrap_or_default()
        .to_ascii_lowercase();
    match extension.as_str() {
        "png" => ("image", "image/png"),
        "jpg" | "jpeg" => ("image", "image/jpeg"),
        "webp" => ("image", "image/webp"),
        "bmp" => ("image", "image/bmp"),
        "gif" => ("gif", "image/gif"),
        "mp4" => ("video", "video/mp4"),
        "webm" => ("video", "video/webm"),
        "mov" => ("video", "video/quicktime"),
        "mp3" => ("audio", "audio/mpeg"),
        "wav" => ("audio", "audio/wav"),
        "ogg" | "oga" => ("audio", "audio/ogg"),
        "flac" => ("audio", "audio/flac"),
        _ if remote.field.to_ascii_lowercase().contains("gif") => ("gif", "image/gif"),
        _ if remote.field.to_ascii_lowercase().contains("video") => {
            ("video", "application/octet-stream")
        }
        _ if remote.field.to_ascii_lowercase().contains("audio") => {
            ("audio", "application/octet-stream")
        }
        _ => ("file", "application/octet-stream"),
    }
}

fn join_field(parent: &str, child: &str) -> String {
    if parent.is_empty() {
        child.to_owned()
    } else {
        format!("{parent}.{child}")
    }
}

fn safe_filename(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '-' | '_' => character,
            _ => '_',
        })
        .collect::<String>();
    if matches!(sanitized.as_str(), "" | "." | "..") {
        "output.bin".to_owned()
    } else {
        sanitized
    }
}

fn safe_segment(value: &str) -> String {
    let sanitized = value
        .chars()
        .map(|character| match character {
            'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' => character,
            _ => '_',
        })
        .collect::<String>();
    if sanitized.is_empty() {
        "output".to_owned()
    } else {
        sanitized
    }
}
