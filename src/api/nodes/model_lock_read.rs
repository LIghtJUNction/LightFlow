use super::model_locks::{ModelLockState, ModelLockStatus};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

const LFW_LOCK: &str = "lfw.lock";

pub(super) fn read_model_lock(root: &Path) -> ModelLockRead {
    let path = root.join(LFW_LOCK);
    if !path.exists() {
        return ModelLockRead::MissingLock;
    }
    let Ok(source) = fs::read_to_string(path) else {
        return ModelLockRead::InvalidLock;
    };
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&source) else {
        return ModelLockRead::InvalidLock;
    };
    let Some(models) = value.get("models").and_then(serde_json::Value::as_object) else {
        return ModelLockRead::Entries(BTreeMap::new());
    };
    ModelLockRead::Entries(
        models
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()))
            .collect(),
    )
}

pub(super) fn model_lock_status(
    lock: &ModelLockRead,
    workflow_id: &str,
    requirement_id: &str,
) -> ModelLockStatus {
    let key = format!("{workflow_id}::{requirement_id}");
    match lock {
        ModelLockRead::MissingLock => ModelLockStatus::empty(ModelLockState::MissingLock, key),
        ModelLockRead::InvalidLock => ModelLockStatus::empty(ModelLockState::InvalidLock, key),
        ModelLockRead::Entries(entries) => {
            let Some(entry) = entries.get(&key) else {
                return ModelLockStatus::empty(ModelLockState::MissingEntry, key);
            };
            let local_paths = entry
                .get("local_paths")
                .and_then(serde_json::Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(serde_json::Value::as_str)
                .map(PathBuf::from)
                .collect::<Vec<_>>();
            let missing_paths = local_paths
                .iter()
                .filter(|path| !path.is_file())
                .cloned()
                .collect::<Vec<_>>();
            ModelLockStatus {
                status: if local_paths.is_empty() || !missing_paths.is_empty() {
                    ModelLockState::MissingPath
                } else {
                    ModelLockState::Available
                },
                key,
                variant_id: entry_string(entry, "variant_id"),
                repo: entry_string(entry, "repo"),
                file: entry_string(entry, "file"),
                format: entry_string(entry, "format"),
                sha256: entry_string(entry, "sha256"),
                hash_algorithm: entry_string(entry, "hash_algorithm"),
                size_bytes: entry.get("size_bytes").and_then(serde_json::Value::as_u64),
                snapshot_revision: entry_string(entry, "snapshot_revision"),
                local_paths,
                missing_paths,
            }
        }
    }
}

pub(super) enum ModelLockRead {
    MissingLock,
    InvalidLock,
    Entries(BTreeMap<String, serde_json::Value>),
}

impl ModelLockStatus {
    fn empty(status: ModelLockState, key: String) -> Self {
        Self {
            status,
            key,
            variant_id: None,
            repo: None,
            file: None,
            format: None,
            sha256: None,
            hash_algorithm: None,
            size_bytes: None,
            snapshot_revision: None,
            local_paths: Vec::new(),
            missing_paths: Vec::new(),
        }
    }
}

fn entry_string(entry: &serde_json::Value, field: &str) -> Option<String> {
    entry
        .get(field)
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned)
}
