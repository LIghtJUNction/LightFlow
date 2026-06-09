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

/// CortexFS structured job specification file name.
pub const JOB_SPEC_FILE: &str = "spec";

/// CortexFS structured job request file name. Writing this file triggers the job.
pub const JOB_REQUEST_FILE: &str = "req";

/// CortexFS structured job JSON output file name.
pub const JOB_OUTPUT_FILE: &str = "out.json";

/// CortexFS structured job status file name.
pub const JOB_STATUS_FILE: &str = "status";

/// CortexFS hook trigger file name.
pub const HOOK_TRIGGER_FILE: &str = "trigger";

/// CortexFS hook specification file name.
pub const HOOK_SPEC_FILE: &str = "spec";

/// CortexFS hook request file name.
pub const HOOK_REQUEST_FILE: &str = "req";

/// CortexFS hook JSON output file name.
pub const HOOK_OUTPUT_FILE: &str = "out.json";

/// CortexFS hook status file name.
pub const HOOK_STATUS_FILE: &str = "status";

/// CortexFS hook last-run summary file name.
pub const HOOK_LAST_FILE: &str = "last";

/// CortexFS hook JSONL log file name.
pub const HOOK_LOG_FILE: &str = "log.jsonl";

/// Runtime channel root exposed by CortexFS.
pub const CHANNEL_DIR: &str = "chan";

/// Where CortexFS execution lives.
pub const CTX_MODE: &str = "userspace";

/// Kernel interface CortexFS uses.
pub const CTX_KERNEL: &str = "fuse";

/// Kernel upstream scope.
pub const CTX_UPSTREAM: &str = "generic kernel primitives only";

/// Human-readable kernel policy.
pub const CTX_POLICY: &str = "Keep LightFlow/CortexFS runtime protocols in userspace; upstream only small generic Linux primitives with tests and benchmarks.";

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

    /// Describe the CortexFS userspace ABI.
    #[must_use]
    pub fn abi(&self) -> CtxAbi {
        CtxAbi {
            mount: self.mount.clone(),
            uid: self.uid,
            home: self.home_dir(),
            mode: CTX_MODE.to_owned(),
            kernel: CTX_KERNEL.to_owned(),
            kernel_tree: false,
            abi: "userspace".to_owned(),
            upstream: CTX_UPSTREAM.to_owned(),
            policy: CTX_POLICY.to_owned(),
            non_goals: vec![
                "LightFlow runtime in the Linux kernel tree".to_owned(),
                "provider/model/MCP/HTTP/OpenAPI/JSON protocols as kernel ABI".to_owned(),
                "policy, routing, secrets, or tool execution in kernel space".to_owned(),
            ],
        }
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

    /// `/ctx/chan`.
    #[must_use]
    pub fn channel_dir(&self) -> PathBuf {
        self.mount.join(CHANNEL_DIR)
    }

    /// Runtime channel configuration and status paths.
    #[must_use]
    pub fn channel_paths(&self, channel_id: ChannelId) -> CortexChannelPaths {
        let channel_id = channel_id.into_string();
        let root = self.channel_dir().join(&channel_id);
        CortexChannelPaths {
            channel_id,
            root: root.clone(),
            url: root.join("url"),
            keyref: root.join("keyref"),
            formats: root.join("fmt"),
            model_filter: root.join("mod"),
            enabled: root.join("enabled"),
            status: root.join("status"),
            local_url: root.join("localurl"),
        }
    }

    /// `/ctx/home/<uid>/job`.
    #[must_use]
    pub fn job_dir(&self) -> PathBuf {
        self.home_dir().join("job")
    }

    /// Structured CortexFS job paths.
    #[must_use]
    pub fn job_exchange(&self, job_id: JobId) -> CortexJobExchange {
        CortexJobExchange::new(job_id, self.job_dir())
    }

    /// `/ctx/home/<uid>/hook`.
    #[must_use]
    pub fn hook_dir(&self) -> PathBuf {
        self.home_dir().join("hook")
    }

    /// CortexFS hook paths.
    #[must_use]
    pub fn hook_exchange(&self, hook_id: HookId) -> CortexHookExchange {
        CortexHookExchange::new(hook_id, self.hook_dir())
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

/// Public `/ctx` ABI contract.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CtxAbi {
    pub mount: PathBuf,
    pub uid: u32,
    pub home: PathBuf,
    pub mode: String,
    pub kernel: String,
    pub kernel_tree: bool,
    pub abi: String,
    pub upstream: String,
    pub policy: String,
    pub non_goals: Vec<String>,
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

/// CortexFS runtime channel id used as one virtual directory name.
#[derive(Debug, Clone, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ChannelId(String);

impl ChannelId {
    /// Create a stable CortexFS channel id.
    pub fn new(value: impl Into<String>) -> Result<Self, CortexPathError> {
        let value = value.into();
        validate_virtual_id("channel_id", &value)?;
        Ok(Self(value))
    }

    /// Borrow the channel id.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn into_string(self) -> String {
        self.0
    }
}

impl Display for ChannelId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for ChannelId {
    type Err = CortexPathError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

/// CortexFS structured job id used as one virtual directory name.
#[derive(Debug, Clone, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct JobId(String);

impl JobId {
    /// Create a stable CortexFS structured job id.
    pub fn new(value: impl Into<String>) -> Result<Self, CortexPathError> {
        let value = value.into();
        validate_virtual_id("job_id", &value)?;
        Ok(Self(value))
    }

    /// Borrow the job id.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn into_string(self) -> String {
        self.0
    }
}

impl Display for JobId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for JobId {
    type Err = CortexPathError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::new(value)
    }
}

