//! CortexFS execution boundary.
//!
//! LightFlow owns workflow planning and run records. CortexFS owns provider,
//! model, tool, thread, policy, and audit execution surfaces under `/ctx`.

use cortex_core::ApiFormat;
use serde::de::{Error as DeError, Visitor};
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::fmt::{Display, Formatter};
use std::fs::{self, File};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::str::FromStr;

/// Canonical CortexFS mount point used by the Linux runtime path.
pub const DEFAULT_CTX_MOUNT: &str = "/ctx";

/// Request files become executable only after this suffix is present.
pub const REQUEST_SUFFIX: &str = ".req.json";

/// Staged request suffix used before the final atomic rename.
pub const STAGED_SUFFIX: &str = ".tmp";

/// Response suffix materialized by CortexFS outboxes.
pub const RESPONSE_SUFFIX: &str = ".resp.json";

/// Route metadata suffix materialized for routed API requests.
pub const ROUTE_SUFFIX: &str = ".route.json";

/// Fingerprint suffix materialized for accepted CortexFS requests.
pub const FINGERPRINT_SUFFIX: &str = ".fingerprint";

/// Error suffix materialized when CortexFS rejects or fails a request.
pub const ERROR_SUFFIX: &str = ".error";

/// User-facing CortexFS root for a Linux uid.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CortexHome {
    mount: PathBuf,
    uid: u32,
}

impl CortexHome {
    /// Build a CortexFS home from a mount path and Linux uid.
    #[must_use]
    pub fn new(mount: impl Into<PathBuf>, uid: u32) -> Self {
        Self {
            mount: mount.into(),
            uid,
        }
    }

    /// Build the default `/ctx/home/<uid>` entry.
    #[must_use]
    pub fn default_for_uid(uid: u32) -> Self {
        Self::new(DEFAULT_CTX_MOUNT, uid)
    }

    /// Build the default `/ctx/home/<current uid>` entry.
    #[must_use]
    pub fn default_for_current_user() -> Self {
        Self::default_for_uid(current_uid())
    }

    /// CortexFS mount point.
    #[must_use]
    pub fn mount(&self) -> &Path {
        &self.mount
    }

    /// Linux uid used for the CortexFS user home.
    #[must_use]
    pub const fn uid(&self) -> u32 {
        self.uid
    }

    /// `/ctx/home/<uid>`.
    #[must_use]
    pub fn home_dir(&self) -> PathBuf {
        self.mount.join("home").join(self.uid.to_string())
    }

    /// `/ctx/home/<uid>/model`.
    #[must_use]
    pub fn model_dir(&self) -> PathBuf {
        self.home_dir().join("model")
    }

    /// `/ctx/home/<uid>/route/<format>`.
    #[must_use]
    pub fn route_dir(&self, format: ApiFormat) -> PathBuf {
        self.home_dir().join("route").join(format.as_str())
    }

    /// `/ctx/audit/events.jsonl`.
    #[must_use]
    pub fn audit_events(&self) -> PathBuf {
        self.mount.join("audit").join("events.jsonl")
    }

    /// API inbox/outbox paths for a CortexFS-native format.
    #[must_use]
    pub fn api_exchange(&self, format: ApiFormat, step_id: StepId) -> CortexExchange {
        let root = self.home_dir().join("api").join(format.as_str());
        CortexExchange::new(
            CortexTarget::Api {
                format: format.into(),
            },
            step_id,
            root.join("inbox"),
            root.join("outbox"),
        )
    }

    /// Tool invocation inbox/outbox paths.
    #[must_use]
    pub fn tool_exchange(&self, tool_id: ToolId, step_id: StepId) -> CortexExchange {
        let root = self
            .mount
            .join("tool")
            .join(tool_id.as_str())
            .join("invoke");
        CortexExchange::new(
            CortexTarget::Tool {
                tool_id: tool_id.into_string(),
            },
            step_id,
            root.join("inbox"),
            root.join("outbox"),
        )
    }

