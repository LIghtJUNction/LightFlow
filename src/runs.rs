//! LightFlow run records.
//!
//! Run records are LightFlow-owned state because they describe workflow
//! structure. Provider/model/tool/thread execution details remain references
//! to CortexFS paths and metadata.

use crate::cortex::{CortexExchange, CortexOutcome, CortexSubmitted};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::env;
use std::fmt::{Display, Formatter};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::Path;
use std::path::PathBuf;

/// Run manifest filename under each run directory.
pub const MANIFEST_FILE: &str = "manifest.json";

/// Original run creation request filename.
pub const REQUEST_FILE: &str = "request.json";

/// Planned workflow definition filename.
pub const RESOLVED_WORKFLOW_FILE: &str = "resolved_workflow.json";

/// Run event stream filename.
pub const EVENTS_FILE: &str = "events.jsonl";

/// Run trace stream filename.
pub const TRACE_FILE: &str = "trace.jsonl";

/// Output artifact directory under each run directory.
pub const OUTPUTS_DIR: &str = "outputs";

/// LightFlow run id used as one XDG state path segment.
#[derive(Debug, Clone, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct RunId(String);

impl RunId {
    /// Create a run id safe for use as one path segment.
    pub fn new(value: impl Into<String>) -> Result<Self, RunIdError> {
        let value = value.into();
        if value.is_empty() || value == "." || value == ".." || value.contains('/') {
            return Err(RunIdError(value));
        }
        Ok(Self(value))
    }

    /// Borrow the run id.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for RunId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Rejected run id.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RunIdError(String);

impl Display for RunIdError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid run id path segment: {}", self.0)
    }
}

impl std::error::Error for RunIdError {}

/// XDG-backed LightFlow directories.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RuntimeDirs {
    pub config_home: PathBuf,
    pub state_home: PathBuf,
    pub cache_home: PathBuf,
    pub runtime_dir: PathBuf,
}

impl RuntimeDirs {
    /// Resolve the Linux/XDG runtime directories for the current process env.
    #[must_use]
    pub fn from_env() -> Self {
        let home = env::var_os("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("."));
        let config_home = env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".config"));
        let state_home = env::var_os("XDG_STATE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".local").join("state"));
        let cache_home = env::var_os("XDG_CACHE_HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|| home.join(".cache"));
        let runtime_dir = env::var_os("XDG_RUNTIME_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|| env::temp_dir().join(format!("lightflow-{}", std::process::id())));

        Self::new(config_home, state_home, cache_home, runtime_dir)
    }

    /// Build explicit runtime directories. Useful for tests and sandboxes.
    #[must_use]
    pub fn new(
        config_home: impl Into<PathBuf>,
        state_home: impl Into<PathBuf>,
        cache_home: impl Into<PathBuf>,
        runtime_dir: impl Into<PathBuf>,
    ) -> Self {
        Self {
            config_home: config_home.into().join("lightflow"),
            state_home: state_home.into().join("lightflow"),
            cache_home: cache_home.into().join("lightflow"),
            runtime_dir: runtime_dir.into().join("lightflow"),
        }
    }

    /// `$XDG_STATE_HOME/lightflow/runs/<run_id>`.
    #[must_use]
    pub fn run_dir(&self, run_id: &RunId) -> PathBuf {
        self.state_home.join("runs").join(run_id.as_str())
    }

    /// `$XDG_STATE_HOME/lightflow/runs/<run_id>/manifest.json`.
    #[must_use]
    pub fn run_manifest_path(&self, run_id: &RunId) -> PathBuf {
        self.run_dir(run_id).join(MANIFEST_FILE)
    }

    /// `$XDG_STATE_HOME/lightflow/runs/<run_id>/outputs`.
    #[must_use]
    pub fn run_outputs_dir(&self, run_id: &RunId) -> PathBuf {
        self.run_dir(run_id).join(OUTPUTS_DIR)
    }

    /// `$XDG_STATE_HOME/lightflow/runs/<run_id>/request.json`.
    #[must_use]
    pub fn run_request_path(&self, run_id: &RunId) -> PathBuf {
        self.run_dir(run_id).join(REQUEST_FILE)
    }

    /// `$XDG_STATE_HOME/lightflow/runs/<run_id>/resolved_workflow.json`.
    #[must_use]
    pub fn run_resolved_workflow_path(&self, run_id: &RunId) -> PathBuf {
        self.run_dir(run_id).join(RESOLVED_WORKFLOW_FILE)
    }

    /// `$XDG_STATE_HOME/lightflow/runs/<run_id>/events.jsonl`.
    #[must_use]
    pub fn run_events_path(&self, run_id: &RunId) -> PathBuf {
        self.run_dir(run_id).join(EVENTS_FILE)
    }

    /// `$XDG_STATE_HOME/lightflow/runs/<run_id>/trace.jsonl`.
    #[must_use]
    pub fn run_trace_path(&self, run_id: &RunId) -> PathBuf {
        self.run_dir(run_id).join(TRACE_FILE)
    }
}

/// Filesystem-backed run store under XDG state.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct RunStore {
    dirs: RuntimeDirs,
}

impl RunStore {
    /// Create a store rooted in the supplied XDG directories.
    #[must_use]
    pub const fn new(dirs: RuntimeDirs) -> Self {
        Self { dirs }
    }

