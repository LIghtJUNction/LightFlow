use super::model_manager::ModelManager;
use super::plan::{
    CONTROL_IF_CAPABILITY, CONTROL_MERGE_CAPABILITY, CONTROL_SPLIT_CAPABILITY,
    CONTROL_SWITCH_CAPABILITY, DataPolicy, ExecutionPlan, ExecutionRecipe, IMAGE_CROP_CAPABILITY,
    IMAGE_EDIT_CAPABILITY, IMAGE_GENERATE_CAPABILITY, IMAGE_INPAINT_CAPABILITY,
    IMAGE_INVERT_CAPABILITY, IMAGE_LOAD_CAPABILITY, IMAGE_RESIZE_CAPABILITY, IMAGE_SAVE_CAPABILITY,
    IMAGE_UPSCALE_CAPABILITY, INVERT_ENGINE, LLM_CLASSIFY_CAPABILITY, LLM_GENERATE_CAPABILITY,
    LLM_STRUCTURED_OUTPUT_CAPABILITY, MASK_COMPOSE_CAPABILITY, MODEL_LOCK_CHECK_CAPABILITY,
    MODEL_SELECT_CAPABILITY, PREVIEW_EDIT_ENGINE, PREVIEW_ENGINE, PREVIEW_INPAINT_ENGINE,
    TEXT_REGEX_CAPABILITY, build_leaf_execution_plan,
};
use super::util::{XdgUserDirectory, lightflow_xdg_user_dir, node_inputs};
use super::{ApiError, ApiResult};
use crate::workflow::{
    ModelProvider, NodeExecution, NodeExecutionStatus, PortSpec, WorkflowArtifact,
    WorkflowCondition, WorkflowExecution, WorkflowExecutionOptions, WorkflowNode, WorkflowNodeKind,
    WorkflowNodePatch, WorkflowSpec,
};
use regex::Regex;
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
        ExecutionRecipe::ImageLoad => execute_image_load(workflow, inputs),
        ExecutionRecipe::ImageSave => execute_image_save(root, workflow, inputs),
        ExecutionRecipe::ImageResize => execute_image_resize(root, workflow, inputs),
        ExecutionRecipe::ImageCrop => execute_image_crop(root, workflow, inputs),
        ExecutionRecipe::PreviewImageEdit => execute_preview_image_edit(root, workflow, inputs),
        ExecutionRecipe::PreviewInpaint => execute_preview_inpaint(root, workflow, inputs),
        ExecutionRecipe::ImageUpscale => execute_image_upscale(root, workflow, inputs),
        ExecutionRecipe::MaskCompose => execute_mask_compose(root, workflow, inputs),
        ExecutionRecipe::RigLlmGenerate => {
            let outputs = super::llm_rig::execute_rig_llm(workflow, inputs)?;
            Ok(LeafExecution {
                outputs,
                artifacts: Vec::new(),
            })
        }
        ExecutionRecipe::BuiltinLlmGenerate => execute_builtin_llm_generate(workflow, inputs),
        ExecutionRecipe::TextConcat => execute_text_concat(workflow, inputs),
        ExecutionRecipe::TextTemplate => execute_text_template(workflow, inputs),
        ExecutionRecipe::TextRegex => execute_text_regex(workflow, inputs),
        ExecutionRecipe::JsonExtract => execute_json_extract(workflow, inputs),
        ExecutionRecipe::ControlIf => execute_control_if(workflow, inputs),
        ExecutionRecipe::ControlSwitch => execute_control_switch(workflow, inputs),
        ExecutionRecipe::ControlMerge => execute_control_merge(workflow, inputs),
        ExecutionRecipe::ControlSplit => execute_control_split(workflow, inputs),
        ExecutionRecipe::ModelSelect => execute_model_select(workflow, inputs),
        ExecutionRecipe::ModelLockCheck => execute_model_lock_check(root, workflow, inputs),
        ExecutionRecipe::LlmClassify => execute_llm_classify(workflow, inputs),
        ExecutionRecipe::LlmStructuredOutput => execute_llm_structured_output(workflow, inputs),
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
    let input_path = input_image_path(inputs)?;
    let output_path = image_transform_output_path(root, workflow, inputs, &input_path, "invert");
    write_inverted_png(&input_path, &output_path)?;

    let artifact = image_transform_artifact(&input_path, &output_path);
    image_path_outputs(workflow, inputs, &output_path, artifact)
}

fn execute_image_load(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let image_path = input_image_path(inputs)?;
    let image = read_png_image(&image_path)?;
    let artifact = image_file_artifact(
        &image_path,
        &image_path,
        "builtin.image.load.v1",
        IMAGE_LOAD_CAPABILITY,
        Some((image.width, image.height)),
    );
    image_path_outputs(workflow, inputs, &image_path, artifact)
}

fn execute_image_save(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let image_path = input_image_path(inputs)?;
    let image = read_png_image(&image_path)?;
    let output_path = image_transform_output_path(root, workflow, inputs, &image_path, "saved");
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::copy(&image_path, &output_path)?;
    let artifact = image_file_artifact(
        &image_path,
        &output_path,
        "builtin.image.save.v1",
        IMAGE_SAVE_CAPABILITY,
        Some((image.width, image.height)),
    );
    image_path_outputs(workflow, inputs, &output_path, artifact)
}