    /// Thread inbox path. CortexFS records thread state, LightFlow records the workflow step.
    #[must_use]
    pub fn thread_exchange(&self, thread_id: ThreadId, step_id: StepId) -> CortexExchange {
        let inbox = self
            .home_dir()
            .join("thread")
            .join(thread_id.as_str())
            .join("inbox");
        CortexExchange::new(
            CortexTarget::Thread {
                thread_id: thread_id.into_string(),
            },
            step_id,
            inbox.clone(),
            inbox,
        )
    }
}

#[cfg(target_os = "linux")]
fn current_uid() -> u32 {
    uid_from_status().unwrap_or(0)
}

#[cfg(target_os = "linux")]
fn uid_from_status() -> Option<u32> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|line| {
        let rest = line.strip_prefix("Uid:")?;
        rest.split_whitespace().next()?.parse().ok()
    })
}

#[cfg(not(target_os = "linux"))]
fn current_uid() -> u32 {
    0
}

/// LightFlow workflow step id used as the CortexFS request id.
#[derive(Debug, Clone, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct StepId(String);

impl StepId {
    /// Create a stable step id that is safe as one path segment.
    pub fn new(value: impl Into<String>) -> Result<Self, CortexPathError> {
        let value = value.into();
        validate_segment("step_id", &value)?;
        Ok(Self(value))
    }

    /// Borrow the step id.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn into_string(self) -> String {
        self.0
    }
}

impl Display for StepId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for StepId {
    type Err = CortexPathError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

/// CortexFS tool id used as one path segment.
#[derive(Debug, Clone, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ToolId(String);

impl ToolId {
    /// Create a stable CortexFS tool id.
    pub fn new(value: impl Into<String>) -> Result<Self, CortexPathError> {
        let value = value.into();
        validate_segment("tool_id", &value)?;
        Ok(Self(value))
    }

    /// Borrow the tool id.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn into_string(self) -> String {
        self.0
    }
}

impl Display for ToolId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ToolId {
    type Err = CortexPathError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

/// CortexFS thread id used as one path segment.
#[derive(Debug, Clone, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ThreadId(String);

impl ThreadId {
    /// Create a stable CortexFS thread id.
    pub fn new(value: impl Into<String>) -> Result<Self, CortexPathError> {
        let value = value.into();
        validate_segment("thread_id", &value)?;
        Ok(Self(value))
    }

    /// Borrow the thread id.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn into_string(self) -> String {
        self.0
    }
}

impl Display for ThreadId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ThreadId {
    type Err = CortexPathError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

/// CortexFS execution target for one LightFlow step.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CortexTarget {
    Api { format: ApiFormatName },
    Tool { tool_id: String },
    Thread { thread_id: String },
}

impl From<ApiFormat> for CortexTarget {
    fn from(format: ApiFormat) -> Self {
        Self::Api {
            format: ApiFormatName::from(format),
        }
    }
}

/// Serde-friendly wrapper around CortexFS API format names.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct ApiFormatName(ApiFormat);

impl ApiFormatName {
    /// Return the CortexFS stable API format string.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        self.0.as_str()
    }

    /// Return the CortexFS core API format.
    #[must_use]
    pub const fn api_format(self) -> ApiFormat {
        self.0
    }
}

impl From<ApiFormat> for ApiFormatName {
    fn from(format: ApiFormat) -> Self {
        Self(format)
    }
}

impl Display for ApiFormatName {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl Serialize for ApiFormatName {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for ApiFormatName {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct ApiFormatNameVisitor;

        impl Visitor<'_> for ApiFormatNameVisitor {
            type Value = ApiFormatName;

            fn expecting(&self, formatter: &mut Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a CortexFS API format name")
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: DeError,
            {
                value
                    .parse::<ApiFormat>()
                    .map(ApiFormatName::from)
                    .map_err(E::custom)
            }
        }

        deserializer.deserialize_str(ApiFormatNameVisitor)
    }
}

/// File paths needed to submit one request through CortexFS.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CortexExchange {
    pub target: CortexTarget,
    pub request_id: String,
    pub inbox_dir: PathBuf,
    pub outbox_dir: PathBuf,
    pub staged_request: PathBuf,
    pub commit_request: PathBuf,
    pub response: PathBuf,
    pub error: PathBuf,
    pub fingerprint: PathBuf,
    pub route_metadata: Option<PathBuf>,
}