    /// Runtime directories used by the store.
    #[must_use]
    pub const fn dirs(&self) -> &RuntimeDirs {
        &self.dirs
    }

    /// Create or replace a run manifest with atomic rename semantics.
    pub fn put_manifest(&self, manifest: &RunManifest) -> io::Result<PathBuf> {
        let run_dir = self.dirs.run_dir(&manifest.run_id);
        fs::create_dir_all(self.dirs.run_outputs_dir(&manifest.run_id))?;

        let manifest_path = run_dir.join(MANIFEST_FILE);
        write_manifest_atomic(&manifest_path, manifest)?;
        Ok(manifest_path)
    }

    /// Read a run manifest by id.
    pub fn get_manifest(&self, run_id: &RunId) -> io::Result<RunManifest> {
        read_json(&self.dirs.run_manifest_path(run_id))
    }

    /// Read the original create-run request.
    pub fn get_request<T: DeserializeOwned>(&self, run_id: &RunId) -> io::Result<T> {
        read_json(&self.dirs.run_request_path(run_id))
    }

    /// Read the resolved workflow definition.
    pub fn get_resolved_workflow<T: DeserializeOwned>(&self, run_id: &RunId) -> io::Result<T> {
        read_json(&self.dirs.run_resolved_workflow_path(run_id))
    }

    /// Write the original create-run request as inspectable JSON.
    pub fn put_request<T: Serialize>(&self, run_id: &RunId, request: &T) -> io::Result<PathBuf> {
        let path = self.dirs.run_request_path(run_id);
        write_json_atomic(&path, request)?;
        Ok(path)
    }

    /// Write the resolved workflow definition as inspectable JSON.
    pub fn put_resolved_workflow<T: Serialize>(
        &self,
        run_id: &RunId,
        definition: &T,
    ) -> io::Result<PathBuf> {
        let path = self.dirs.run_resolved_workflow_path(run_id);
        write_json_atomic(&path, definition)?;
        Ok(path)
    }

    /// Append one JSON event line to `events.jsonl`.
    pub fn append_event(&self, run_id: &RunId, event: RunEvent<'_>) -> io::Result<PathBuf> {
        let path = self.dirs.run_events_path(run_id);
        append_json_line(&path, &event)?;
        Ok(path)
    }

    /// Append one JSON trace line to `trace.jsonl`.
    pub fn append_trace(&self, run_id: &RunId, trace: RunTrace<'_>) -> io::Result<PathBuf> {
        let path = self.dirs.run_trace_path(run_id);
        append_json_line(&path, &trace)?;
        Ok(path)
    }

    /// Read the run event stream as raw JSONL text.
    pub fn events(&self, run_id: &RunId) -> io::Result<String> {
        read_text_or_empty(&self.dirs.run_events_path(run_id))
    }

    /// Read the run trace stream as raw JSONL text.
    pub fn trace(&self, run_id: &RunId) -> io::Result<String> {
        read_text_or_empty(&self.dirs.run_trace_path(run_id))
    }
}

fn write_manifest_atomic(path: &Path, manifest: &RunManifest) -> io::Result<()> {
    write_json_atomic(path, manifest)
}

fn write_json_atomic(path: &Path, value: &impl Serialize) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "json path has no parent"))?;
    fs::create_dir_all(parent)?;

    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "json path has no file name"))?;
    let temp_path = parent.join(format!("{file_name}.tmp"));
    let mut file = File::create(&temp_path)?;
    serde_json::to_writer_pretty(&mut file, value).map_err(io::Error::other)?;
    file.write_all(b"\n")?;
    file.sync_all()?;
    drop(file);

    fs::rename(temp_path, path)?;
    Ok(())
}

fn read_json<T: DeserializeOwned>(path: &Path) -> io::Result<T> {
    let file = File::open(path)?;
    serde_json::from_reader(file).map_err(io::Error::other)
}

