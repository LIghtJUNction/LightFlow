use super::model_manager::ModelManager;
use super::plan::{
    DataPolicy, ExecutionPlan, ExecutionRecipe, IMAGE_GENERATE_CAPABILITY, IMAGE_INVERT_CAPABILITY,
    INVERT_ENGINE, PREVIEW_ENGINE, build_leaf_execution_plan,
};
use super::util::{XdgUserDirectory, lightflow_xdg_user_dir, node_inputs};
use super::{ApiError, ApiResult};
use crate::workflow::{
    ModelProvider, NodeExecution, NodeExecutionStatus, PortSpec, WorkflowArtifact,
    WorkflowCondition, WorkflowExecution, WorkflowExecutionOptions, WorkflowNode, WorkflowNodeKind,
    WorkflowNodePatch, WorkflowSpec,
};
use std::collections::{BTreeMap, BTreeSet};
use std::fs;
use std::hash::{Hash, Hasher};
use std::io::{BufReader, BufWriter};
use std::path::{Path, PathBuf};
use std::time::Instant;

pub(super) fn execute_workflow_spec(
    root: &Path,
    workflow: &WorkflowSpec,
    workflows: &BTreeMap<String, WorkflowSpec>,
    options: WorkflowExecutionOptions,
) -> ApiResult<WorkflowExecution> {
    let mut model_manager = ModelManager::new(root);
    execute_workflow_spec_with_models(root, workflow, workflows, options, &mut model_manager)
}

fn execute_workflow_spec_with_models(
    root: &Path,
    workflow: &WorkflowSpec,
    workflows: &BTreeMap<String, WorkflowSpec>,
    options: WorkflowExecutionOptions,
    model_manager: &mut ModelManager,
) -> ApiResult<WorkflowExecution> {
    let validation = super::validation::validate_workflow_spec(workflow, workflows);
    if !validation.valid {
        return Err(ApiError::InvalidRequest(validation.issues.join("; ")));
    }

    if workflow.nodes.is_empty() {
        let leaf = execute_leaf_workflow(root, workflow, &options.inputs, model_manager)?;
        return Ok(WorkflowExecution {
            workflow_id: workflow.id.clone(),
            version: workflow.version.clone(),
            inputs: options.inputs,
            outputs: leaf.outputs,
            artifacts: leaf.artifacts,
            nodes: Vec::new(),
        });
    }

    let disabled_nodes = options.disabled_nodes.into_iter().collect::<BTreeSet<_>>();
    let enabled_nodes = options.enabled_nodes.into_iter().collect::<BTreeSet<_>>();
    let patch = options.patch.unwrap_or_default();
    let nodes_by_id = workflow
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let mut node_outputs = BTreeMap::<String, serde_json::Map<String, serde_json::Value>>::new();
    let mut artifacts = Vec::new();
    let mut executions = Vec::new();

    for node_id in validation.topological_order {
        let Some(node) = nodes_by_id.get(node_id.as_str()) else {
            continue;
        };
        let node_started_at = Instant::now();
        let node_inputs =
            collect_node_inputs(node, workflow, workflows, &options.inputs, &node_outputs);
        let node_patch = patch.nodes.get(&node.id);
        let is_enabled =
            enabled_nodes.contains(&node.id) || node_patch.is_some_and(|patch| patch.enable);
        let is_disabled = (node.disabled
            || disabled_nodes.contains(&node.id)
            || node_patch.is_some_and(|patch| patch.disable))
            && !is_enabled;
        if is_disabled
            && node_patch
                .and_then(|patch| patch.fallback_workflow_id.as_ref())
                .is_none()
        {
            executions.push(NodeExecution {
                node_id: node.id.clone(),
                workflow_id: node.workflow_id.clone(),
                selected_workflow_id: None,
                status: NodeExecutionStatus::Skipped,
                duration_ms: elapsed_ms(node_started_at),
                attempts: 0,
                inputs: node_inputs,
                outputs: serde_json::Map::new(),
                artifacts: Vec::new(),
            });
            continue;
        }

        let selected_workflow_id =
            patched_node_workflow_id(node, &node_inputs, node_patch, is_disabled)?;
        let child = workflows
            .get(selected_workflow_id.as_str())
            .ok_or_else(|| {
                ApiError::InvalidRequest(format!(
                    "node {} references missing workflow {}",
                    node.id, selected_workflow_id
                ))
            })?;
        let child_run = execute_child_workflow_with_patch(
            root,
            child,
            workflows,
            &node_inputs,
            model_manager,
            node_patch,
        )?;
        node_outputs.insert(node.id.clone(), child_run.leaf.outputs.clone());
        artifacts.extend(child_run.leaf.artifacts.clone());
        executions.push(NodeExecution {
            node_id: node.id.clone(),
            workflow_id: node.workflow_id.clone(),
            selected_workflow_id: if node.kind == WorkflowNodeKind::If
                || selected_workflow_id != node.workflow_id
            {
                Some(selected_workflow_id)
            } else {
                None
            },
            status: NodeExecutionStatus::Completed,
            duration_ms: elapsed_ms(node_started_at),
            attempts: child_run.attempts,
            inputs: node_inputs,
            outputs: child_run.leaf.outputs,
            artifacts: child_run.leaf.artifacts,
        });
    }

    let outputs = collect_workflow_outputs(workflow, &options.inputs, &node_outputs);
    Ok(WorkflowExecution {
        workflow_id: workflow.id.clone(),
        version: workflow.version.clone(),
        inputs: options.inputs,
        outputs,
        artifacts,
        nodes: executions,
    })
}