/// CortexFS hook id used as one virtual directory name.
#[derive(Debug, Clone, Eq, Hash, Ord, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct HookId(String);

impl HookId {
    /// Create a stable CortexFS hook id.
    pub fn new(value: impl Into<String>) -> Result<Self, CortexPathError> {
        let value = value.into();
        validate_virtual_id("hook_id", &value)?;
        Ok(Self(value))
    }

    /// Borrow the hook id.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }

    fn into_string(self) -> String {
        self.0
    }
}

impl Display for HookId {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl FromStr for HookId {
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

/// File paths needed to configure one CortexFS runtime channel.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CortexChannelPaths {
    pub channel_id: String,
    pub root: PathBuf,
    pub url: PathBuf,
    pub keyref: PathBuf,
    pub formats: PathBuf,
    pub model_filter: PathBuf,
    pub enabled: PathBuf,
    pub status: PathBuf,
    pub local_url: PathBuf,
}

/// File paths needed to run one CortexFS structured job.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CortexJobExchange {
    pub target: CortexJobTarget,
    pub job_id: String,
    pub job_dir: PathBuf,
    pub spec: PathBuf,
    pub request: PathBuf,
    pub output: PathBuf,
    pub status: PathBuf,
}

impl CortexJobExchange {
    fn new(job_id: JobId, job_root: PathBuf) -> Self {
        let job_id = job_id.into_string();
        let job_dir = job_root.join(&job_id);
        Self {
            target: CortexJobTarget {
                job_id: job_id.clone(),
            },
            job_id,
            spec: job_dir.join(JOB_SPEC_FILE),
            request: job_dir.join(JOB_REQUEST_FILE),
            output: job_dir.join(JOB_OUTPUT_FILE),
            status: job_dir.join(JOB_STATUS_FILE),
            job_dir,
        }
    }

    /// Submit a structured job through CortexFS file semantics.
    ///
    /// CortexFS creates a virtual `home/<uid>/job/<id>` directory, reads `spec`,
    /// and runs the job when `req` is written.
    pub fn submit_job(&self, spec: &[u8], request: &[u8]) -> io::Result<CortexJobSubmitted> {
        fs::create_dir_all(&self.job_dir)?;
        write_synced(&self.spec, spec)?;
        write_synced(&self.request, request)?;
        Ok(CortexJobSubmitted {
            spec: self.spec.clone(),
            request: self.request.clone(),
            output: self.output.clone(),
            status: self.status.clone(),
        })
    }

    /// Read the current output/status side of an already submitted structured job.
    pub fn read_job_outcome(&self) -> io::Result<CortexJobOutcome> {
        Ok(CortexJobOutcome {
            output: read_optional(&self.output)?,
            status: read_optional_trimmed(&self.status)?,
        })
    }
}