fn execute_image_resize(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let image_path = input_image_path(inputs)?;
    let image = read_png_image(&image_path)?;
    let width = input_u32(inputs, "width").unwrap_or(image.width).max(1);
    let height = input_u32(inputs, "height").unwrap_or(image.height).max(1);
    let resized = resize_png_image(&image, width, height);
    let output_path = image_transform_output_path(root, workflow, inputs, &image_path, "resized");
    write_png_image(&output_path, &resized)?;
    let artifact = image_file_artifact(
        &image_path,
        &output_path,
        "builtin.image.resize.v1",
        IMAGE_RESIZE_CAPABILITY,
        Some((width, height)),
    );
    image_path_outputs(workflow, inputs, &output_path, artifact)
}

fn execute_image_crop(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let image_path = input_image_path(inputs)?;
    let image = read_png_image(&image_path)?;
    let x = input_u32(inputs, "x").unwrap_or(0);
    let y = input_u32(inputs, "y").unwrap_or(0);
    let width = input_u32(inputs, "width").unwrap_or(image.width.saturating_sub(x));
    let height = input_u32(inputs, "height").unwrap_or(image.height.saturating_sub(y));
    let cropped = crop_png_image(&image, x, y, width, height)?;
    let output_path = image_transform_output_path(root, workflow, inputs, &image_path, "cropped");
    write_png_image(&output_path, &cropped)?;
    let artifact = image_file_artifact(
        &image_path,
        &output_path,
        "builtin.image.crop.v1",
        IMAGE_CROP_CAPABILITY,
        Some((cropped.width, cropped.height)),
    );
    image_path_outputs(workflow, inputs, &output_path, artifact)
}

fn execute_preview_image_edit(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let image_path = input_image_path(inputs)?;
    let prompt = input_string(inputs, "prompt")
        .or_else(|| input_string(inputs, "text"))
        .unwrap_or_default();
    let seed = input_u64(inputs, "seed").unwrap_or_else(|| stable_seed(&prompt));
    let image = read_png_image(&image_path)?;
    let edited = preview_edit_image(&image, seed, &prompt, None);
    let output_path = image_transform_output_path(root, workflow, inputs, &image_path, "edited");
    write_png_image(&output_path, &edited)?;
    let artifact = preview_transform_artifact(PreviewTransformArtifact {
        workflow,
        input_path: &image_path,
        mask_path: None,
        output_path: &output_path,
        prompt: &prompt,
        seed,
        engine: PREVIEW_EDIT_ENGINE,
        capability: IMAGE_EDIT_CAPABILITY,
        dimensions: Some((edited.width, edited.height)),
        inputs,
    });
    preview_image_outputs(workflow, inputs, &output_path, artifact, &prompt, seed)
}

fn execute_preview_inpaint(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let image_path = input_image_path(inputs)?;
    let mask_path = input_mask_path(inputs)?;
    let prompt = input_string(inputs, "prompt")
        .or_else(|| input_string(inputs, "text"))
        .unwrap_or_default();
    let seed = input_u64(inputs, "seed").unwrap_or_else(|| stable_seed(&prompt));
    let image = read_png_image(&image_path)?;
    let mask = read_png_image(&mask_path)?;
    let mask = if mask.width == image.width && mask.height == image.height {
        mask
    } else {
        resize_png_image(&mask, image.width, image.height)
    };
    let inpainted = preview_edit_image(&image, seed, &prompt, Some(&mask));
    let output_path = image_transform_output_path(root, workflow, inputs, &image_path, "inpainted");
    write_png_image(&output_path, &inpainted)?;
    let artifact = preview_transform_artifact(PreviewTransformArtifact {
        workflow,
        input_path: &image_path,
        mask_path: Some(&mask_path),
        output_path: &output_path,
        prompt: &prompt,
        seed,
        engine: PREVIEW_INPAINT_ENGINE,
        capability: IMAGE_INPAINT_CAPABILITY,
        dimensions: Some((inpainted.width, inpainted.height)),
        inputs,
    });
    preview_image_outputs(workflow, inputs, &output_path, artifact, &prompt, seed)
}

fn execute_image_upscale(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let image_path = input_image_path(inputs)?;
    let image = read_png_image(&image_path)?;
    let scale = input_u32(inputs, "scale").unwrap_or(2).clamp(1, 16);
    let width = image.width.saturating_mul(scale).max(1);
    let height = image.height.saturating_mul(scale).max(1);
    let upscaled = resize_png_image(&image, width, height);
    let output_path = image_transform_output_path(root, workflow, inputs, &image_path, "upscaled");
    write_png_image(&output_path, &upscaled)?;
    let artifact = image_file_artifact(
        &image_path,
        &output_path,
        "builtin.image.upscale.v1",
        IMAGE_UPSCALE_CAPABILITY,
        Some((width, height)),
    );
    image_path_outputs(workflow, inputs, &output_path, artifact)
}

