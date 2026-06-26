use super::{ApiError, ApiResult};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const LFW_LOCK: &str = "lfw.lock";

#[derive(Debug)]
pub(super) struct ModelManager {
    root: PathBuf,
    resident: BTreeMap<ModelKey, Arc<ModelHandle>>,
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

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[allow(dead_code)]
pub(super) enum ModelResidency {
    HostMapped,
    DeviceResident,
}

#[derive(Clone)]
#[allow(dead_code)]
enum ModelBackend {
    Path,
    #[cfg(feature = "gguf")]
    CandleGguf(Arc<CandleGgufModel>),
}

#[allow(dead_code)]
impl ModelBackend {
    fn load(path: &Path) -> ApiResult<Self> {
        if is_gguf(path) {
            return Self::load_gguf(path);
        }
        Ok(Self::Path)
    }

    fn residency(&self) -> ModelResidency {
        match self {
            Self::Path => ModelResidency::HostMapped,
            #[cfg(feature = "gguf")]
            Self::CandleGguf(model) => model.residency,
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Self::Path => "path",
            #[cfg(feature = "gguf")]
            Self::CandleGguf(_) => "candle.gguf",
        }
    }

    #[cfg(feature = "gguf")]
    fn load_gguf(path: &Path) -> ApiResult<Self> {
        let device = candle_device()?;
        let var_builder =
            candle_transformers::quantized_var_builder::VarBuilder::from_gguf(path, &device)
                .map_err(|error| {
                    ApiError::InvalidRequest(format!(
                        "failed to load GGUF model with Candle from {}: {error}",
                        path.display()
                    ))
                })?;
        let residency = if matches!(device, candle_core::Device::Cpu) {
            ModelResidency::HostMapped
        } else {
            ModelResidency::DeviceResident
        };
        Ok(Self::CandleGguf(Arc::new(CandleGgufModel {
            device,
            var_builder,
            residency,
        })))
    }

    #[cfg(not(feature = "gguf"))]
    fn load_gguf(path: &Path) -> ApiResult<Self> {
        Err(ApiError::InvalidRequest(format!(
            "model {} is GGUF, but this LightFlow build has no GGUF loader; rebuild with default features or --features gguf",
            path.display()
        )))
    }
}

#[cfg(feature = "gguf")]
struct CandleGgufModel {
    #[allow(dead_code)]
    device: candle_core::Device,
    #[allow(dead_code)]
    var_builder: candle_transformers::quantized_var_builder::VarBuilder,
    #[allow(dead_code)]
    residency: ModelResidency,
}

#[cfg(feature = "gguf")]
#[allow(dead_code)]
fn candle_device() -> ApiResult<candle_core::Device> {
    #[cfg(feature = "gguf-cuda")]
    {
        return candle_core::Device::new_cuda(0).map_err(|error| {
            ApiError::InvalidRequest(format!(
                "failed to initialize Candle CUDA device 0: {error}"
            ))
        });
    }

    #[cfg(all(not(feature = "gguf-cuda"), feature = "gguf-metal"))]
    {
        return candle_core::Device::new_metal(0).map_err(|error| {
            ApiError::InvalidRequest(format!(
                "failed to initialize Candle Metal device 0: {error}"
            ))
        });
    }

    #[cfg(not(any(feature = "gguf-cuda", feature = "gguf-metal")))]
    {
        Ok(candle_core::Device::Cpu)
    }
}

#[allow(dead_code)]
fn is_gguf(path: &Path) -> bool {
    path.extension()
        .and_then(std::ffi::OsStr::to_str)
        .is_some_and(|extension| extension.eq_ignore_ascii_case("gguf"))
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
}