/// CortexFS structured job target.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CortexJobTarget {
    pub job_id: String,
}

/// Paths returned after a structured job has been submitted.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CortexJobSubmitted {
    pub spec: PathBuf,
    pub request: PathBuf,
    pub output: PathBuf,
    pub status: PathBuf,
}

/// Current files materialized by a CortexFS structured job.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CortexJobOutcome {
    pub output: Option<String>,
    pub status: Option<String>,
}

/// File paths needed to invoke one CortexFS hook.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CortexHookExchange {
    pub target: CortexHookTarget,
    pub hook_id: String,
    pub hook_dir: PathBuf,
    pub trigger: PathBuf,
    pub spec: PathBuf,
    pub request: PathBuf,
    pub output: PathBuf,
    pub status: PathBuf,
    pub last: PathBuf,
    pub log: PathBuf,
}

impl CortexHookExchange {
    fn new(hook_id: HookId, hook_root: PathBuf) -> Self {
        let hook_id = hook_id.into_string();
        let hook_dir = hook_root.join(&hook_id);
        Self {
            target: CortexHookTarget {
                hook_id: hook_id.clone(),
            },
            hook_id,
            trigger: hook_dir.join(HOOK_TRIGGER_FILE),
            spec: hook_dir.join(HOOK_SPEC_FILE),
            request: hook_dir.join(HOOK_REQUEST_FILE),
            output: hook_dir.join(HOOK_OUTPUT_FILE),
            status: hook_dir.join(HOOK_STATUS_FILE),
            last: hook_dir.join(HOOK_LAST_FILE),
            log: hook_dir.join(HOOK_LOG_FILE),
            hook_dir,
        }
    }

    /// Submit a hook through CortexFS file semantics.
    ///
    /// External schedulers trigger hooks by writing `req`; CortexFS exposes
    /// `out.json`, `status`, `last`, and `log.jsonl` as the hook outcome.
    pub fn submit_hook(&self, request: &[u8]) -> io::Result<CortexHookSubmitted> {
        fs::create_dir_all(&self.hook_dir)?;
        write_synced(&self.request, request)?;
        Ok(CortexHookSubmitted {
            request: self.request.clone(),
            output: self.output.clone(),
            status: self.status.clone(),
            last: self.last.clone(),
            log: self.log.clone(),
        })
    }

    /// Read the current output/status side of an already submitted hook.
    pub fn read_hook_outcome(&self) -> io::Result<CortexHookOutcome> {
        Ok(CortexHookOutcome {
            output: read_optional(&self.output)?,
            status: read_optional_trimmed(&self.status)?,
            last: read_optional_trimmed(&self.last)?,
            log: read_optional(&self.log)?,
        })
    }
}

/// CortexFS hook target.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CortexHookTarget {
    pub hook_id: String,
}

/// Paths returned after a hook request has been submitted.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CortexHookSubmitted {
    pub request: PathBuf,
    pub output: PathBuf,
    pub status: PathBuf,
    pub last: PathBuf,
    pub log: PathBuf,
}

/// Current files materialized by a CortexFS hook.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct CortexHookOutcome {
    pub output: Option<String>,
    pub status: Option<String>,
    pub last: Option<String>,
    pub log: Option<String>,
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