fn append_json_line(path: &Path, value: &impl Serialize) -> io::Result<()> {
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "jsonl path has no parent"))?;
    fs::create_dir_all(parent)?;
    let mut file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)?;
    serde_json::to_writer(&mut file, value).map_err(io::Error::other)?;
    file.write_all(b"\n")?;
    file.sync_data()?;
    Ok(())
}

fn read_text_or_empty(path: &Path) -> io::Result<String> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(content),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(String::new()),
        Err(error) => Err(error),
    }
}

/// One append-only run event.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct RunEvent<'a> {
    pub event: &'a str,
    pub run_id: &'a str,
    pub step_id: Option<&'a str>,
    pub detail: Option<&'a str>,
}

/// One append-only run trace item.
#[derive(Debug, Clone, Eq, PartialEq, Serialize)]
pub struct RunTrace<'a> {
    pub event: &'a str,
    pub step_id: &'a str,
    pub path: Option<&'a Path>,
}

/// Serializable run manifest written under a LightFlow run directory.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RunManifest {
    pub run_id: RunId,
    pub workflow_asset_id: String,
    pub steps: Vec<RunStepRecord>,
}

impl RunManifest {
    /// Create an empty run manifest for a workflow asset.
    #[must_use]
    pub fn new(run_id: RunId, workflow_asset_id: impl Into<String>) -> Self {
        Self {
            run_id,
            workflow_asset_id: workflow_asset_id.into(),
            steps: Vec::new(),
        }
    }

    /// Add a step record that points at CortexFS-owned execution artifacts.
    pub fn push_step(&mut self, step: RunStepRecord) {
        self.steps.push(step);
    }
}

/// LightFlow-owned record for one workflow step.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct RunStepRecord {
    pub step_id: String,
    pub node_or_composition_id: String,
    pub cortex: CortexExchange,
    pub status: RunStepStatus,
    pub submitted_request_path: Option<PathBuf>,
    pub response_path: Option<PathBuf>,
    pub error_path: Option<PathBuf>,
    pub provider_id: Option<String>,
    pub model_id: Option<String>,
    pub route_decision: Option<String>,
    pub fingerprint: Option<String>,
    pub audit_correlation: Option<String>,
    pub output_artifacts: Vec<PathBuf>,
}

impl RunStepRecord {
    /// Create a step record from a planned CortexFS exchange.
    #[must_use]
    pub fn planned(node_or_composition_id: impl Into<String>, cortex: CortexExchange) -> Self {
        Self {
            step_id: cortex.request_id.clone(),
            node_or_composition_id: node_or_composition_id.into(),
            cortex,
            status: RunStepStatus::Planned,
            submitted_request_path: None,
            response_path: None,
            error_path: None,
            provider_id: None,
            model_id: None,
            route_decision: None,
            fingerprint: None,
            audit_correlation: None,
            output_artifacts: Vec::new(),
        }
    }

    /// Mark the step as committed into a CortexFS inbox.
    pub fn mark_submitted(&mut self, submitted: CortexSubmitted) {
        self.status = RunStepStatus::Submitted;
        self.submitted_request_path = Some(submitted.request_path);
        self.response_path = None;
        self.error_path = None;
    }

    /// Apply the current CortexFS outbox state to this run step.
    pub fn apply_outcome(&mut self, outcome: CortexOutcome) {
        match outcome {
            CortexOutcome::Response {
                fingerprint,
                route_metadata,
                ..
            } => {
                self.status = RunStepStatus::Succeeded;
                self.response_path = Some(self.cortex.response.clone());
                self.error_path = None;
                self.push_output_artifact(self.cortex.response.clone());
                self.fingerprint = fingerprint;
                self.apply_route_metadata(route_metadata.as_deref());
            }
            CortexOutcome::Error {
                fingerprint,
                route_metadata,
                ..
            } => {
                self.status = RunStepStatus::Failed;
                self.error_path = Some(self.cortex.error.clone());
                self.response_path = None;
                self.push_output_artifact(self.cortex.error.clone());
                self.fingerprint = fingerprint;
                self.apply_route_metadata(route_metadata.as_deref());
            }
        }
    }

    fn push_output_artifact(&mut self, path: PathBuf) {
        if !self
            .output_artifacts
            .iter()
            .any(|existing| existing == &path)
        {
            self.output_artifacts.push(path);
        }
    }

