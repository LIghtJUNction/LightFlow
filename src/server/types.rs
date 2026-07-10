use crate::api::ApiService;
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::Semaphore;

pub(crate) const OPENAPI_YAML: &str = include_str!("../../openapi/lightflow.yaml");

#[derive(Clone)]
pub(crate) struct AppState {
    pub(crate) service: Arc<ApiService>,
    pub(crate) blocking_runs: Arc<Semaphore>,
}

impl AppState {
    pub(crate) fn new(service: ApiService) -> Self {
        Self {
            service: Arc::new(service),
            blocking_runs: super::blocking::configured_semaphore(),
        }
    }
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct LoopCheckQuery {
    #[serde(default)]
    pub(crate) require_replay: bool,
    #[serde(default)]
    pub(crate) require_selected_replay: bool,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct LoopProjectsQuery {
    #[serde(default)]
    pub(crate) dirty: bool,
    #[serde(default)]
    pub(crate) changed: bool,
    pub(crate) project: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct PublishQuery {
    pub(crate) project: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct PatchValidationQuery {
    pub(crate) workflow_id: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct ReleaseCheckQuery {
    pub(crate) workflow_id: Option<String>,
    pub(crate) project: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct RunListQuery {
    pub(crate) limit: Option<usize>,
    pub(crate) workflow_id: Option<String>,
    pub(crate) status: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct ArtifactListQuery {
    pub(crate) limit: Option<usize>,
    pub(crate) run_id: Option<String>,
    pub(crate) workflow_id: Option<String>,
    pub(crate) kind: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
pub(crate) struct ModelListQuery {
    pub(crate) workflow_id: Option<String>,
    pub(crate) status: Option<String>,
}

#[cfg(test)]
pub(crate) const HTTP_PATHS: &[&str] = &[
    "/health",
    "/openapi.yaml",
    "/loop",
    "/loop/changes",
    "/loop/projects",
    "/release",
    "/publish",
    "/workflows",
    "/nodes",
    "/nodes/{workflow_id}",
    "/executors",
    "/models",
    "/runs",
    "/runs/{run_id}",
    "/runs/{run_id}/replay",
    "/runs/{run_id}/events",
    "/artifacts",
    "/patches",
    "/patches/{name}",
    "/patches/validate",
    "/workflows/{workflow_id}",
    "/workflows/{workflow_id}/dependencies",
    "/workflows/{workflow_id}/loop",
    "/workflows/{workflow_id}/plan",
    "/workflows/{workflow_id}/publish",
    "/workflows/{workflow_id}/run",
    "/workflows/validate",
    "/mcp",
];