fn write_synced(path: &Path, body: &[u8]) -> io::Result<()> {
    let mut file = File::create(path)?;
    file.write_all(body)?;
    file.sync_all()?;
    Ok(())
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

fn validate_virtual_id(field: &'static str, value: &str) -> Result<(), CortexPathError> {
    let valid = !value.is_empty()
        && value.len() <= 64
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-'));
    if valid {
        Ok(())
    } else {
        Err(CortexPathError::new(field, value))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ChannelId, CortexHome, CortexOutcome, CortexTarget, ERROR_SUFFIX, FINGERPRINT_SUFFIX,
        HOOK_LAST_FILE, HOOK_LOG_FILE, HOOK_OUTPUT_FILE, HOOK_REQUEST_FILE, HOOK_SPEC_FILE,
        HOOK_STATUS_FILE, HOOK_TRIGGER_FILE, HookId, JOB_OUTPUT_FILE, JOB_REQUEST_FILE,
        JOB_SPEC_FILE, JOB_STATUS_FILE, JobId, REQUEST_SUFFIX, RESPONSE_SUFFIX, ROUTE_SUFFIX,
        STAGED_SUFFIX, StepId, ThreadId, ToolId,
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
        assert_eq!(home.channel_dir(), Path::new("/ctx/chan"));
        assert_eq!(home.job_dir(), Path::new("/ctx/home/1000/job"));
        assert_eq!(home.hook_dir(), Path::new("/ctx/home/1000/hook"));
    }

    #[test]
    fn ctx_abi_keeps_ai_runtime_out_of_kernel_abi() {
        let abi = CortexHome::default_for_uid(1000).abi();

        assert_eq!(abi.mount, Path::new("/ctx"));
        assert_eq!(abi.home, Path::new("/ctx/home/1000"));
        assert_eq!(abi.mode, "userspace");
        assert_eq!(abi.kernel, "fuse");
        assert!(!abi.kernel_tree);
        assert_eq!(abi.abi, "userspace");
        assert_eq!(abi.upstream, "generic kernel primitives only");
        assert!(
            abi.non_goals
                .iter()
                .any(|goal| goal.contains("LightFlow runtime"))
        );
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
    fn channel_paths_match_cortexfs_file_channel_abi() {
        let paths =
            CortexHome::default_for_uid(1000).channel_paths(ChannelId::new("fengying").unwrap());

        assert_eq!(paths.root, Path::new("/ctx/chan/fengying"));
        assert_eq!(paths.url, Path::new("/ctx/chan/fengying/url"));
        assert_eq!(paths.keyref, Path::new("/ctx/chan/fengying/keyref"));
        assert_eq!(paths.formats, Path::new("/ctx/chan/fengying/fmt"));
        assert_eq!(paths.model_filter, Path::new("/ctx/chan/fengying/mod"));
        assert_eq!(paths.enabled, Path::new("/ctx/chan/fengying/enabled"));
        assert_eq!(paths.status, Path::new("/ctx/chan/fengying/status"));
        assert_eq!(paths.local_url, Path::new("/ctx/chan/fengying/localurl"));
    }

    #[test]
    fn job_exchange_matches_cortexfs_structured_job_abi() {
        let job =
            CortexHome::default_for_uid(1000).job_exchange(JobId::new("translate.zh").unwrap());

        assert_eq!(job.job_dir, Path::new("/ctx/home/1000/job/translate.zh"));
        assert_eq!(job.spec, Path::new("/ctx/home/1000/job/translate.zh/spec"));
        assert_eq!(
            job.request,
            Path::new("/ctx/home/1000/job/translate.zh/req")
        );
        assert_eq!(
            job.output,
            Path::new("/ctx/home/1000/job/translate.zh/out.json")
        );
        assert_eq!(
            job.status,
            Path::new("/ctx/home/1000/job/translate.zh/status")
        );
    }

    #[test]
    fn hook_exchange_matches_cortexfs_hook_abi() {
        let hook = CortexHome::default_for_uid(1000)
            .hook_exchange(HookId::new("daily-translate").unwrap());

        assert_eq!(
            hook.hook_dir,
            Path::new("/ctx/home/1000/hook/daily-translate")
        );
        assert_eq!(
            hook.trigger,
            Path::new("/ctx/home/1000/hook/daily-translate/trigger")
        );
        assert_eq!(
            hook.spec,
            Path::new("/ctx/home/1000/hook/daily-translate/spec")
        );
        assert_eq!(
            hook.request,
            Path::new("/ctx/home/1000/hook/daily-translate/req")
        );
        assert_eq!(
            hook.output,
            Path::new("/ctx/home/1000/hook/daily-translate/out.json")
        );
        assert_eq!(
            hook.status,
            Path::new("/ctx/home/1000/hook/daily-translate/status")
        );
        assert_eq!(
            hook.last,
            Path::new("/ctx/home/1000/hook/daily-translate/last")
        );
        assert_eq!(
            hook.log,
            Path::new("/ctx/home/1000/hook/daily-translate/log.jsonl")
        );
    }

    #[test]
    fn path_ids_reject_slashes_response_and_error_names() {
        assert!(StepId::new("node/a").is_err());
        assert!(StepId::new("node.resp.json").is_err());
        assert!(StepId::new("node.error").is_err());
        assert!(ToolId::new("").is_err());
        assert!(ThreadId::new("..").is_err());
        assert!(JobId::new("translate_zh").is_err());
        assert!(JobId::new("job/name").is_err());
        assert!(HookId::new("daily_translate").is_err());
        assert!(HookId::new("hook/name").is_err());
        assert!(ChannelId::new("x".repeat(65)).is_err());
    }

    #[test]
    fn suffix_constants_match_cortexfs_abi() {
        assert_eq!(STAGED_SUFFIX, ".tmp");
        assert_eq!(REQUEST_SUFFIX, ".req.json");
        assert_eq!(RESPONSE_SUFFIX, ".resp.json");
        assert_eq!(ROUTE_SUFFIX, ".route.json");
        assert_eq!(FINGERPRINT_SUFFIX, ".fingerprint");
        assert_eq!(ERROR_SUFFIX, ".error");
        assert_eq!(JOB_SPEC_FILE, "spec");
        assert_eq!(JOB_REQUEST_FILE, "req");
        assert_eq!(JOB_OUTPUT_FILE, "out.json");
        assert_eq!(JOB_STATUS_FILE, "status");
        assert_eq!(HOOK_TRIGGER_FILE, "trigger");
        assert_eq!(HOOK_SPEC_FILE, "spec");
        assert_eq!(HOOK_REQUEST_FILE, "req");
        assert_eq!(HOOK_OUTPUT_FILE, "out.json");
        assert_eq!(HOOK_STATUS_FILE, "status");
        assert_eq!(HOOK_LAST_FILE, "last");
        assert_eq!(HOOK_LOG_FILE, "log.jsonl");
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
    fn submit_job_writes_spec_then_req_files() -> Result<(), Box<dyn std::error::Error>> {
        let root = unique_temp_root();
        let job = CortexHome::new(&root, 1000).job_exchange(JobId::new("translate.zh")?);

        let submitted = job.submit_job(
            b"kind=translate\nfrom=en\nto=zh\nout=json\nfields=text,from,to,input\n",
            b"hello world\n",
        )?;

        assert_eq!(submitted.spec, job.spec);
        assert_eq!(
            fs::read_to_string(&job.spec)?,
            "kind=translate\nfrom=en\nto=zh\nout=json\nfields=text,from,to,input\n"
        );
        assert_eq!(fs::read_to_string(&job.request)?, "hello world\n");

        fs::write(&job.output, "{\"text\":\"你好，世界\"}\n")?;
        fs::write(&job.status, "done\n")?;
        let outcome = job.read_job_outcome()?;
        assert_eq!(
            outcome.output,
            Some("{\"text\":\"你好，世界\"}\n".to_owned())
        );
        assert_eq!(outcome.status, Some("done".to_owned()));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn submit_hook_writes_req_and_reads_outcome_files() -> Result<(), Box<dyn std::error::Error>> {
        let root = unique_temp_root();
        let hook = CortexHome::new(&root, 1000).hook_exchange(HookId::new("daily-translate")?);

        let submitted = hook.submit_hook(b"hello world\n")?;

        assert_eq!(submitted.request, hook.request);
        assert_eq!(fs::read_to_string(&hook.request)?, "hello world\n");

        fs::write(&hook.output, "{\"text\":\"你好，世界\"}\n")?;
        fs::write(&hook.status, "done\n")?;
        fs::write(&hook.last, "2026-06-09T00:00:00Z\n")?;
        fs::write(&hook.log, "{\"event\":\"hook.done\"}\n")?;
        let outcome = hook.read_hook_outcome()?;
        assert_eq!(
            outcome.output,
            Some("{\"text\":\"你好，世界\"}\n".to_owned())
        );
        assert_eq!(outcome.status, Some("done".to_owned()));
        assert_eq!(outcome.last, Some("2026-06-09T00:00:00Z".to_owned()));
        assert_eq!(outcome.log, Some("{\"event\":\"hook.done\"}\n".to_owned()));

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