    fn apply_route_metadata(&mut self, route_metadata: Option<&str>) {
        let Some(route_metadata) = route_metadata else {
            return;
        };
        let Ok(value) = serde_json::from_str::<serde_json::Value>(route_metadata) else {
            return;
        };
        self.provider_id = value
            .get("provider")
            .and_then(serde_json::Value::as_str)
            .filter(|provider| !provider.is_empty())
            .map(ToOwned::to_owned);
        self.model_id = value
            .get("model")
            .and_then(serde_json::Value::as_str)
            .filter(|model| !model.is_empty())
            .map(ToOwned::to_owned);
        self.route_decision = value
            .get("reason")
            .and_then(serde_json::Value::as_str)
            .filter(|reason| !reason.is_empty())
            .map(ToOwned::to_owned);
    }
}

/// Current lifecycle state for a LightFlow run step.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RunStepStatus {
    Planned,
    Submitted,
    Succeeded,
    Failed,
}

#[cfg(test)]
mod tests {
    use super::{
        MANIFEST_FILE, OUTPUTS_DIR, RunId, RunManifest, RunStepRecord, RunStore, RuntimeDirs,
    };
    use crate::cortex::{CortexHome, StepId};
    use cortex_core::ApiFormat;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn explicit_runtime_dirs_are_xdg_scoped_under_lightflow() {
        let dirs = RuntimeDirs::new("/cfg", "/state", "/cache", "/run/user/1000");
        let run_id = RunId::new("run-001").unwrap();

        assert_eq!(dirs.config_home, Path::new("/cfg/lightflow"));
        assert_eq!(dirs.state_home, Path::new("/state/lightflow"));
        assert_eq!(dirs.cache_home, Path::new("/cache/lightflow"));
        assert_eq!(dirs.runtime_dir, Path::new("/run/user/1000/lightflow"));
        assert_eq!(
            dirs.run_dir(&run_id),
            Path::new("/state/lightflow/runs/run-001")
        );
    }

    #[test]
    fn run_id_rejects_nested_paths() {
        assert!(RunId::new("today/001").is_err());
        assert!(RunId::new(".").is_err());
        assert_eq!(RunId::new("today-001").unwrap().as_str(), "today-001");
    }

    #[test]
    fn manifest_records_workflow_structure_and_cortexfs_correlation_paths() {
        let exchange = CortexHome::default_for_uid(1000)
            .api_exchange(ApiFormat::OpenAiChat, StepId::new("draft").unwrap());
        let mut step = RunStepRecord::planned("node.llm.prompt", exchange);
        step.provider_id = Some("local".to_owned());
        step.model_id = Some("smollm2:135m".to_owned());
        step.route_decision = Some("default_provider".to_owned());
        step.fingerprint = Some("fnv1a64:1234".to_owned());
        step.audit_correlation = Some("/ctx/audit/events.jsonl#draft".to_owned());

        let mut manifest =
            RunManifest::new(RunId::new("run-001").unwrap(), "workflow.text-to-plan");
        manifest.push_step(step);

        let json = serde_json::to_value(&manifest).unwrap();
        assert_eq!(json["workflow_asset_id"], "workflow.text-to-plan");
        assert_eq!(json["steps"][0]["step_id"], "draft");
        assert_eq!(
            json["steps"][0]["node_or_composition_id"],
            "node.llm.prompt"
        );
        assert_eq!(
            json["steps"][0]["cortex"]["commit_request"],
            "/ctx/home/1000/api/openai.chat/inbox/draft.req.json"
        );
        assert_eq!(
            json["steps"][0]["cortex"]["route_metadata"],
            "/ctx/home/1000/api/openai.chat/outbox/draft.route.json"
        );
        assert_eq!(
            json["steps"][0]["audit_correlation"],
            "/ctx/audit/events.jsonl#draft"
        );
    }

    #[test]
    fn run_store_writes_manifest_atomically_under_xdg_state()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = unique_temp_root();
        let dirs = RuntimeDirs::new(
            root.join("cfg"),
            root.join("state"),
            root.join("cache"),
            root.join("runtime"),
        );
        let run_id = RunId::new("run-store-001")?;
        let manifest = RunManifest::new(run_id.clone(), "workflow.echo");
        let store = RunStore::new(dirs.clone());

        let manifest_path = store.put_manifest(&manifest)?;
        let loaded = store.get_manifest(&run_id)?;

        assert_eq!(manifest_path, dirs.run_dir(&run_id).join(MANIFEST_FILE));
        assert_eq!(manifest, loaded);
        assert!(dirs.run_outputs_dir(&run_id).is_dir());
        assert_eq!(
            dirs.run_outputs_dir(&run_id),
            dirs.run_dir(&run_id).join(OUTPUTS_DIR)
        );
        assert!(!dirs.run_dir(&run_id).join("manifest.json.tmp").exists());

        std::fs::remove_dir_all(root)?;
        Ok(())
    }

    fn unique_temp_root() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "lightflow-runs-test-{}-{nanos}",
            std::process::id()
        ))
    }
}
