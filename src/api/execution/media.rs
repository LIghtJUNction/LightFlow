use super::types::LeafExecution;
use crate::api::media_paths::{MediaKind, MediaPathProvider};
use crate::api::util::node_inputs;
use crate::api::{ApiError, ApiResult};
use crate::workflow::{ModelProvider, PortSpec, WorkflowArtifact, WorkflowNode, WorkflowSpec};
use std::collections::BTreeMap;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::path::{Path, PathBuf};

mod inputs;
pub(super) use inputs::{input_image_path, input_mask_path, input_string, input_u32, input_u64};

pub(super) fn collect_node_inputs(
    node: &WorkflowNode,
    workflow: &WorkflowSpec,
    workflows: &BTreeMap<String, WorkflowSpec>,
    workflow_inputs: &serde_json::Map<String, serde_json::Value>,
    node_outputs: &BTreeMap<String, serde_json::Map<String, serde_json::Value>>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut inputs = serde_json::Map::new();
    for input in node_inputs(node, workflows) {
        if let Some(value) = workflow_inputs.get(&input.name) {
            inputs.insert(input.name.clone(), value.clone());
        }
    }
    for edge in workflow.edges.iter().filter(|edge| edge.to.node == node.id) {
        if let Some(value) = node_outputs
            .get(&edge.from.node)
            .and_then(|outputs| outputs.get(&edge.from.port))
        {
            inputs.insert(edge.to.port.clone(), value.clone());
        }
    }
    inputs
}

pub(super) fn collect_workflow_outputs(
    workflow: &WorkflowSpec,
    workflow_inputs: &serde_json::Map<String, serde_json::Value>,
    node_outputs: &BTreeMap<String, serde_json::Map<String, serde_json::Value>>,
) -> Result<serde_json::Map<String, serde_json::Value>, ApiError> {
    let mut outputs = serde_json::Map::new();

    for output in &workflow.outputs {
        if let Some(value) = workflow_inputs.get(&output.name) {
            outputs.insert(output.name.clone(), value.clone());
            continue;
        }

        let mut producers = Vec::new();
        for node in &workflow.nodes {
            if let Some(value) = node_outputs
                .get(&node.id)
                .and_then(|ports| ports.get(&output.name))
            {
                producers.push((node.id.as_str(), value));
            }
        }
        match producers.as_slice() {
            [] => {
                outputs.insert(output.name.clone(), serde_json::Value::Null);
            }
            [(_, value)] => {
                outputs.insert(output.name.clone(), (*value).clone());
            }
            _ => {
                let node_ids = producers
                    .iter()
                    .map(|(node_id, _)| (*node_id).to_owned())
                    .collect::<Vec<_>>()
                    .join(", ");
                return Err(ApiError::InvalidRequest(format!(
                    "workflow {} output `{}` is produced by multiple nodes [{}]; rename ports or collapse producers so each public output has a unique source",
                    workflow.id, output.name, node_ids
                )));
            }
        }
    }

    Ok(outputs)
}

pub(super) fn execute_passthrough_ports(
    output_ports: &[PortSpec],
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut outputs = serde_json::Map::new();
    let first_input = inputs.values().next().cloned();

    for output in output_ports {
        let value = inputs
            .get(&output.name)
            .cloned()
            .or_else(|| {
                if inputs.len() == 1 {
                    first_input.clone()
                } else {
                    None
                }
            })
            .unwrap_or(serde_json::Value::Null);

        outputs.insert(output.name.clone(), value);
    }

    outputs
}

pub(super) fn output_path(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
    seed: u64,
) -> PathBuf {
    let paths = MediaPathProvider::new(root);
    paths.output_path_or_default(
        input_string(inputs, "output_path").as_deref(),
        MediaKind::Image,
        workflow,
        format!("{seed}.png"),
    )
}

pub(super) fn image_transform_output_path(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
    input_path: &Path,
    suffix: &str,
) -> PathBuf {
    let paths = MediaPathProvider::new(root);
    paths.output_path_or_default(
        input_string(inputs, "output_path").as_deref(),
        MediaKind::Image,
        workflow,
        image_filename(input_path, suffix),
    )
}

fn image_filename(input_path: &Path, suffix: &str) -> String {
    let stem = input_path
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .filter(|stem| !stem.is_empty())
        .unwrap_or("image");
    format!("{stem}-{suffix}.png")
}

pub(super) fn stable_seed(prompt: &str) -> u64 {
    let mut hasher = DefaultHasher::new();
    prompt.hash(&mut hasher);
    hasher.finish()
}

pub(super) fn selected_model(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> Option<serde_json::Value> {
    let requested = input_string(inputs, "model");
    let requirement = workflow
        .models
        .iter()
        .find(|model| model.capability == "text-to-image")?;
    let variant = requested
        .as_deref()
        .and_then(|id| requirement.variants.iter().find(|variant| variant.id == id))
        .or_else(|| {
            requirement
                .variants
                .iter()
                .find(|variant| variant.format == "gguf")
        })
        .or_else(|| requirement.variants.first())?;

    Some(serde_json::json!({
        "requirement_id": requirement.id,
        "variant_id": variant.id,
        "provider": match variant.provider {
            ModelProvider::HuggingFace => "hugging_face",
        },
        "format": variant.format,
        "repo": variant.repo,
        "file": variant.file,
    }))
}

pub(super) fn preview_image_outputs(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
    image_path: &Path,
    artifact: WorkflowArtifact,
    prompt: &str,
    seed: u64,
) -> ApiResult<LeafExecution> {
    let artifact_value = serde_json::to_value(&artifact)
        .map_err(|error| ApiError::InvalidRequest(error.to_string()))?;
    let mut outputs = serde_json::Map::new();

    for output in &workflow.outputs {
        let value = match output.name.as_str() {
            "image" | "artifact" => artifact_value.clone(),
            "image_path" | "output_path" => {
                serde_json::Value::String(image_path.display().to_string())
            }
            "source_image_path" => inputs
                .get("image_path")
                .cloned()
                .unwrap_or_else(|| serde_json::Value::String(image_path.display().to_string())),
            "prompt" => serde_json::Value::String(prompt.to_owned()),
            "seed" => seed.into(),
            other => inputs
                .get(other)
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        };
        outputs.insert(output.name.clone(), value);
    }

    Ok(LeafExecution {
        outputs,
        runtime: None,
        artifacts: vec![artifact],
    })
}