impl CortexExchange {
    fn new(target: CortexTarget, step_id: StepId, inbox_dir: PathBuf, outbox_dir: PathBuf) -> Self {
        let request_id = step_id.into_string();
        let staged_name = format!("{request_id}{STAGED_SUFFIX}");
        let request_name = format!("{request_id}{REQUEST_SUFFIX}");
        let response_name = format!("{request_id}{RESPONSE_SUFFIX}");
        let error_name = format!("{request_id}{ERROR_SUFFIX}");
        let fingerprint_name = format!("{request_id}{FINGERPRINT_SUFFIX}");
        let route_name = format!("{request_id}{ROUTE_SUFFIX}");
        let route_metadata =
            matches!(target, CortexTarget::Api { .. }).then(|| outbox_dir.join(route_name));

        Self {
            target,
            request_id,
            staged_request: inbox_dir.join(staged_name),
            commit_request: inbox_dir.join(request_name),
            response: outbox_dir.join(response_name),
            error: outbox_dir.join(error_name),
            fingerprint: outbox_dir.join(fingerprint_name),
            route_metadata,
            inbox_dir,
            outbox_dir,
        }
    }

    /// Submit a request body through CortexFS atomic file semantics.
    ///
    /// This writes `*.tmp`, syncs it, and renames it to `*.req.json`. CortexFS
    /// owns the execution triggered by that final rename.
    pub fn submit_request(&self, body: &[u8]) -> io::Result<CortexSubmitted> {
        fs::create_dir_all(&self.inbox_dir)?;
        let mut file = File::create(&self.staged_request)?;
        file.write_all(body)?;
        file.sync_all()?;
        drop(file);

        fs::rename(&self.staged_request, &self.commit_request)?;
        Ok(CortexSubmitted {
            request_path: self.commit_request.clone(),
            response: self.response.clone(),
            error: self.error.clone(),
            fingerprint: self.fingerprint.clone(),
            route_metadata: self.route_metadata.clone(),
        })
    }

    /// Read the response side of an already submitted exchange if present.
    pub fn read_outcome(&self) -> io::Result<Option<CortexOutcome>> {
        CortexOutcome::read_from(self)
    }
}

/// Paths returned after a request has been committed into a CortexFS inbox.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CortexSubmitted {
    pub request_path: PathBuf,
    pub response: PathBuf,
    pub error: PathBuf,
    pub fingerprint: PathBuf,
    pub route_metadata: Option<PathBuf>,
}

/// Current outcome files materialized by CortexFS.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub enum CortexOutcome {
    Response {
        body: String,
        fingerprint: Option<String>,
        route_metadata: Option<String>,
    },
    Error {
        body: String,
        fingerprint: Option<String>,
        route_metadata: Option<String>,
    },
}

impl CortexOutcome {
    fn read_from(exchange: &CortexExchange) -> io::Result<Option<Self>> {
        let fingerprint = read_optional_trimmed(&exchange.fingerprint)?;
        let route_metadata = match &exchange.route_metadata {
            Some(path) => read_optional(path)?,
            None => None,
        };

        if let Some(body) = read_optional(&exchange.response)? {
            return Ok(Some(Self::Response {
                body,
                fingerprint,
                route_metadata,
            }));
        }

        if let Some(body) = read_optional(&exchange.error)? {
            return Ok(Some(Self::Error {
                body,
                fingerprint,
                route_metadata,
            }));
        }

        Ok(None)
    }
}

fn read_optional(path: &Path) -> io::Result<Option<String>> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(error),
    }
}

fn read_optional_trimmed(path: &Path) -> io::Result<Option<String>> {
    read_optional(path).map(|content| content.map(|value| value.trim().to_owned()))
}

/// Validation error for filesystem-bound ids.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct CortexPathError {
    field: &'static str,
    value: String,
}

impl CortexPathError {
    fn new(field: &'static str, value: &str) -> Self {
        Self {
            field,
            value: value.to_owned(),
        }
    }

