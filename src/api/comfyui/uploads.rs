use std::fs::{File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Component, Path};

#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;

use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use super::deadline::Deadline;
use super::paths;
use crate::api::{ApiError, ApiResult};

#[derive(Debug)]
pub(super) struct Upload {
    pub(super) snapshot: File,
    pub(super) byte_len: u64,
    pub(super) name: String,
    pub(super) subfolder: String,
    pub(super) upload_type: String,
    pub(super) overwrite: bool,
    pub(super) bindings: Vec<UploadBinding>,
    pub(super) sha256: String,
}

#[derive(Debug, Clone)]
pub(super) struct UploadBinding {
    pub(super) node_id: String,
    pub(super) input: String,
}

pub(super) fn parse(
    root: &Path,
    workflow: &Value,
    value: Option<&Value>,
    deadline: &Deadline,
) -> ApiResult<Vec<Upload>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Some(items) = value.as_array() else {
        return invalid("uploads must be an array");
    };
    items
        .iter()
        .enumerate()
        .map(|(index, item)| parse_upload(root, workflow, index, item, deadline))
        .collect()
}

fn parse_upload(
    root: &Path,
    workflow: &Value,
    index: usize,
    value: &Value,
    deadline: &Deadline,
) -> ApiResult<Upload> {
    let Some(item) = value.as_object() else {
        return invalid(format!("uploads[{index}] must be an object"));
    };
    reject_unknown_fields(
        item,
        &["path", "name", "subfolder", "type", "overwrite", "bind"],
        &format!("uploads[{index}]"),
    )?;
    let context = format!("uploads[{index}].path");
    let path_value = required_non_empty_string(item, "path", &format!("uploads[{index}]"))?;
    let path = paths::canonical_existing_project_file(root, path_value, &context)?;
    let default_name = Path::new(path_value)
        .file_name()
        .and_then(|value| value.to_str())
        .ok_or_else(|| ApiError::InvalidRequest(format!("{context} has no UTF-8 basename")))?;
    let name = match item.get("name") {
        Some(value) => value
            .as_str()
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                ApiError::InvalidRequest(format!(
                    "uploads[{index}].name must be a non-empty string"
                ))
            })?,
        None => default_name,
    }
    .to_owned();
    validate_basename(&name, &format!("uploads[{index}].name"))?;
    let subfolder = match item.get("subfolder") {
        Some(value) => value.as_str().ok_or_else(|| {
            ApiError::InvalidRequest(format!("uploads[{index}].subfolder must be a string"))
        })?,
        None => "lightflow",
    }
    .to_owned();
    validate_subfolder(&subfolder, &format!("uploads[{index}].subfolder"))?;
    let upload_type = match item.get("type") {
        Some(value) => value.as_str().ok_or_else(|| {
            ApiError::InvalidRequest(format!("uploads[{index}].type must be a string"))
        })?,
        None => "input",
    }
    .to_owned();
    if !matches!(upload_type.as_str(), "input" | "temp") {
        return invalid(format!("uploads[{index}].type must be input or temp"));
    }
    let overwrite = match item.get("overwrite") {
        Some(value) => value.as_bool().ok_or_else(|| {
            ApiError::InvalidRequest(format!("uploads[{index}].overwrite must be boolean"))
        })?,
        None => true,
    };
    let bindings = parse_bindings(workflow, index, item.get("bind"))?;
    let mut source = open_upload_file(&path)?;
    let (snapshot, byte_len, sha256) = snapshot_file(&mut source, &path, deadline)?;
    Ok(Upload {
        snapshot,
        byte_len,
        name,
        subfolder,
        upload_type,
        overwrite,
        bindings,
        sha256,
    })
}

fn snapshot_file(
    source: &mut File,
    path: &Path,
    deadline: &Deadline,
) -> ApiResult<(File, u64, String)> {
    let metadata = source.metadata()?;
    if !metadata.is_file() {
        return invalid(format!(
            "upload path does not name a regular file: {}",
            path.display()
        ));
    }
    let mut snapshot = tempfile::tempfile()?;
    let mut hasher = Sha256::new();
    let mut total = 0_u64;
    let mut buffer = [0_u8; 64 * 1024];
    loop {
        deadline.check("hash upload")?;
        let count = source.read(&mut buffer)?;
        deadline.check("hash upload")?;
        if count == 0 {
            break;
        }
        snapshot.write_all(&buffer[..count])?;
        deadline.check("hash upload")?;
        hasher.update(&buffer[..count]);
        total = total.saturating_add(count as u64);
    }
    let metadata_len = source.metadata()?.len();
    if total != metadata_len {
        return invalid(format!(
            "upload file changed while hashing: {}",
            path.display()
        ));
    }
    snapshot.flush()?;
    snapshot.seek(SeekFrom::Start(0))?;
    Ok((snapshot, total, format!("{:x}", hasher.finalize())))
}

fn open_upload_file(path: &Path) -> ApiResult<File> {
    let mut options = OpenOptions::new();
    options.read(true);
    #[cfg(unix)]
    options.custom_flags(libc::O_NOFOLLOW | libc::O_CLOEXEC);
    options.open(path).map_err(ApiError::from)
}