fn execute_mask_compose(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let mask_a_path = input_string(inputs, "mask_a_path")
        .or_else(|| input_string(inputs, "a_path"))
        .or_else(|| input_string(inputs, "mask_path"))
        .map(PathBuf::from)
        .ok_or_else(|| ApiError::InvalidRequest("mask_a_path is required".to_owned()))?;
    let mask_b_path = input_string(inputs, "mask_b_path")
        .or_else(|| input_string(inputs, "b_path"))
        .map(PathBuf::from)
        .ok_or_else(|| ApiError::InvalidRequest("mask_b_path is required".to_owned()))?;
    let mode = input_string(inputs, "mode").unwrap_or_else(|| "max".to_owned());
    let mask_a = read_png_image(&mask_a_path)?;
    let mask_b = read_png_image(&mask_b_path)?;
    let mask_b = if mask_b.width == mask_a.width && mask_b.height == mask_a.height {
        mask_b
    } else {
        resize_png_image(&mask_b, mask_a.width, mask_a.height)
    };
    let composed = compose_masks(&mask_a, &mask_b, &mode)?;
    let output_path = mask_output_path(root, workflow, inputs, &mask_a_path, "composed");
    write_png_image(&output_path, &composed)?;
    let artifact = mask_artifact(
        &mask_a_path,
        &mask_b_path,
        &output_path,
        &mode,
        Some((composed.width, composed.height)),
    );
    mask_path_outputs(workflow, inputs, &output_path, artifact, &mode)
}