    /// Failed field name.
    #[must_use]
    pub const fn field(&self) -> &'static str {
        self.field
    }

    /// Rejected value.
    #[must_use]
    pub fn value(&self) -> &str {
        &self.value
    }
}

impl Display for CortexPathError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "invalid {} path segment: {}", self.field, self.value)
    }
}

impl std::error::Error for CortexPathError {}

fn validate_segment(field: &'static str, value: &str) -> Result<(), CortexPathError> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.ends_with(RESPONSE_SUFFIX)
        || value.ends_with(ERROR_SUFFIX)
    {
        return Err(CortexPathError::new(field, value));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        CortexHome, CortexOutcome, CortexTarget, ERROR_SUFFIX, FINGERPRINT_SUFFIX, REQUEST_SUFFIX,
        RESPONSE_SUFFIX, ROUTE_SUFFIX, STAGED_SUFFIX, StepId, ThreadId, ToolId,
    };
    use cortex_core::ApiFormat;
    use std::fs;
    use std::path::Path;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn default_home_uses_global_ctx_mount() {
        let home = CortexHome::default_for_uid(1000);

        assert_eq!(home.mount(), Path::new("/ctx"));
        assert_eq!(home.home_dir(), Path::new("/ctx/home/1000"));
        assert_eq!(home.model_dir(), Path::new("/ctx/home/1000/model"));
        assert_eq!(
            home.route_dir(ApiFormat::OpenAiChat),
            Path::new("/ctx/home/1000/route/openai.chat")
        );
        assert_eq!(home.audit_events(), Path::new("/ctx/audit/events.jsonl"));
    }

    #[test]
    fn api_exchange_matches_cortexfs_atomic_submission_contract() {
        let exchange = CortexHome::default_for_uid(1000)
            .api_exchange(ApiFormat::OpenAiResponses, StepId::new("plan-001").unwrap());

        assert_eq!(
            exchange.target,
            CortexTarget::Api {
                format: ApiFormat::OpenAiResponses.into()
            }
        );
        assert_eq!(
            exchange.inbox_dir,
            Path::new("/ctx/home/1000/api/openai.responses/inbox")
        );
        assert_eq!(
            exchange.outbox_dir,
            Path::new("/ctx/home/1000/api/openai.responses/outbox")
        );
        assert_eq!(
            exchange.staged_request,
            Path::new("/ctx/home/1000/api/openai.responses/inbox/plan-001.tmp")
        );
        assert_eq!(
            exchange.commit_request,
            Path::new("/ctx/home/1000/api/openai.responses/inbox/plan-001.req.json")
        );
        assert_eq!(
            exchange.response,
            Path::new("/ctx/home/1000/api/openai.responses/outbox/plan-001.resp.json")
        );
        assert_eq!(
            exchange.error,
            Path::new("/ctx/home/1000/api/openai.responses/outbox/plan-001.error")
        );
        assert_eq!(
            exchange.fingerprint,
            Path::new("/ctx/home/1000/api/openai.responses/outbox/plan-001.fingerprint")
        );
        assert_eq!(
            exchange.route_metadata,
            Some(
                Path::new("/ctx/home/1000/api/openai.responses/outbox/plan-001.route.json").into()
            )
        );
    }

    #[test]
    fn tool_exchange_does_not_claim_route_metadata() {
        let exchange = CortexHome::default_for_uid(1000).tool_exchange(
            ToolId::new("filesystem.read").unwrap(),
            StepId::new("read-file").unwrap(),
        );

        assert_eq!(
            exchange.staged_request,
            Path::new("/ctx/tool/filesystem.read/invoke/inbox/read-file.tmp")
        );
        assert_eq!(
            exchange.commit_request,
            Path::new("/ctx/tool/filesystem.read/invoke/inbox/read-file.req.json")
        );
        assert_eq!(
            exchange.response,
            Path::new("/ctx/tool/filesystem.read/invoke/outbox/read-file.resp.json")
        );
        assert_eq!(exchange.route_metadata, None);
    }

    #[test]
    fn thread_exchange_uses_thread_inbox_without_parallel_provider_surface() {
        let exchange = CortexHome::default_for_uid(1000).thread_exchange(
            ThreadId::new("design-review").unwrap(),
            StepId::new("turn-1").unwrap(),
        );

        assert_eq!(
            exchange.commit_request,
            Path::new("/ctx/home/1000/thread/design-review/inbox/turn-1.req.json")
        );
        assert_eq!(
            exchange.outbox_dir,
            Path::new("/ctx/home/1000/thread/design-review/inbox")
        );
        assert_eq!(exchange.route_metadata, None);
    }

    #[test]
    fn path_ids_reject_slashes_response_and_error_names() {
        assert!(StepId::new("node/a").is_err());
        assert!(StepId::new("node.resp.json").is_err());
        assert!(StepId::new("node.error").is_err());
        assert!(ToolId::new("").is_err());
        assert!(ThreadId::new("..").is_err());
    }

    #[test]
    fn suffix_constants_match_cortexfs_abi() {
        assert_eq!(STAGED_SUFFIX, ".tmp");
        assert_eq!(REQUEST_SUFFIX, ".req.json");
        assert_eq!(RESPONSE_SUFFIX, ".resp.json");
        assert_eq!(ROUTE_SUFFIX, ".route.json");
        assert_eq!(FINGERPRINT_SUFFIX, ".fingerprint");
        assert_eq!(ERROR_SUFFIX, ".error");
    }

    #[test]
    fn submit_request_writes_tmp_then_renames_to_req_json() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = unique_temp_root();
        let exchange = CortexHome::new(&root, 1000)
            .api_exchange(ApiFormat::OpenAiChat, StepId::new("submit-001")?);

        let submitted = exchange.submit_request(br#"{"model":"demo"}"#)?;

        assert_eq!(submitted.request_path, exchange.commit_request);
        assert!(!exchange.staged_request.exists());
        assert_eq!(
            fs::read_to_string(&exchange.commit_request)?,
            r#"{"model":"demo"}"#
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn read_outcome_prefers_response_with_fingerprint_and_route()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = unique_temp_root();
        let exchange = CortexHome::new(&root, 1000)
            .api_exchange(ApiFormat::OpenAiChat, StepId::new("outcome-001")?);
        fs::create_dir_all(&exchange.outbox_dir)?;
        fs::write(&exchange.response, "{\"ok\":true}\n")?;
        fs::write(&exchange.fingerprint, "fnv1a64:abc\n")?;
        fs::write(
            exchange
                .route_metadata
                .as_ref()
                .ok_or("route metadata missing")?,
            "{\"provider\":\"local\"}\n",
        )?;

        let outcome = exchange.read_outcome()?;

        assert_eq!(
            outcome,
            Some(CortexOutcome::Response {
                body: "{\"ok\":true}\n".to_owned(),
                fingerprint: Some("fnv1a64:abc".to_owned()),
                route_metadata: Some("{\"provider\":\"local\"}\n".to_owned()),
            })
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn read_outcome_returns_error_when_error_file_exists() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = unique_temp_root();
        let exchange = CortexHome::new(&root, 1000)
            .tool_exchange(ToolId::new("filesystem.read")?, StepId::new("tool-error")?);
        fs::create_dir_all(&exchange.outbox_dir)?;
        fs::write(&exchange.error, "denied\n")?;

        let outcome = exchange.read_outcome()?;

        assert_eq!(
            outcome,
            Some(CortexOutcome::Error {
                body: "denied\n".to_owned(),
                fingerprint: None,
                route_metadata: None,
            })
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn read_outcome_returns_none_before_cortexfs_materializes_files()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = unique_temp_root();
        let exchange = CortexHome::new(&root, 1000)
            .api_exchange(ApiFormat::OpenAiChat, StepId::new("pending")?);

        assert_eq!(exchange.read_outcome()?, None);

        if root.exists() {
            fs::remove_dir_all(root)?;
        }
        Ok(())
    }

    fn unique_temp_root() -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock must be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!(
            "lightflow-cortex-test-{}-{nanos}",
            std::process::id()
        ))
    }
}
