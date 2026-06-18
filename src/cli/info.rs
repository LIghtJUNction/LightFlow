use super::runtime::RuntimeConfig;
use super::{CliResult, ensure_no_extra_args};
use crate::api::ApiService;
use serde::Serialize;
use serde_json::json;
use std::collections::{BTreeMap, BTreeSet};
use std::env;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize)]
struct BuildFeatures {
    flux: bool,
    flux_native: bool,
    gguf: bool,
    gguf_cuda: bool,
    gguf_metal: bool,
    rig: bool,
}

#[derive(Debug, Clone, Serialize)]
struct ExecutorInfo {
    id: &'static str,
    kind: &'static str,
    capabilities: Vec<&'static str>,
    available: bool,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    features: Vec<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    env: Option<&'static str>,
    #[serde(skip_serializing_if = "Option::is_none")]
    command: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct CategoryCount {
    category: String,
    workflows: usize,
}

#[derive(Debug, Clone, Serialize)]
struct RuntimeCapabilityCount {
    capability: String,
    workflows: usize,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    engines: Vec<String>,
}

pub(super) fn architecture_info(
    service: &ApiService,
    runtime: &RuntimeConfig,
    args: &[String],
) -> CliResult<serde_json::Value> {
    ensure_no_extra_args(args, 0, "info")?;
    let summaries = service.list_workflows()?.workflows;
    let mut category_counts = BTreeMap::<String, usize>::new();
    let mut runtime_capabilities = BTreeMap::<String, (usize, BTreeSet<String>)>::new();
    let mut leaf_workflows = 0usize;
    let mut composite_workflows = 0usize;
    let mut model_requirements = 0usize;

    for summary in &summaries {
        let category = summary
            .category
            .clone()
            .unwrap_or_else(|| "uncategorized".to_owned());
        *category_counts.entry(category).or_insert(0) += 1;

        let workflow = service.get_workflow(&summary.id)?;
        if workflow.nodes.is_empty() {
            leaf_workflows += 1;
        } else {
            composite_workflows += 1;
        }
        model_requirements += workflow.models.len();
        for runtime in workflow.runtimes {
            let entry = runtime_capabilities
                .entry(runtime.capability)
                .or_insert_with(|| (0, BTreeSet::new()));
            entry.0 += 1;
            if let Some(engine) = runtime.engine {
                entry.1.insert(engine);
            }
        }
    }

    let categories = category_counts
        .into_iter()
        .map(|(category, workflows)| CategoryCount {
            category,
            workflows,
        })
        .collect::<Vec<_>>();
    let runtime_capabilities = runtime_capabilities
        .into_iter()
        .map(
            |(capability, (workflows, engines))| RuntimeCapabilityCount {
                capability,
                workflows,
                engines: engines.into_iter().collect(),
            },
        )
        .collect::<Vec<_>>();

    Ok(json!({
        "package": {
            "name": env!("CARGO_PKG_NAME"),
            "version": env!("CARGO_PKG_VERSION"),
            "repository": env!("CARGO_PKG_REPOSITORY"),
        },
        "build": {
            "features": build_features(),
        },
        "project": {
            "root": service.repo_root(),
            "home": runtime.home_path,
            "lfw_path": runtime.lfw_path,
            "workflow_paths": runtime.workflow_paths.iter().map(PathBuf::from).collect::<Vec<_>>(),
        },
        "workflows": {
            "total": summaries.len(),
            "leaf": leaf_workflows,
            "composite": composite_workflows,
            "categories": categories,
            "runtime_capabilities": runtime_capabilities,
            "model_requirements": model_requirements,
        },
        "executors": executors(),
    }))
}

fn build_features() -> BuildFeatures {
    BuildFeatures {
        flux: cfg!(feature = "flux"),
        flux_native: cfg!(feature = "flux-native"),
        gguf: cfg!(feature = "gguf"),
        gguf_cuda: cfg!(feature = "gguf-cuda"),
        gguf_metal: cfg!(feature = "gguf-metal"),
        rig: cfg!(feature = "rig"),
    }
}

fn executors() -> Vec<ExecutorInfo> {
    vec![
        ExecutorInfo {
            id: "passthrough",
            kind: "builtin",
            capabilities: vec!["lightflow.data.copy"],
            available: true,
            features: Vec::new(),
            env: None,
            command: None,
        },
        ExecutorInfo {
            id: "builtin.preview.v1",
            kind: "builtin",
            capabilities: vec!["lightflow.image.generate"],
            available: true,
            features: Vec::new(),
            env: None,
            command: None,
        },
        ExecutorInfo {
            id: "builtin.image.invert.v1",
            kind: "builtin",
            capabilities: vec!["lightflow.image.invert"],
            available: true,
            features: Vec::new(),
            env: None,
            command: None,
        },
        ExecutorInfo {
            id: "flux2-klein.gguf.runner.v1",
            kind: "external",
            capabilities: vec![
                "lightflow.image.generate",
                "lightflow.image.edit",
                "lightflow.image.inpaint",
            ],
            available: env::var("LIGHTFLOW_FLUX_RUNNER").is_ok(),
            features: vec!["flux"],
            env: Some("LIGHTFLOW_FLUX_RUNNER"),
            command: env::var("LIGHTFLOW_FLUX_RUNNER").ok(),
        },
        ExecutorInfo {
            id: "diffusion-rs.native.v1",
            kind: "native",
            capabilities: vec![
                "lightflow.image.generate",
                "lightflow.image.edit",
                "lightflow.image.inpaint",
            ],
            available: cfg!(feature = "flux-native"),
            features: vec!["flux-native"],
            env: Some("LIGHTFLOW_FLUX_BACKEND=native"),
            command: None,
        },
        ExecutorInfo {
            id: "rig-core",
            kind: "native",
            capabilities: vec!["lightflow.llm.generate"],
            available: cfg!(feature = "rig"),
            features: vec!["rig"],
            env: None,
            command: None,
        },
    ]
}