fn image_path_outputs(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
    image_path: &Path,
    artifact: WorkflowArtifact,
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

fn input_mask_path(inputs: &serde_json::Map<String, serde_json::Value>) -> ApiResult<PathBuf> {
    input_string(inputs, "mask_path")
        .or_else(|| input_string(inputs, "mask"))
        .map(PathBuf::from)
        .ok_or_else(|| ApiError::InvalidRequest("mask_path is required".to_owned()))
}

fn input_image_path(inputs: &serde_json::Map<String, serde_json::Value>) -> ApiResult<PathBuf> {
    input_string(inputs, "image_path")
        .or_else(|| input_string(inputs, "path"))
        .map(PathBuf::from)
        .ok_or_else(|| ApiError::InvalidRequest("image_path is required".to_owned()))
}

fn execute_text_concat(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let separator = input_string(inputs, "separator").unwrap_or_default();
    let items = inputs
        .get("items")
        .and_then(serde_json::Value::as_array)
        .map(|items| items.iter().map(json_value_text).collect::<Vec<_>>())
        .unwrap_or_else(|| {
            ["a", "b"]
                .into_iter()
                .filter_map(|name| inputs.get(name))
                .map(json_value_text)
                .collect()
        });
    let text = items.join(&separator);
    Ok(LeafExecution {
        outputs: text_outputs(workflow, inputs, &text),
        artifacts: Vec::new(),
    })
}

fn execute_text_template(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let template = input_string(inputs, "template").unwrap_or_default();
    let vars = inputs.get("vars").unwrap_or(&serde_json::Value::Null);
    let text = render_template(&template, vars);
    Ok(LeafExecution {
        outputs: text_outputs(workflow, inputs, &text),
        artifacts: Vec::new(),
    })
}

fn execute_text_regex(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let text = input_string(inputs, "text").unwrap_or_default();
    let pattern = input_string(inputs, "pattern").unwrap_or_default();
    let replacement = input_string(inputs, "replacement");
    let regex = Regex::new(&pattern)
        .map_err(|error| ApiError::InvalidRequest(format!("invalid regex pattern: {error}")))?;
    let captures = regex
        .captures_iter(&text)
        .map(|captures| {
            serde_json::Value::Array(
                captures
                    .iter()
                    .map(|capture| {
                        capture
                            .map(|capture| serde_json::Value::String(capture.as_str().to_owned()))
                            .unwrap_or(serde_json::Value::Null)
                    })
                    .collect(),
            )
        })
        .collect::<Vec<_>>();
    let match_count = captures.len();
    let matched = match_count > 0;
    let result = replacement
        .as_deref()
        .map(|replacement| regex.replace_all(&text, replacement).into_owned())
        .unwrap_or_else(|| text.clone());
    let first_match = regex
        .find(&text)
        .map(|found| found.as_str().to_owned())
        .unwrap_or_default();
    let mut outputs = serde_json::Map::new();
    for output in &workflow.outputs {
        let value = match output.name.as_str() {
            "text" | "result" | "value" => serde_json::Value::String(result.clone()),
            "matched" => serde_json::Value::Bool(matched),
            "match_count" => serde_json::json!(match_count),
            "captures" => serde_json::Value::Array(captures.clone()),
            "first_match" => serde_json::Value::String(first_match.clone()),
            "capability" => serde_json::Value::String(TEXT_REGEX_CAPABILITY.to_owned()),
            other => inputs
                .get(other)
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        };
        outputs.insert(output.name.clone(), value);
    }
    Ok(LeafExecution {
        outputs,
        artifacts: Vec::new(),
    })
}

fn execute_json_extract(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let source = inputs.get("value").unwrap_or(&serde_json::Value::Null);
    let path = input_string(inputs, "path").unwrap_or_default();
    let extracted = lookup_json_path(source, &path)
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let found = !extracted.is_null();
    let text = json_value_text(&extracted);
    let mut outputs = serde_json::Map::new();
    for output in &workflow.outputs {
        let value = match output.name.as_str() {
            "value" => extracted.clone(),
            "text" => serde_json::Value::String(text.clone()),
            "found" => serde_json::Value::Bool(found),
            other => inputs
                .get(other)
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        };
        outputs.insert(output.name.clone(), value);
    }
    Ok(LeafExecution {
        outputs,
        artifacts: Vec::new(),
    })
}

fn execute_model_select(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let requirement_id = input_string(inputs, "requirement_id").unwrap_or_default();
    let preferred = input_string(inputs, "preferred").or_else(|| input_string(inputs, "variant"));
    let variants = inputs
        .get("variants")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default();
    let selected = preferred
        .as_deref()
        .and_then(|preferred| {
            variants.iter().find(|variant| {
                variant.get("id").and_then(serde_json::Value::as_str) == Some(preferred)
                    || variant.get("format").and_then(serde_json::Value::as_str) == Some(preferred)
            })
        })
        .cloned()
        .or_else(|| variants.first().cloned())
        .unwrap_or(serde_json::Value::Null);
    let variant_id = selected
        .get("id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default()
        .to_owned();
    let mut outputs = serde_json::Map::new();
    for output in &workflow.outputs {
        let value = match output.name.as_str() {
            "model" | "variant" => selected.clone(),
            "variant_id" => serde_json::Value::String(variant_id.clone()),
            "requirement_id" => serde_json::Value::String(requirement_id.clone()),
            "capability" => serde_json::Value::String(MODEL_SELECT_CAPABILITY.to_owned()),
            other => inputs
                .get(other)
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        };
        outputs.insert(output.name.clone(), value);
    }
    Ok(LeafExecution {
        outputs,
        artifacts: Vec::new(),
    })
}

fn execute_model_lock_check(
    root: &Path,
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let workflow_id = input_string(inputs, "workflow_id").unwrap_or_default();
    let requirement_id = input_string(inputs, "requirement_id").unwrap_or_default();
    let key = format!("{workflow_id}::{requirement_id}");
    let lock_path = root.join("lfw.lock");
    let lock = if lock_path.exists() {
        serde_json::from_slice::<serde_json::Value>(&fs::read(&lock_path)?)
            .map_err(|error| ApiError::InvalidRequest(format!("invalid lfw.lock: {error}")))?
    } else {
        serde_json::Value::Null
    };
    let entry = lock
        .get("models")
        .and_then(|models| models.get(&key))
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let path = entry
        .get("local_paths")
        .and_then(serde_json::Value::as_array)
        .and_then(|paths| paths.first())
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let locked = !entry.is_null();
    let exists = path
        .as_str()
        .map(|path| Path::new(path).exists())
        .unwrap_or(false);
    let mut outputs = serde_json::Map::new();
    for output in &workflow.outputs {
        let value = match output.name.as_str() {
            "locked" => serde_json::Value::Bool(locked),
            "exists" => serde_json::Value::Bool(exists),
            "key" => serde_json::Value::String(key.clone()),
            "path" => path.clone(),
            "entry" => entry.clone(),
            "capability" => serde_json::Value::String(MODEL_LOCK_CHECK_CAPABILITY.to_owned()),
            other => inputs
                .get(other)
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        };
        outputs.insert(output.name.clone(), value);
    }
    Ok(LeafExecution {
        outputs,
        artifacts: Vec::new(),
    })
}

fn execute_builtin_llm_generate(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let prompt = input_string(inputs, "prompt")
        .or_else(|| input_string(inputs, "text"))
        .unwrap_or_default();
    let model = input_string(inputs, "model").unwrap_or_else(|| "mock".to_owned());
    let text = format!("mock:{model}:{prompt}");
    let mut outputs = text_outputs(workflow, inputs, &text);
    outputs.insert("model".to_owned(), model.into());
    outputs.insert("provider".to_owned(), "mock".into());
    outputs.insert(
        "capability".to_owned(),
        serde_json::Value::String(LLM_GENERATE_CAPABILITY.to_owned()),
    );
    Ok(LeafExecution {
        outputs,
        artifacts: Vec::new(),
    })
}

fn execute_llm_classify(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let text = input_string(inputs, "text")
        .or_else(|| input_string(inputs, "prompt"))
        .unwrap_or_default();
    let labels = inputs
        .get("labels")
        .and_then(serde_json::Value::as_array)
        .map(|labels| labels.iter().map(json_value_text).collect::<Vec<_>>())
        .unwrap_or_default();
    let lower = text.to_ascii_lowercase();
    let label = labels
        .iter()
        .find(|label| lower.contains(&label.to_ascii_lowercase()))
        .cloned()
        .or_else(|| labels.first().cloned())
        .unwrap_or_default();
    let confidence = if label.is_empty() { 0.0 } else { 1.0 };
    let mut outputs = serde_json::Map::new();
    for output in &workflow.outputs {
        let value = match output.name.as_str() {
            "label" => serde_json::Value::String(label.clone()),
            "confidence" => serde_json::json!(confidence),
            "capability" => serde_json::Value::String(LLM_CLASSIFY_CAPABILITY.to_owned()),
            other => inputs
                .get(other)
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        };
        outputs.insert(output.name.clone(), value);
    }
    Ok(LeafExecution {
        outputs,
        artifacts: Vec::new(),
    })
}

fn execute_llm_structured_output(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let text = input_string(inputs, "text")
        .or_else(|| input_string(inputs, "prompt"))
        .unwrap_or_default();
    let object = serde_json::from_str::<serde_json::Value>(&text)
        .unwrap_or_else(|_| serde_json::json!({ "text": text }));
    let mut outputs = serde_json::Map::new();
    for output in &workflow.outputs {
        let value = match output.name.as_str() {
            "object" | "value" => object.clone(),
            "json" => serde_json::Value::String(object.to_string()),
            "capability" => serde_json::Value::String(LLM_STRUCTURED_OUTPUT_CAPABILITY.to_owned()),
            other => inputs
                .get(other)
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        };
        outputs.insert(output.name.clone(), value);
    }
    Ok(LeafExecution {
        outputs,
        artifacts: Vec::new(),
    })
}

fn execute_control_if(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let condition = input_bool(inputs, "condition").unwrap_or(false);
    let selected = if condition { "then" } else { "else" };
    let value = if condition {
        inputs
            .get("then_value")
            .or_else(|| inputs.get("then"))
            .cloned()
    } else {
        inputs
            .get("else_value")
            .or_else(|| inputs.get("else"))
            .cloned()
    }
    .unwrap_or(serde_json::Value::Null);
    Ok(LeafExecution {
        outputs: control_outputs(workflow, inputs, value, selected, CONTROL_IF_CAPABILITY),
        artifacts: Vec::new(),
    })
}

fn execute_control_switch(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let selector = input_string(inputs, "selector").unwrap_or_default();
    let cases = inputs.get("cases").and_then(serde_json::Value::as_object);
    let value = cases
        .and_then(|cases| cases.get(&selector))
        .cloned()
        .or_else(|| inputs.get("default").cloned())
        .unwrap_or(serde_json::Value::Null);
    let selected = if cases.is_some_and(|cases| cases.contains_key(&selector)) {
        selector.as_str()
    } else {
        "default"
    };
    Ok(LeafExecution {
        outputs: control_outputs(workflow, inputs, value, selected, CONTROL_SWITCH_CAPABILITY),
        artifacts: Vec::new(),
    })
}

fn execute_control_merge(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let mode = input_string(inputs, "mode").unwrap_or_else(|| "first_non_null".to_owned());
    let a = inputs.get("a").cloned().unwrap_or(serde_json::Value::Null);
    let b = inputs.get("b").cloned().unwrap_or(serde_json::Value::Null);
    let value = match mode.as_str() {
        "object" => merge_objects(a, b),
        "array" => serde_json::json!([a, b]),
        _ => {
            if !a.is_null() {
                a
            } else {
                b
            }
        }
    };
    Ok(LeafExecution {
        outputs: control_outputs(workflow, inputs, value, &mode, CONTROL_MERGE_CAPABILITY),
        artifacts: Vec::new(),
    })
}

fn execute_control_split(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> ApiResult<LeafExecution> {
    let value = inputs
        .get("value")
        .cloned()
        .unwrap_or(serde_json::Value::Null);
    let (first, rest, items) = split_value(value);
    let mut outputs = serde_json::Map::new();
    for output in &workflow.outputs {
        let value = match output.name.as_str() {
            "first" => first.clone(),
            "rest" => rest.clone(),
            "items" => items.clone(),
            "selected" => serde_json::Value::String(CONTROL_SPLIT_CAPABILITY.to_owned()),
            other => inputs
                .get(other)
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        };
        outputs.insert(output.name.clone(), value);
    }
    Ok(LeafExecution {
        outputs,
        artifacts: Vec::new(),
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

fn input_bool(inputs: &serde_json::Map<String, serde_json::Value>, name: &str) -> Option<bool> {
    inputs.get(name).and_then(|value| match value {
        serde_json::Value::Bool(value) => Some(*value),
        serde_json::Value::String(value) => match value.as_str() {
            "true" => Some(true),
            "false" => Some(false),
            _ => None,
        },
        _ => None,
    })
}

fn control_outputs(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
    selected_value: serde_json::Value,
    selected: &str,
    capability: &str,
) -> serde_json::Map<String, serde_json::Value> {
    let mut outputs = serde_json::Map::new();
    for output in &workflow.outputs {
        let value = match output.name.as_str() {
            "value" => selected_value.clone(),
            "selected" => serde_json::Value::String(selected.to_owned()),
            "capability" => serde_json::Value::String(capability.to_owned()),
            other => inputs
                .get(other)
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        };
        outputs.insert(output.name.clone(), value);
    }
    outputs
}

fn merge_objects(a: serde_json::Value, b: serde_json::Value) -> serde_json::Value {
    let mut merged = serde_json::Map::new();
    if let serde_json::Value::Object(map) = a {
        merged.extend(map);
    }
    if let serde_json::Value::Object(map) = b {
        merged.extend(map);
    }
    serde_json::Value::Object(merged)
}

fn split_value(
    value: serde_json::Value,
) -> (serde_json::Value, serde_json::Value, serde_json::Value) {
    match value {
        serde_json::Value::Array(items) => {
            let first = items.first().cloned().unwrap_or(serde_json::Value::Null);
            let rest = serde_json::Value::Array(items.iter().skip(1).cloned().collect());
            let items = serde_json::Value::Array(items);
            (first, rest, items)
        }
        serde_json::Value::Object(map) => {
            let items = map
                .iter()
                .map(|(key, value)| serde_json::json!({ "key": key, "value": value }))
                .collect::<Vec<_>>();
            let first = items.first().cloned().unwrap_or(serde_json::Value::Null);
            let rest = serde_json::Value::Array(items.iter().skip(1).cloned().collect());
            (first, rest, serde_json::Value::Array(items))
        }
        value => (
            value.clone(),
            serde_json::Value::Null,
            serde_json::json!([value]),
        ),
    }
}

fn text_outputs(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
    text: &str,
) -> serde_json::Map<String, serde_json::Value> {
    let mut outputs = serde_json::Map::new();
    for output in &workflow.outputs {
        let value = match output.name.as_str() {
            "text" | "prompt" | "result" | "value" => serde_json::Value::String(text.to_owned()),
            other => inputs
                .get(other)
                .cloned()
                .unwrap_or(serde_json::Value::Null),
        };
        outputs.insert(output.name.clone(), value);
    }
    outputs
}

fn render_template(template: &str, vars: &serde_json::Value) -> String {
    let mut rendered = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        rendered.push_str(&rest[..start]);
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find("}}") else {
            rendered.push_str(&rest[start..]);
            return rendered;
        };
        let key = after_start[..end].trim();
        if let Some(value) = lookup_json_path(vars, key) {
            rendered.push_str(&json_value_text(value));
        }
        rest = &after_start[end + 2..];
    }
    rendered.push_str(rest);
    rendered
}

fn lookup_json_path<'a>(value: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let path = path.trim();
    let path = path.strip_prefix('$').unwrap_or(path);
    let path = path.strip_prefix('.').unwrap_or(path);
    if path.is_empty() {
        return Some(value);
    }
    let mut current = value;
    for segment in path.split('.').filter(|segment| !segment.is_empty()) {
        current = match current {
            serde_json::Value::Object(map) => map.get(segment)?,
            serde_json::Value::Array(items) => items.get(segment.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(current)
}

fn json_value_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(value) => value.clone(),
        serde_json::Value::Null => String::new(),
        value => value.to_string(),
    }
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

fn image_file_artifact(
    input_path: &Path,
    output_path: &Path,
    engine: &str,
    capability: &str,
    dimensions: Option<(u32, u32)>,
) -> WorkflowArtifact {
    let mut metadata = serde_json::Map::new();
    metadata.insert("engine".to_owned(), engine.into());
    metadata.insert("capability".to_owned(), capability.into());
    metadata.insert(
        "source_image_path".to_owned(),
        input_path.display().to_string().into(),
    );
    if let Some((width, height)) = dimensions {
        metadata.insert("width".to_owned(), width.into());
        metadata.insert("height".to_owned(), height.into());
    }

    WorkflowArtifact {
        id: "image".to_owned(),
        kind: "image".to_owned(),
        path: output_path.display().to_string(),
        mime_type: "image/png".to_owned(),
        metadata,
    }
}

struct PreviewTransformArtifact<'a> {
    workflow: &'a WorkflowSpec,
    input_path: &'a Path,
    mask_path: Option<&'a Path>,
    output_path: &'a Path,
    prompt: &'a str,
    seed: u64,
    engine: &'a str,
    capability: &'a str,
    dimensions: Option<(u32, u32)>,
    inputs: &'a serde_json::Map<String, serde_json::Value>,
}

fn preview_transform_artifact(spec: PreviewTransformArtifact<'_>) -> WorkflowArtifact {
    let mut metadata = serde_json::Map::new();
    metadata.insert("engine".to_owned(), spec.engine.into());
    metadata.insert("capability".to_owned(), spec.capability.into());
    metadata.insert("prompt".to_owned(), spec.prompt.to_owned().into());
    metadata.insert("seed".to_owned(), spec.seed.into());
    metadata.insert(
        "source_image_path".to_owned(),
        spec.input_path.display().to_string().into(),
    );
    if let Some(mask_path) = spec.mask_path {
        metadata.insert(
            "mask_path".to_owned(),
            mask_path.display().to_string().into(),
        );
    }
    if let Some((width, height)) = spec.dimensions {
        metadata.insert("width".to_owned(), width.into());
        metadata.insert("height".to_owned(), height.into());
    }
    if let Some(negative) = input_string(spec.inputs, "negative") {
        metadata.insert("negative_prompt".to_owned(), negative.into());
    }
    if let Some(model) = selected_model(spec.workflow, spec.inputs) {
        metadata.insert("model".to_owned(), model);
    }

    WorkflowArtifact {
        id: "image".to_owned(),
        kind: "image".to_owned(),
        path: spec.output_path.display().to_string(),
        mime_type: "image/png".to_owned(),
        metadata,
    }
}

fn preview_image_outputs(
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
        artifacts: vec![artifact],
    })
}

fn mask_artifact(
    mask_a_path: &Path,
    mask_b_path: &Path,
    output_path: &Path,
    mode: &str,
    dimensions: Option<(u32, u32)>,
) -> WorkflowArtifact {
    let mut metadata = serde_json::Map::new();
    metadata.insert("engine".to_owned(), "builtin.mask.compose.v1".into());
    metadata.insert("capability".to_owned(), MASK_COMPOSE_CAPABILITY.into());
    metadata.insert("mode".to_owned(), mode.to_owned().into());
    metadata.insert(
        "mask_a_path".to_owned(),
        mask_a_path.display().to_string().into(),
    );
    metadata.insert(
        "mask_b_path".to_owned(),
        mask_b_path.display().to_string().into(),
    );
    if let Some((width, height)) = dimensions {
        metadata.insert("width".to_owned(), width.into());
        metadata.insert("height".to_owned(), height.into());
    }

    WorkflowArtifact {
        id: "mask".to_owned(),
        kind: "mask".to_owned(),
        path: output_path.display().to_string(),
        mime_type: "image/png".to_owned(),
        metadata,
    }
}

fn mask_path_outputs(
    workflow: &WorkflowSpec,
    inputs: &serde_json::Map<String, serde_json::Value>,
    mask_path: &Path,
    artifact: WorkflowArtifact,
    mode: &str,
) -> ApiResult<LeafExecution> {
    let artifact_value = serde_json::to_value(&artifact)
        .map_err(|error| ApiError::InvalidRequest(error.to_string()))?;
    let mut outputs = serde_json::Map::new();
    for output in &workflow.outputs {
        let value = match output.name.as_str() {
            "mask" | "artifact" => artifact_value.clone(),
            "mask_path" | "output_path" => {
                serde_json::Value::String(mask_path.display().to_string())
            }
            "mode" => serde_json::Value::String(mode.to_owned()),
            "capability" => serde_json::Value::String(MASK_COMPOSE_CAPABILITY.to_owned()),
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

#[derive(Debug, Clone, PartialEq)]
struct PngImage {
    width: u32,
    height: u32,
    color_type: png::ColorType,
    bit_depth: png::BitDepth,
    channels: usize,
    data: Vec<u8>,
}

fn read_png_image(path: &Path) -> ApiResult<PngImage> {
    let file = fs::File::open(path)?;
    let decoder = png::Decoder::new(BufReader::new(file));
    let mut reader = decoder
        .read_info()
        .map_err(|error| ApiError::Io(std::io::Error::other(error)))?;
    let buffer_size = reader.output_buffer_size().ok_or_else(|| {
        ApiError::InvalidRequest(format!("image is too large to decode: {}", path.display()))
    })?;
    let mut data = vec![0; buffer_size];
    let info = reader
        .next_frame(&mut data)
        .map_err(|error| ApiError::Io(std::io::Error::other(error)))?;
    data.truncate(info.buffer_size());
    if info.bit_depth != png::BitDepth::Eight {
        return Err(ApiError::InvalidRequest(format!(
            "only 8-bit PNG images are supported: {}",
            path.display()
        )));
    }
    let channels = png_channels(info.color_type).ok_or_else(|| {
        ApiError::InvalidRequest(format!(
            "indexed PNG images are not supported: {}",
            path.display()
        ))
    })?;
    Ok(PngImage {
        width: info.width,
        height: info.height,
        color_type: info.color_type,
        bit_depth: info.bit_depth,
        channels,
        data,
    })
}

fn write_png_image(path: &Path, image: &PngImage) -> ApiResult<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    let file = fs::File::create(path)?;
    let writer = BufWriter::new(file);
    let mut encoder = png::Encoder::new(writer, image.width, image.height);
    encoder.set_color(image.color_type);
    encoder.set_depth(image.bit_depth);
    let mut png = encoder
        .write_header()
        .map_err(|error| ApiError::Io(std::io::Error::other(error)))?;
    png.write_image_data(&image.data)
        .map_err(|error| ApiError::Io(std::io::Error::other(error)))
}

fn resize_png_image(image: &PngImage, width: u32, height: u32) -> PngImage {
    let mut data = vec![0; width as usize * height as usize * image.channels];
    for y in 0..height {
        let src_y = (u64::from(y) * u64::from(image.height) / u64::from(height)) as u32;
        for x in 0..width {
            let src_x = (u64::from(x) * u64::from(image.width) / u64::from(width)) as u32;
            let src = pixel_offset(src_x, src_y, image.width, image.channels);
            let dst = pixel_offset(x, y, width, image.channels);
            data[dst..dst + image.channels].copy_from_slice(&image.data[src..src + image.channels]);
        }
    }
    PngImage {
        width,
        height,
        color_type: image.color_type,
        bit_depth: image.bit_depth,
        channels: image.channels,
        data,
    }
}

fn crop_png_image(
    image: &PngImage,
    x: u32,
    y: u32,
    width: u32,
    height: u32,
) -> ApiResult<PngImage> {
    if width == 0 || height == 0 || x >= image.width || y >= image.height {
        return Err(ApiError::InvalidRequest(
            "crop rectangle must intersect the source image".to_owned(),
        ));
    }
    let width = width.min(image.width - x);
    let height = height.min(image.height - y);
    let mut data = vec![0; width as usize * height as usize * image.channels];
    for row in 0..height {
        let src = pixel_offset(x, y + row, image.width, image.channels);
        let dst = pixel_offset(0, row, width, image.channels);
        let len = width as usize * image.channels;
        data[dst..dst + len].copy_from_slice(&image.data[src..src + len]);
    }
    Ok(PngImage {
        width,
        height,
        color_type: image.color_type,
        bit_depth: image.bit_depth,
        channels: image.channels,
        data,
    })
}

fn preview_edit_image(
    image: &PngImage,
    seed: u64,
    prompt: &str,
    mask: Option<&PngImage>,
) -> PngImage {
    let mut edited = image.clone();
    let color_channels = color_channels(image.color_type);
    let prompt_mix = stable_seed(prompt);
    for y in 0..image.height {
        for x in 0..image.width {
            let offset = pixel_offset(x, y, image.width, image.channels);
            let mask_strength = mask
                .map(|mask| luminance_at(mask, x, y) as u16)
                .unwrap_or(255);
            if mask_strength == 0 {
                continue;
            }
            let base = seed ^ prompt_mix ^ ((x as u64) << 32) ^ y as u64;
            for channel in 0..color_channels {
                let current = edited.data[offset + channel] as u16;
                let generated = ((base >> (channel * 8)) & 0xff) as u16;
                let blend = (generated * mask_strength + current * (255 - mask_strength)) / 255;
                edited.data[offset + channel] = ((current * 3 + blend) / 4) as u8;
            }
        }
    }
    edited
}

fn compose_masks(mask_a: &PngImage, mask_b: &PngImage, mode: &str) -> ApiResult<PngImage> {
    let mut data = vec![0; mask_a.width as usize * mask_a.height as usize];
    for y in 0..mask_a.height {
        for x in 0..mask_a.width {
            let a = luminance_at(mask_a, x, y);
            let b = luminance_at(mask_b, x, y);
            let value = match mode {
                "add" => a.saturating_add(b),
                "multiply" | "intersect" => ((u16::from(a) * u16::from(b)) / 255) as u8,
                "min" => a.min(b),
                "subtract" => a.saturating_sub(b),
                "max" | "union" => a.max(b),
                other => {
                    return Err(ApiError::InvalidRequest(format!(
                        "unsupported mask compose mode: {other}"
                    )));
                }
            };
            data[(y as usize * mask_a.width as usize) + x as usize] = value;
        }
    }
    Ok(PngImage {
        width: mask_a.width,
        height: mask_a.height,
        color_type: png::ColorType::Grayscale,
        bit_depth: png::BitDepth::Eight,
        channels: 1,
        data,
    })
}

fn luminance_at(image: &PngImage, x: u32, y: u32) -> u8 {
    let offset = pixel_offset(x, y, image.width, image.channels);
    match image.color_type {
        png::ColorType::Rgb | png::ColorType::Rgba => {
            let r = u16::from(image.data[offset]);
            let g = u16::from(image.data[offset + 1]);
            let b = u16::from(image.data[offset + 2]);
            ((r * 77 + g * 150 + b * 29) / 256) as u8
        }
        png::ColorType::Grayscale | png::ColorType::GrayscaleAlpha => image.data[offset],
        png::ColorType::Indexed => 0,
    }
}

fn color_channels(color_type: png::ColorType) -> usize {
    match color_type {
        png::ColorType::Rgb => 3,
        png::ColorType::Rgba => 3,
        png::ColorType::Grayscale => 1,
        png::ColorType::GrayscaleAlpha => 1,
        png::ColorType::Indexed => 0,
    }
}

fn pixel_offset(x: u32, y: u32, width: u32, channels: usize) -> usize {
    (y as usize * width as usize + x as usize) * channels
}

fn png_channels(color_type: png::ColorType) -> Option<usize> {
    match color_type {
        png::ColorType::Rgb => Some(3),
        png::ColorType::Rgba => Some(4),
        png::ColorType::Grayscale => Some(1),
        png::ColorType::GrayscaleAlpha => Some(2),
        png::ColorType::Indexed => None,
    }
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

fn mask_output_path(
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
        .unwrap_or("mask");
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