fn parse_bindings(
    workflow: &Value,
    upload_index: usize,
    value: Option<&Value>,
) -> ApiResult<Vec<UploadBinding>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Some(items) = value.as_array() else {
        return invalid(format!("uploads[{upload_index}].bind must be an array"));
    };
    items
        .iter()
        .enumerate()
        .map(|(index, value)| parse_binding(workflow, upload_index, index, value))
        .collect()
}

fn parse_binding(
    workflow: &Value,
    upload_index: usize,
    index: usize,
    value: &Value,
) -> ApiResult<UploadBinding> {
    let context = format!("uploads[{upload_index}].bind[{index}]");
    let Some(binding) = value.as_object() else {
        return invalid(format!("{context} must be an object"));
    };
    reject_unknown_fields(binding, &["node_id", "input"], &context)?;
    let node_id = required_non_empty_string(binding, "node_id", &context)?;
    let input = required_non_empty_string(binding, "input", &context)?;
    ensure_node_inputs(workflow, node_id, "upload bind")?;
    Ok(UploadBinding {
        node_id: node_id.to_owned(),
        input: input.to_owned(),
    })
}

fn ensure_node_inputs(workflow: &Value, node_id: &str, context: &str) -> ApiResult<()> {
    workflow
        .as_object()
        .and_then(|workflow| workflow.get(node_id))
        .and_then(Value::as_object)
        .and_then(|node| node.get("inputs"))
        .and_then(Value::as_object)
        .map(|_| ())
        .ok_or_else(|| {
            ApiError::InvalidRequest(format!(
                "{context} references unknown node or inputs container {node_id}"
            ))
        })
}

fn validate_basename(value: &str, context: &str) -> ApiResult<()> {
    if value.contains(['/', '\\', '"', '\r', '\n'])
        || Path::new(value).file_name().and_then(|name| name.to_str()) != Some(value)
    {
        return invalid(format!(
            "{context} must be a safe basename without slashes, quotes, or line breaks"
        ));
    }
    Ok(())
}

fn validate_subfolder(value: &str, context: &str) -> ApiResult<()> {
    let path = Path::new(value);
    if path.is_absolute()
        || value.contains(['\\', '\r', '\n'])
        || path
            .components()
            .any(|component| !matches!(component, Component::Normal(_)))
            && !value.is_empty()
    {
        return invalid(format!("{context} must be a safe relative path"));
    }
    Ok(())
}

fn required_non_empty_string<'a>(
    object: &'a Map<String, Value>,
    name: &str,
    context: &str,
) -> ApiResult<&'a str> {
    object
        .get(name)
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            ApiError::InvalidRequest(format!(
                "{context}.{name} must be a required non-empty string"
            ))
        })
}

fn reject_unknown_fields(
    object: &Map<String, Value>,
    allowed: &[&str],
    context: &str,
) -> ApiResult<()> {
    if let Some(field) = object
        .keys()
        .find(|field| !allowed.contains(&field.as_str()))
    {
        return invalid(format!("{context} contains unknown field {field}"));
    }
    Ok(())
}

fn invalid<T>(message: impl Into<String>) -> ApiResult<T> {
    Err(ApiError::InvalidRequest(message.into()))
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use serde_json::json;

    use super::*;
    use crate::api::comfyui::deadline::Deadline;
    use crate::api::comfyui::multipart::MultipartBody;

    #[test]
    fn upload_snapshot_survives_same_length_in_place_source_change() {
        let root = test_root("upload-in-place-change");
        fs::create_dir_all(&root).expect("test root");
        let path = root.join("input.bin");
        let original = b"original";
        let replacement = b"replaced";
        fs::write(&path, original).expect("original upload");
        let workflow = json!({"1":{"class_type":"LoadImage","inputs":{}}});
        let uploads = json!([{"path":"input.bin"}]);
        let deadline = Deadline::new(Duration::from_secs(5));

        let upload = parse(&root, &workflow, Some(&uploads), &deadline)
            .expect("parse upload")
            .pop()
            .expect("upload");
        assert_eq!(upload.sha256, format!("{:x}", Sha256::digest(original)));
        fs::write(&path, replacement).expect("modify upload in place");

        let mut multipart = MultipartBody::new(&upload, "lightflow-boundary")
            .expect("multipart from pinned upload");
        let mut body = Vec::new();
        multipart.read_to_end(&mut body).expect("multipart body");
        assert!(body.windows(original.len()).any(|bytes| bytes == original));
        assert!(
            !body
                .windows(replacement.len())
                .any(|bytes| bytes == replacement)
        );

        fs::remove_dir_all(root).expect("remove test root");
    }

    #[test]
    fn expired_deadline_fails_while_hashing_upload() {
        let root = test_root("expired-upload-hash");
        fs::create_dir_all(&root).expect("test root");
        fs::write(root.join("input.bin"), b"upload").expect("upload");
        let workflow = json!({"1":{"class_type":"LoadImage","inputs":{}}});
        let uploads = json!([{"path":"input.bin"}]);
        let deadline = Deadline::new(Duration::ZERO);

        let error = parse(&root, &workflow, Some(&uploads), &deadline)
            .expect_err("expired hash must fail")
            .to_string();
        assert!(
            error.contains("ComfyUI hash upload exceeded total timeout of 0ms"),
            "{error}"
        );

        fs::remove_dir_all(root).expect("remove test root");
    }

    fn test_root(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "lightflow-comfyui-{name}-{}-{nanos}",
            std::process::id()
        ))
    }
}
