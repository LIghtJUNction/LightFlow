use super::model_backend::{ModelBackend, ModelResidency};
use super::{ApiError, ApiResult};
use crate::runner::ModelBinding;
use serde::Deserialize;
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const LFW_LOCK: &str = "lfw.lock";

#[derive(Debug)]
pub(super) struct ModelManager {
    root: PathBuf,
    resident: BTreeMap<ModelKey, Arc<ModelHandle>>,
}

pub(super) fn resolve_runner_models(
    root: &Path,
    workflow: &crate::workflow::WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<BTreeMap<String, ModelBinding>> {
    if workflow.models.is_empty() {
        return Ok(BTreeMap::new());
    }
    // A missing lock means the project never synced models. The runner decides
    // whether it can execute without bindings: preview runners ignore models,
    // while model-backed runners fail closed on the absent binding.
    let lock_path = root.join(LFW_LOCK);
    let source = match std::fs::read_to_string(&lock_path) {
        Ok(source) => source,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            return Ok(BTreeMap::new());
        }
        Err(error) => return Err(ApiError::Io(error)),
    };
    let lock: LfwLock = serde_json::from_str(&source).map_err(|error| {
        ApiError::InvalidRequest(format!("invalid {}: {error}", lock_path.display()))
    })?;
    let mut bindings = BTreeMap::new();
    for requirement in &workflow.models {
        let key = format!("{}::{}", workflow.id, requirement.id);
        // An unsynced requirement stays unbound; synced entries are enforced.
        let Some(entry) = lock.models.get(&key) else {
            continue;
        };
        let path = entry.local_paths.first().ok_or_else(|| {
            ApiError::InvalidRequest(format!("model lock entry {key} has no local path"))
        })?;
        if !path.is_file() {
            return Err(ApiError::InvalidRequest(format!(
                "model file for {key} is missing: {}",
                path.display()
            )));
        }
        let actual_size = std::fs::metadata(path)?.len();
        if let Some(expected_size) = entry.size_bytes
            && expected_size != actual_size
        {
            return Err(ApiError::InvalidRequest(format!(
                "model file size mismatch for {key}: expected {expected_size}, got {actual_size}"
            )));
        }
        let actual_sha256 = sha256_file(path)?;
        if let Some(expected_sha256) = entry.sha256.as_deref()
            && !expected_sha256.eq_ignore_ascii_case(&actual_sha256)
        {
            return Err(ApiError::InvalidRequest(format!(
                "model file SHA-256 mismatch for {key}"
            )));
        }
        let variant_id = entry.variant_id.clone().ok_or_else(|| {
            ApiError::InvalidRequest(format!("model lock entry {key} has no variant_id"))
        })?;
        for port in workflow
            .inputs
            .iter()
            .filter(|port| port.model_requirement.as_deref() == Some(&requirement.id))
        {
            if let Some(selected) = inputs.get(&port.name) {
                let selected = selected.as_str().ok_or_else(|| {
                    ApiError::InvalidRequest(format!(
                        "model selector input `{}` must be a string",
                        port.name
                    ))
                })?;
                if selected != variant_id {
                    return Err(ApiError::InvalidRequest(format!(
                        "model selector input `{}` requested {selected}, but lfw.lock resolved {}",
                        port.name, variant_id
                    )));
                }
            }
        }
        bindings.insert(
            requirement.id.clone(),
            ModelBinding {
                requirement_id: requirement.id.clone(),
                variant_id,
                path: path.clone(),
                sha256: Some(actual_sha256),
                size_bytes: Some(actual_size),
                snapshot_revision: entry.snapshot_revision.clone(),
            },
        );
    }
    Ok(bindings)
}

