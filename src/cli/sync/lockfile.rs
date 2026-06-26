use super::cache_metadata::sha256_file;
use crate::cli::{CliError, CliResult};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

pub(super) const LFW_LOCK: &str = "lfw.lock";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub(super) struct LfwLock {
    version: u32,
    #[serde(default)]
    models: BTreeMap<String, serde_json::Value>,
    #[serde(default)]
    pub(super) skills: BTreeMap<String, SkillLockEntry>,
}

impl Default for LfwLock {
    fn default() -> Self {
        Self {
            version: 2,
            models: BTreeMap::new(),
            skills: BTreeMap::new(),
        }
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub(super) struct SkillLockEntry {
    pub(super) source: String,
    pub(super) choice: String,
    pub(super) target: Option<String>,
    pub(super) link: Option<String>,
}

pub(super) fn write_lfw_lock(
    root: &Path,
    workflow_scope: Option<&str>,
    downloads: &[serde_json::Value],
) -> CliResult<()> {
    if downloads.is_empty() {
        return Ok(());
    }
    let mut lock = read_lfw_lock_optional(root)?;
    for download in downloads {
        let requirement_id = download["requirement_id"].as_str().unwrap_or("unknown");
        let key = lock_key(workflow_scope, requirement_id);
        let mut entry = download.clone();
        if let Some(object) = entry.as_object_mut() {
            object.insert("workflow_scope".to_owned(), json!(workflow_scope));
        }
        lock.models.insert(key, entry);
    }
    write_lfw_lock_file(root, &lock)
}

pub(super) fn write_lfw_lock_file(root: &Path, lock: &LfwLock) -> CliResult<()> {
    let path = root.join(LFW_LOCK);
    let mut bytes = serde_json::to_vec_pretty(&lock)?;
    bytes.push(b'\n');
    fs::write(path, bytes)?;
    Ok(())
}

fn read_lfw_lock(root: &Path) -> CliResult<LfwLock> {
    let path = root.join(LFW_LOCK);
    if !path.exists() {
        return Err(CliError::Usage(format!(
            "sync --locked requires {}",
            path.display()
        )));
    }
    Ok(serde_json::from_slice::<LfwLock>(&fs::read(&path)?)?)
}

pub(super) fn read_lfw_lock_optional(root: &Path) -> CliResult<LfwLock> {
    let path = root.join(LFW_LOCK);
    if path.exists() {
        Ok(serde_json::from_slice::<LfwLock>(&fs::read(&path)?)?)
    } else {
        Ok(LfwLock::default())
    }
}

pub(super) fn verify_locked_downloads(
    root: &Path,
    workflow_scope: Option<&str>,
    downloads: &[serde_json::Value],
) -> CliResult<Vec<serde_json::Value>> {
    if downloads.is_empty() {
        return Ok(Vec::new());
    }
    let lock = read_lfw_lock(root)?;
    downloads
        .iter()
        .map(|download| verify_locked_download(workflow_scope, download, &lock))
        .collect()
}

fn verify_locked_download(
    workflow_scope: Option<&str>,
    download: &serde_json::Value,
    lock: &LfwLock,
) -> CliResult<serde_json::Value> {
    let requirement_id = download["requirement_id"]
        .as_str()
        .ok_or_else(|| CliError::Usage("invalid hf download plan".to_owned()))?;
    let key = lock_key(workflow_scope, requirement_id);
    let entry = lock.models.get(&key).ok_or_else(|| {
        CliError::Usage(format!(
            "sync --locked is missing model lock entry for {key}"
        ))
    })?;
    for field in ["repo", "file", "variant_id", "format"] {
        if entry.get(field) != download.get(field) {
            return Err(CliError::Usage(format!(
                "sync --locked model lock mismatch for {key}: field {field}"
            )));
        }
    }
    let local_path = entry["local_paths"]
        .as_array()
        .and_then(|paths| paths.first())
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| {
            CliError::Usage(format!(
                "sync --locked model lock entry for {key} has no local path"
            ))
        })?;
    let local_path = Path::new(local_path);
    if !local_path.is_file() {
        return Err(CliError::Usage(format!(
            "sync --locked cached model file is missing for {key}: {}",
            local_path.display()
        )));
    }
    if let Some(expected_size) = entry["size_bytes"].as_u64() {
        let actual_size = fs::metadata(local_path)?.len();
        if actual_size != expected_size {
            return Err(CliError::Usage(format!(
                "sync --locked size mismatch for {key}: expected {expected_size}, got {actual_size}"
            )));
        }
    }
    if let Some(expected_sha256) = entry["sha256"].as_str() {
        let actual_sha256 = sha256_file(local_path)?.ok_or_else(|| {
            CliError::Usage(format!(
                "sync --locked cannot hash cached model file for {key}"
            ))
        })?;
        if actual_sha256 != expected_sha256 {
            return Err(CliError::Usage(format!(
                "sync --locked sha256 mismatch for {key}"
            )));
        }
    }
    Ok(json!({
        "requirement_id": requirement_id,
        "key": key,
        "status": "verified",
        "local_path": local_path.to_string_lossy(),
        "sha256": entry["sha256"].clone(),
        "size_bytes": entry["size_bytes"].clone(),
        "snapshot_revision": entry["snapshot_revision"].clone(),
    }))
}

fn lock_key(workflow_scope: Option<&str>, requirement_id: &str) -> String {
    format!("{}::{requirement_id}", workflow_scope.unwrap_or("*"))
}