fn patched_node_workflow_id(
    node: &WorkflowNode,
    inputs: &serde_json::Map<String, serde_json::Value>,
    patch: Option<&WorkflowNodePatch>,
    is_disabled: bool,
) -> ApiResult<String> {
    if let Some(fallback) = patch.and_then(|patch| patch.fallback_workflow_id.as_ref())
        && is_disabled
    {
        return Ok(fallback.clone());
    }
    if let Some(replacement) = patch.and_then(|patch| patch.replace_with.as_ref()) {
        return Ok(replacement.clone());
    }
    selected_node_workflow_id(node, inputs)
}

fn selected_node_workflow_id(
    node: &WorkflowNode,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<String> {
    match node.kind {
        WorkflowNodeKind::Workflow => Ok(node.workflow_id.clone()),
        WorkflowNodeKind::If => {
            let condition = node.condition.as_ref().ok_or_else(|| {
                ApiError::InvalidRequest(format!("if node {} has no condition", node.id))
            })?;
            let condition_matches = evaluate_condition(condition, inputs);
            if condition_matches {
                node.then_workflow_id.clone()
            } else {
                node.else_workflow_id.clone()
            }
            .ok_or_else(|| {
                ApiError::InvalidRequest(format!("if node {} has an incomplete branch", node.id))
            })
        }
    }
}

fn evaluate_condition(
    condition: &WorkflowCondition,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> bool {
    match condition {
        WorkflowCondition::InputEquals { input, value } => inputs.get(input) == Some(value),
    }
}

fn execute_child_workflow(
    root: &Path,
    workflow: &WorkflowSpec,
    workflows: &BTreeMap<String, WorkflowSpec>,
    inputs: &serde_json::Map<String, serde_json::Value>,
    model_manager: &mut ModelManager,
) -> ApiResult<LeafExecution> {
    if workflow.nodes.is_empty() {
        return execute_leaf_workflow(root, workflow, inputs, model_manager);
    }
    let execution = execute_workflow_spec_with_models(
        root,
        workflow,
        workflows,
        WorkflowExecutionOptions {
            inputs: inputs.clone(),
            disabled_nodes: Vec::new(),
            enabled_nodes: Vec::new(),
            patch: None,
        },
        model_manager,
    )?;
    Ok(LeafExecution {
        outputs: execution.outputs,
        artifacts: execution.artifacts,
    })
}

fn execute_child_workflow_with_patch(
    root: &Path,
    workflow: &WorkflowSpec,
    workflows: &BTreeMap<String, WorkflowSpec>,
    inputs: &serde_json::Map<String, serde_json::Value>,
    model_manager: &mut ModelManager,
    patch: Option<&WorkflowNodePatch>,
) -> ApiResult<ChildWorkflowRun> {
    let attempts = patch.and_then(|patch| patch.retry).unwrap_or(1).max(1);
    let timeout_ms = patch.and_then(|patch| patch.timeout_ms).map(u128::from);
    let mut last_error = None;
    for attempt in 1..=attempts {
        let started_at = Instant::now();
        let result = execute_child_workflow(root, workflow, workflows, inputs, model_manager);
        match result {
            Ok(output) => {
                if let Some(timeout_ms) = timeout_ms
                    && started_at.elapsed().as_millis() > timeout_ms
                {
                    last_error = Some(ApiError::InvalidRequest(format!(
                        "node execution timed out after {timeout_ms}ms"
                    )));
                    continue;
                }
                return Ok(ChildWorkflowRun {
                    leaf: output,
                    attempts: attempt,
                });
            }
            Err(error) => last_error = Some(error),
        }
    }
    Err(last_error.expect("node attempts are always at least one"))
}

#[derive(Debug, Clone, PartialEq)]
struct ChildWorkflowRun {
    leaf: LeafExecution,
    attempts: usize,
}

#[derive(Debug, Clone, PartialEq)]
struct LeafExecution {
    outputs: serde_json::Map<String, serde_json::Value>,
    artifacts: Vec<WorkflowArtifact>,
}

fn elapsed_ms(started_at: Instant) -> u64 {
    started_at
        .elapsed()
        .as_millis()
        .try_into()
        .unwrap_or(u64::MAX)
}

fn execute_leaf_workflow(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
    model_manager: &mut ModelManager,
) -> ApiResult<LeafExecution> {
    let plan = build_leaf_execution_plan(workflow)?;
    execute_leaf_plan(root, workflow, inputs, model_manager, &plan)
}

fn execute_leaf_plan(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
    model_manager: &mut ModelManager,
    plan: &ExecutionPlan,
) -> ApiResult<LeafExecution> {
    let _data_policy = match plan.node.data_policy {
        DataPolicy::JsonValues => "json",
        DataPolicy::ArtifactHandles => "artifact_handles",
        DataPolicy::DeviceResidentPreferred => "device_resident_preferred",
    };

    match plan.node.recipe {
        ExecutionRecipe::PreviewTextToImage => {
            execute_preview_text_to_image(root, workflow, inputs)
        }
        ExecutionRecipe::FluxTextToImage => {
            let result =
                super::flux::execute_flux_text_to_image(root, workflow, inputs, model_manager)?;
            Ok(LeafExecution {
                outputs: result.outputs,
                artifacts: result.artifacts,
            })
        }
        ExecutionRecipe::FluxImageEdit => {
            let result =
                super::flux::execute_flux_image_edit(root, workflow, inputs, model_manager)?;
            Ok(LeafExecution {
                outputs: result.outputs,
                artifacts: result.artifacts,
            })
        }
        ExecutionRecipe::FluxInpaint => {
            let result = super::flux::execute_flux_inpaint(root, workflow, inputs, model_manager)?;
            Ok(LeafExecution {
                outputs: result.outputs,
                artifacts: result.artifacts,
            })
        }
        ExecutionRecipe::ImageInvert => execute_image_invert(root, workflow, inputs),
        ExecutionRecipe::RigLlmGenerate => {
            let outputs = super::llm_rig::execute_rig_llm(workflow, inputs)?;
            Ok(LeafExecution {
                outputs,
                artifacts: Vec::new(),
            })
        }
        ExecutionRecipe::Passthrough => Ok(LeafExecution {
            outputs: execute_passthrough_ports(&workflow.outputs, inputs),
            artifacts: Vec::new(),
        }),
    }
}

fn collect_node_inputs(
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

fn execute_passthrough_ports(
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

fn execute_preview_text_to_image(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let prompt = input_string(inputs, "prompt")
        .or_else(|| input_string(inputs, "text"))
        .or_else(|| input_string(inputs, "positive"))
        .unwrap_or_default();
    let width = input_u32(inputs, "width").unwrap_or(512).clamp(64, 2048);
    let height = input_u32(inputs, "height").unwrap_or(512).clamp(64, 2048);
    let seed = input_u64(inputs, "seed").unwrap_or_else(|| stable_seed(&prompt));
    let path = output_path(root, workflow, inputs, seed);

    write_preview_png(&path, width, height, seed, &prompt)?;
    let artifact = image_artifact(workflow, &path, &prompt, width, height, seed, inputs);
    let artifact_value = serde_json::to_value(&artifact)
        .map_err(|error| ApiError::InvalidRequest(error.to_string()))?;
    let mut outputs = serde_json::Map::new();
    for output in &workflow.outputs {
        let value = match output.name.as_str() {
            "image" | "artifact" => artifact_value.clone(),
            "image_path" | "output_path" => serde_json::Value::String(artifact.path.clone()),
            "prompt" => serde_json::Value::String(prompt.clone()),
            "width" => width.into(),
            "height" => height.into(),
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
        artifacts: vec![artifact],
    })
}

fn execute_image_invert(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let input_path = input_string(inputs, "image_path")
        .ok_or_else(|| ApiError::InvalidRequest("image_path is required".to_owned()))?;
    let input_path = PathBuf::from(input_path);
    let output_path = image_transform_output_path(root, workflow, inputs, &input_path, "invert");
    write_inverted_png(&input_path, &output_path)?;

    let artifact = image_transform_artifact(&input_path, &output_path);
    let artifact_value = serde_json::to_value(&artifact)
        .map_err(|error| ApiError::InvalidRequest(error.to_string()))?;
    let mut outputs = serde_json::Map::new();
    for output in &workflow.outputs {
        let value = match output.name.as_str() {
            "image" | "artifact" => artifact_value.clone(),
            "image_path" | "output_path" => {
                serde_json::Value::String(output_path.display().to_string())
            }
            "source_image_path" => serde_json::Value::String(input_path.display().to_string()),
            other => inputs
                .get(other)
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        };
        outputs.insert(output.name.clone(), value);
    }

    Ok(LeafExecution {
        outputs,
        artifacts: vec![artifact],
    })
}

fn input_string(inputs: &serde_json::Map<String, serde_json::Value>, name: &str) -> Option<String> {
    inputs.get(name).and_then(|value| match value {
        serde_json::Value::String(value) => Some(value.clone()),
        value if !value.is_null() => Some(value.to_string()),
        _ => None,
    })
}

fn input_u32(inputs: &serde_json::Map<String, serde_json::Value>, name: &str) -> Option<u32> {
    inputs
        .get(name)
        .and_then(serde_json::Value::as_u64)
        .and_then(|value| u32::try_from(value).ok())
}

fn input_u64(inputs: &serde_json::Map<String, serde_json::Value>, name: &str) -> Option<u64> {
    inputs.get(name).and_then(serde_json::Value::as_u64)
}

fn stable_seed(prompt: &str) -> u64 {
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    prompt.hash(&mut hasher);
    hasher.finish()
}

fn output_path(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
    seed: u64,
) -> PathBuf {
    if let Some(path) = input_string(inputs, "output_path") {
        return PathBuf::from(path);
    }
    default_lightflow_picture_dir(root)
        .join(workflow.id.replace('.', "_"))
        .join(format!("{seed}.png"))
}

fn image_artifact(
    workflow: &WorkflowSpec,
    path: &Path,
    prompt: &str,
    width: u32,
    height: u32,
    seed: u64,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> WorkflowArtifact {
    let mut metadata = serde_json::Map::new();
    metadata.insert("prompt".to_owned(), prompt.to_owned().into());
    metadata.insert("width".to_owned(), width.into());
    metadata.insert("height".to_owned(), height.into());
    metadata.insert("seed".to_owned(), seed.into());
    metadata.insert("engine".to_owned(), PREVIEW_ENGINE.into());
    metadata.insert("capability".to_owned(), IMAGE_GENERATE_CAPABILITY.into());
    if let Some(negative) = input_string(inputs, "negative") {
        metadata.insert("negative_prompt".to_owned(), negative.into());
    }
    if let Some(model) = selected_model(workflow, inputs) {
        metadata.insert("model".to_owned(), model);
    }

    WorkflowArtifact {
        id: "image".to_owned(),
        kind: "image".to_owned(),
        path: path.display().to_string(),
        mime_type: "image/png".to_owned(),
        metadata,
    }
}

fn image_transform_artifact(input_path: &Path, output_path: &Path) -> WorkflowArtifact {
    let mut metadata = serde_json::Map::new();
    metadata.insert("engine".to_owned(), INVERT_ENGINE.into());
    metadata.insert("capability".to_owned(), IMAGE_INVERT_CAPABILITY.into());
    metadata.insert(
        "source_image_path".to_owned(),
        input_path.display().to_string().into(),
    );

    WorkflowArtifact {
        id: "image".to_owned(),
        kind: "image".to_owned(),
        path: output_path.display().to_string(),
        mime_type: "image/png".to_owned(),
        metadata,
    }
}

fn selected_model(
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

fn write_preview_png(
    path: &Path,
    width: u32,
    height: u32,
    seed: u64,
    prompt: &str,
) -> ApiResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = fs::File::create(path)?;
    let writer = BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, width, height);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    let mut png = encoder
        .write_header()
        .map_err(|error| ApiError::Io(std::io::Error::other(error)))?;
    let data = preview_pixels(width, height, seed, prompt);
    png.write_image_data(&data)
        .map_err(|error| ApiError::Io(std::io::Error::other(error)))
}

fn write_inverted_png(input_path: &Path, output_path: &Path) -> ApiResult<()> {
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let file = fs::File::open(input_path)?;
    let decoder = png::Decoder::new(BufReader::new(file));
    let mut reader = decoder
        .read_info()
        .map_err(|error| ApiError::Io(std::io::Error::other(error)))?;
    let buffer_size = reader.output_buffer_size().ok_or_else(|| {
        ApiError::InvalidRequest(format!(
            "image is too large to decode: {}",
            input_path.display()
        ))
    })?;
    let mut data = vec![0; buffer_size];
    let info = reader
        .next_frame(&mut data)
        .map_err(|error| ApiError::Io(std::io::Error::other(error)))?;
    data.truncate(info.buffer_size());

    if info.bit_depth != png::BitDepth::Eight {
        return Err(ApiError::InvalidRequest(format!(
            "only 8-bit PNG images can be inverted: {}",
            input_path.display()
        )));
    }

    let color_channels = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 3,
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 1,
        png::ColorType::Indexed => {
            return Err(ApiError::InvalidRequest(format!(
                "indexed PNG images are not supported for invert: {}",
                input_path.display()
            )));
        }
    };
    let channels = match info.color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 4,
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 2,
        png::ColorType::Indexed => unreachable!("indexed PNG is rejected above"),
    };
    for pixel in data.chunks_exact_mut(channels) {
        for channel in pixel.iter_mut().take(color_channels) {
            *channel = 255 - *channel;
        }
    }

    let file = fs::File::create(output_path)?;
    let writer = BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, info.width, info.height);
    encoder.set_color(info.color_type);
    encoder.set_depth(info.bit_depth);
    let mut png = encoder
        .write_header()
        .map_err(|error| ApiError::Io(std::io::Error::other(error)))?;
    png.write_image_data(&data)
        .map_err(|error| ApiError::Io(std::io::Error::other(error)))
}

fn image_transform_output_path(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
    input_path: &Path,
    suffix: &str,
) -> PathBuf {
    if let Some(path) = input_string(inputs, "output_path") {
        return PathBuf::from(path);
    }

    let stem = input_path
        .file_stem()
        .and_then(std::ffi::OsStr::to_str)
        .filter(|stem| !stem.is_empty())
        .unwrap_or("image");
    default_lightflow_picture_dir(root)
        .join(workflow.id.replace('.', "_"))
        .join(format!("{stem}-{suffix}.png"))
}

fn default_lightflow_picture_dir(root: &Path) -> PathBuf {
    lightflow_xdg_user_dir(root, XdgUserDirectory::Pictures)
}

fn preview_pixels(width: u32, height: u32, seed: u64, prompt: &str) -> Vec<u8> {
    let mut data = Vec::with_capacity((width as usize) * (height as usize) * 3);
    let prompt_mix = stable_seed(prompt);
    for y in 0..height {
        for x in 0..width {
            let base = seed ^ prompt_mix ^ ((x as u64) << 32) ^ y as u64;
            data.push(((x * 255 / width) as u8) ^ (base as u8));
            data.push(((y * 255 / height) as u8) ^ ((base >> 8) as u8));
            data.push((((x + y) * 127 / (width + height)) as u8) ^ ((base >> 16) as u8));
        }
    }
    data
}

fn collect_workflow_outputs(
    workflow: &WorkflowSpec,
    workflow_inputs: &serde_json::Map<String, serde_json::Value>,
    node_outputs: &BTreeMap<String, serde_json::Map<String, serde_json::Value>>,
) -> serde_json::Map<String, serde_json::Value> {
    let mut outputs = serde_json::Map::new();
    for output in &workflow.outputs {
        let value = workflow_inputs
            .get(&output.name)
            .cloned()
            .or_else(|| {
                workflow
                    .nodes
                    .iter()
                    .rev()
                    .filter_map(|node| node_outputs.get(&node.id))
                    .find_map(|outputs| outputs.get(&output.name).cloned())
            })
            .unwrap_or(serde_json::Value::Null);
        outputs.insert(output.name.clone(), value);
    }
    outputs
}