fn sha256_file(path: &Path) -> ApiResult<String> {
    let file = std::fs::File::open(path)?;
    let mut reader = BufReader::new(file);
    let mut hasher = Sha256::new();
    let mut buffer = [0_u8; 1024 * 1024];
    loop {
        let count = reader.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(format!("{:x}", hasher.finalize()))
}

impl ModelManager {
    pub(super) fn new(root: impl Into<PathBuf>) -> Self {
        Self {
            root: root.into(),
            resident: BTreeMap::new(),
        }
    }

    #[allow(dead_code)]
    pub(super) fn get_locked(
        &mut self,
        workflow_id: &str,
        requirement_id: &str,
    ) -> ApiResult<Arc<ModelHandle>> {
        let path = self.locked_path(workflow_id, requirement_id)?;
        let key = ModelKey {
            workflow_id: workflow_id.to_owned(),
            requirement_id: requirement_id.to_owned(),
            path: path.clone(),
        };
        if let Some(handle) = self.resident.get(&key) {
            return Ok(Arc::clone(handle));
        }

        let handle = Arc::new(ModelHandle::load(
            workflow_id.to_owned(),
            requirement_id.to_owned(),
            path,
        )?);
        self.resident.insert(key, Arc::clone(&handle));
        Ok(handle)
    }

    pub(super) fn locked_path(
        &self,
        workflow_id: &str,
        requirement_id: &str,
    ) -> ApiResult<PathBuf> {
        read_locked_model_path(&self.root, workflow_id, requirement_id, None)
    }

    #[cfg(test)]
    pub(super) fn locked_path_with_format(
        &self,
        workflow_id: &str,
        requirement_id: &str,
        expected_format: &str,
    ) -> ApiResult<PathBuf> {
        read_locked_model_path(
            &self.root,
            workflow_id,
            requirement_id,
            Some(expected_format),
        )
    }

    #[allow(dead_code)]
    pub(super) fn unload(&mut self, workflow_id: &str, requirement_id: &str) -> bool {
        let before = self.resident.len();
        self.resident.retain(|key, _| {
            key.workflow_id != workflow_id || key.requirement_id != requirement_id
        });
        self.resident.len() != before
    }

    #[allow(dead_code)]
    pub(super) fn clear(&mut self) {
        self.resident.clear();
    }

    #[cfg(test)]
    pub(super) fn resident_len(&self) -> usize {
        self.resident.len()
    }
}

#[derive(Clone)]
pub(super) struct ModelHandle {
    workflow_id: String,
    requirement_id: String,
    path: PathBuf,
    residency: ModelResidency,
    backend: ModelBackend,
}

impl std::fmt::Debug for ModelHandle {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("ModelHandle")
            .field("workflow_id", &self.workflow_id)
            .field("requirement_id", &self.requirement_id)
            .field("path", &self.path)
            .field("residency", &self.residency)
            .field("backend", &self.backend.kind())
            .finish()
    }
}

#[allow(dead_code)]
impl ModelHandle {
    fn load(workflow_id: String, requirement_id: String, path: PathBuf) -> ApiResult<Self> {
        let backend = ModelBackend::load(&path)?;
        let residency = backend.residency();
        Ok(Self {
            workflow_id,
            requirement_id,
            path,
            residency,
            backend,
        })
    }

    pub(super) fn path(&self) -> &Path {
        &self.path
    }

    pub(super) fn requirement_id(&self) -> &str {
        &self.requirement_id
    }

    #[allow(dead_code)]
    pub(super) fn residency(&self) -> ModelResidency {
        self.residency
    }

    pub(super) fn backend_kind(&self) -> &'static str {
        self.backend.kind()
    }
}

#[derive(Debug, Clone, Eq, PartialEq, Ord, PartialOrd)]
struct ModelKey {
    workflow_id: String,
    requirement_id: String,
    path: PathBuf,
}

fn read_locked_model_path(
    root: &Path,
    workflow_id: &str,
    requirement_id: &str,
    expected_format: Option<&str>,
) -> ApiResult<PathBuf> {
    let lock_path = root.join(LFW_LOCK);
    let source = std::fs::read_to_string(&lock_path).map_err(|error| {
        if error.kind() == std::io::ErrorKind::NotFound {
            ApiError::InvalidRequest(format!(
                "runtime requires synced models for workflow {workflow_id}; run `lfw sync {workflow_id} --auto-model --apply` or `lfw sync {workflow_id} --locked --apply` first"
            ))
        } else {
            ApiError::Io(error)
        }
    })?;
    let lock: LfwLock = serde_json::from_str(&source).map_err(|error| {
        ApiError::InvalidRequest(format!("invalid {}: {error}", lock_path.display()))
    })?;
    let key = format!("{workflow_id}::{requirement_id}");
    let entry = lock.models.get(&key).ok_or_else(|| {
        ApiError::InvalidRequest(format!(
            "runtime is missing model lock entry {key}; run `lfw sync {workflow_id} --auto-model --apply` or verify the cache with `lfw sync {workflow_id} --locked --apply`"
        ))
    })?;
    if let Some(expected_format) = expected_format {
        let actual_format = entry
            .format
            .as_deref()
            .or_else(|| entry.file.as_deref().and_then(file_extension));
        if let Some(actual_format) = actual_format
            && !actual_format.eq_ignore_ascii_case(expected_format)
        {
            return Err(ApiError::InvalidRequest(format!(
                "model lock entry {key} has incompatible format {actual_format}; expected {expected_format}. Run `lfw sync {workflow_id} --model {requirement_id}=<variant> --apply` with a compatible variant"
            )));
        }
    }
    let path = entry.local_paths.first().ok_or_else(|| {
        ApiError::InvalidRequest(format!(
            "model lock entry {key} has no local path; run `lfw sync {workflow_id} --auto-model --apply` or `lfw sync {workflow_id} --locked --apply`"
        ))
    })?;
    if !path.is_file() {
        return Err(ApiError::InvalidRequest(format!(
            "model file for {key} is missing: {}; run `lfw sync {workflow_id} --locked --apply` to verify the locked cache or resync with `lfw sync {workflow_id} --auto-model --apply`",
            path.display(),
        )));
    }
    Ok(path.clone())
}

fn file_extension(file: &str) -> Option<&str> {
    Path::new(file)
        .extension()
        .and_then(std::ffi::OsStr::to_str)
}

#[derive(Debug, Deserialize)]
struct LfwLock {
    #[serde(default)]
    models: BTreeMap<String, LockedModel>,
}

#[derive(Debug, Deserialize)]
struct LockedModel {
    #[serde(default)]
    local_paths: Vec<PathBuf>,
    #[serde(default)]
    format: Option<String>,
    #[serde(default)]
    file: Option<String>,
    #[serde(default)]
    variant_id: Option<String>,
    #[serde(default)]
    sha256: Option<String>,
    #[serde(default)]
    size_bytes: Option<u64>,
    #[serde(default)]
    snapshot_revision: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn model_manager_reuses_locked_model_handles() -> Result<(), Box<dyn std::error::Error>> {
        let root =
            std::env::temp_dir().join(format!("lightflow-model-manager-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("models"))?;
        let model_path = root.join("models/model.safetensors");
        fs::write(&model_path, b"tiny")?;
        fs::write(
            root.join(LFW_LOCK),
            serde_json::json!({
                "models": {
                    "lightflow.test::flux_model": {
                        "local_paths": [model_path]
                    }
                }
            })
            .to_string(),
        )?;

        let mut manager = ModelManager::new(&root);
        let first = manager.get_locked("lightflow.test", "flux_model")?;
        let second = manager.get_locked("lightflow.test", "flux_model")?;

        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(manager.resident_len(), 1);
        assert!(manager.unload("lightflow.test", "flux_model"));
        assert_eq!(manager.resident_len(), 0);

        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn model_manager_rejects_incompatible_locked_format() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = std::env::temp_dir().join(format!(
            "lightflow-model-manager-format-{}",
            std::process::id()
        ));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("models"))?;
        let model_path = root.join("models/model.safetensors");
        fs::write(&model_path, b"tiny")?;
        fs::write(
            root.join(LFW_LOCK),
            serde_json::json!({
                "models": {
                    "lightflow.test::flux_model": {
                        "format": "safetensors",
                        "local_paths": [model_path]
                    }
                }
            })
            .to_string(),
        )?;

        let manager = ModelManager::new(&root);
        let error = manager
            .locked_path_with_format("lightflow.test", "flux_model", "gguf")
            .expect_err("format mismatch should fail");
        let message = error.to_string();

        assert!(message.contains("incompatible format safetensors"));
        assert!(message.contains("expected gguf"));
        assert!(message.contains("lfw sync lightflow.test --model flux_model=<variant> --apply"));

        let _ = fs::remove_dir_all(root);
        Ok(())
    }

    #[test]
    fn runner_models_resolve_verified_lock_and_reject_selector_mismatch()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = tempfile::tempdir()?;
        let model_path = root.path().join("model.gguf");
        fs::write(&model_path, b"locked model")?;
        let sha256 = sha256_file(&model_path)?;
        fs::write(
            root.path().join(LFW_LOCK),
            serde_json::json!({
                "models": {
                    "lightflow.test::image_model": {
                        "variant_id": "tiny-q4",
                        "local_paths": [model_path],
                        "sha256": sha256,
                        "size_bytes": 12
                    }
                }
            })
            .to_string(),
        )?;
        let workflow = crate::workflow::workflow_with_identity("lightflow.test", "0.1.0")
            .input("model", "text")
            .input_model_requirement("model", "image_model")
            .model("image_model", "image-generation")
            .build();
        let inputs =
            serde_json::Map::from_iter([("model".to_owned(), serde_json::json!("tiny-q4"))]);
        let models = resolve_runner_models(root.path(), &workflow, &inputs)?;
        assert_eq!(models["image_model"].variant_id, "tiny-q4");
        assert_eq!(
            models["image_model"].sha256.as_deref(),
            Some(sha256.as_str())
        );

        let mismatch =
            serde_json::Map::from_iter([("model".to_owned(), serde_json::json!("another"))]);
        let error = resolve_runner_models(root.path(), &workflow, &mismatch)
            .expect_err("selector mismatch");
        assert!(error.to_string().contains("but lfw.lock resolved tiny-q4"));

        fs::write(&model_path, b"tampered")?;
        let error =
            resolve_runner_models(root.path(), &workflow, &inputs).expect_err("tampered model");
        assert!(
            error.to_string().contains("size mismatch")
                || error.to_string().contains("SHA-256 mismatch")
        );
        Ok(())
    }
}
