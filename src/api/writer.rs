use super::{ApiError, ApiResult, workflow_package_identity};
use crate::workflow::{
    CargoDependencySource, ModelProvider, PortSpec, WorkflowCondition, WorkflowNodeKind,
    WorkflowSpec,
};
use std::fs;
use std::path::Path;

pub(super) fn write_text_atomic(path: &Path, body: &str) -> ApiResult<()> {
    let parent = path
        .parent()
        .ok_or_else(|| ApiError::InvalidRequest("workflow path has no parent".to_owned()))?;
    fs::create_dir_all(parent).map_err(ApiError::from)?;
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ApiError::InvalidRequest("workflow path has no file name".to_owned()))?;
    let temp_path = parent.join(format!("{file_name}.tmp"));
    fs::write(&temp_path, body).map_err(ApiError::from)?;
    fs::rename(temp_path, path).map_err(ApiError::from)
}

pub(super) fn ensure_workflow_manifest(crate_dir: &Path, workflow: &WorkflowSpec) -> ApiResult<()> {
    let manifest = crate_dir.join("Cargo.toml");
    if manifest.exists() {
        let (id, version) = workflow_package_identity(&manifest)?;
        if id != workflow.id || version != workflow.version {
            return Err(ApiError::InvalidRequest(format!(
                "workflow manifest {} defines {id} {version}, but request defines {} {}",
                manifest.display(),
                workflow.id,
                workflow.version
            )));
        }
        return Ok(());
    }

    let package = workflow_package_name(&workflow.id)?;
    let source = format!(
        "[package]\nname = {package:?}\nversion = {:?}\nedition = \"2024\"\n\n[dependencies]\nlightflow = {{ workspace = true }}\n",
        workflow.version
    );
    write_text_atomic(&manifest, &source)
}

fn workflow_package_name(workflow_id: &str) -> ApiResult<String> {
    let suffix = workflow_id
        .strip_prefix("lightflow.")
        .filter(|suffix| {
            !suffix.is_empty()
                && suffix
                    .chars()
                    .all(|character| character.is_ascii_alphanumeric() || character == '_')
        })
        .ok_or_else(|| {
            ApiError::InvalidRequest(format!(
                "workflow id {workflow_id} cannot be represented by a LightFlow Cargo package"
            ))
        })?;
    Ok(format!("lightflow-{}", suffix.replace('_', "-")))
}

pub(super) fn workflow_source(workflow: &WorkflowSpec) -> String {
    let mut source = String::from("use lightflow::preload::*;\n\n");
    source.push_str("pub fn define() -> WorkflowSpec {\n");
    source.push_str("    workflow!()\n");
    source.push_str(&format!("        .name({})\n", rust_string(&workflow.name)));
    if let Some(description) = &workflow.description {
        source.push_str(&format!(
            "        .description({})\n",
            rust_string(description)
        ));
    }
    for input in &workflow.inputs {
        source.push_str(&format!(
            "        .input({}, {})\n",
            rust_string(&input.name),
            rust_string(&input.ty)
        ));
        push_input_port_metadata(&mut source, input);
    }
    for output in &workflow.outputs {
        source.push_str(&format!(
            "        .output({}, {})\n",
            rust_string(&output.name),
            rust_string(&output.ty)
        ));
        push_output_port_metadata(&mut source, output);
    }
    for dependency in &workflow.dependencies {
        if let Some(install) = &dependency.install {
            match &install.source {
                Some(CargoDependencySource::Path(path)) => {
                    source.push_str(&format!(
                        "        .depends_on_path({}, {}, {}, {})\n",
                        rust_string(&dependency.workflow_id),
                        rust_string(dependency.version.as_deref().unwrap_or("*")),
                        rust_string(&install.crate_name),
                        rust_string(path)
                    ));
                }
                Some(CargoDependencySource::Git(git)) => {
                    source.push_str(&format!(
                        "        .depends_on_git({}, {}, {}, {}, {})\n",
                        rust_string(&dependency.workflow_id),
                        rust_string(dependency.version.as_deref().unwrap_or("*")),
                        rust_string(&install.crate_name),
                        rust_string(git),
                        rust_string(install.package.as_deref().unwrap_or(""))
                    ));
                }
                None => {
                    source.push_str(&format!(
                        "        .depends_on_crate({}, {}, {})\n",
                        rust_string(&dependency.workflow_id),
                        rust_string(dependency.version.as_deref().unwrap_or("*")),
                        rust_string(&install.crate_name)
                    ));
                }
            }
        } else {
            source.push_str(&format!(
                "        .depends_on({}, {})\n",
                rust_string(&dependency.workflow_id),
                rust_string(dependency.version.as_deref().unwrap_or("*"))
            ));
        }
    }
    for model in &workflow.models {
        if model.variants.is_empty() {
            source.push_str(&format!(
                "        .model({}, {})\n",
                rust_string(&model.id),
                rust_string(&model.capability)
            ));
        } else {
            for variant in &model.variants {
                if variant.provider != ModelProvider::HuggingFace {
                    continue;
                }
                source.push_str(&format!(
                    "        .hf_model({}, {}, {}, {}, {}, {})\n",
                    rust_string(&model.id),
                    rust_string(&variant.id),
                    rust_string(&model.capability),
                    rust_string(&variant.format),
                    rust_string(&variant.repo),
                    rust_string(variant.file.as_deref().unwrap_or(""))
                ));
            }
        }
    }
    for node in &workflow.nodes {
        match node.kind {
            WorkflowNodeKind::Workflow => {
                let method = if node.disabled {
                    "disabled_node"
                } else {
                    "node"
                };
                source.push_str(&format!(
                    "        .{method}({}, {})\n",
                    rust_string(&node.id),
                    rust_string(&node.workflow_id)
                ));
            }
            WorkflowNodeKind::If => {
                if let Some(WorkflowCondition::InputEquals { input, value }) = &node.condition
                    && let Some(expected) = value.as_bool()
                {
                    source.push_str(&format!(
                        "        .if_node({}, {}, {}, {}, {})\n",
                        rust_string(&node.id),
                        rust_string(input),
                        expected,
                        rust_string(node.then_workflow_id.as_deref().unwrap_or("")),
                        rust_string(node.else_workflow_id.as_deref().unwrap_or(""))
                    ));
                }
            }
        }
    }
    for edge in &workflow.edges {
        source.push_str(&format!(
            "        .edge({}, {}, {}, {})\n",
            rust_string(&edge.from.node),
            rust_string(&edge.from.port),
            rust_string(&edge.to.node),
            rust_string(&edge.to.port)
        ));
    }
    source.push_str("        .build()\n");
    source.push_str("}\n");
    source
}

fn rust_string(value: &str) -> String {
    format!("{value:?}")
}

fn push_input_port_metadata(source: &mut String, port: &PortSpec) {
    if let Some(description) = &port.description {
        source.push_str(&format!(
            "        .input_description({}, {})\n",
            rust_string(&port.name),
            rust_string(description)
        ));
    }
    if let Some(required) = port.required {
        source.push_str(&format!(
            "        .input_required({}, {required})\n",
            rust_string(&port.name)
        ));
    }
    if let Some(default) = &port.default {
        source.push_str(&format!(
            "        .input_default_json({}, {})\n",
            rust_string(&port.name),
            rust_string(&default.to_string())
        ));
    }
    if let (Some(min), Some(max), Some(step)) = (port.min, port.max, port.step) {
        source.push_str(&format!(
            "        .input_range({}, {min}, {max}, {step})\n",
            rust_string(&port.name)
        ));
    }
    if !port.enum_values.is_empty() {
        source.push_str(&format!(
            "        .input_enum_json({}, {})\n",
            rust_string(&port.name),
            rust_string(&serde_json::Value::Array(port.enum_values.clone()).to_string())
        ));
    }
    if let Some(widget) = &port.widget {
        source.push_str(&format!(
            "        .input_widget({}, {})\n",
            rust_string(&port.name),
            rust_string(widget)
        ));
    }
    if let Some(artifact_kind) = &port.artifact_kind {
        source.push_str(&format!(
            "        .input_artifact_kind({}, {})\n",
            rust_string(&port.name),
            rust_string(artifact_kind)
        ));
    }
    if let Some(model_requirement) = &port.model_requirement {
        source.push_str(&format!(
            "        .input_model_requirement({}, {})\n",
            rust_string(&port.name),
            rust_string(model_requirement)
        ));
    }
}

fn push_output_port_metadata(source: &mut String, port: &PortSpec) {
    if let Some(description) = &port.description {
        source.push_str(&format!(
            "        .output_description({}, {})\n",
            rust_string(&port.name),
            rust_string(description)
        ));
    }
    if let Some(artifact_kind) = &port.artifact_kind {
        source.push_str(&format!(
            "        .output_artifact_kind({}, {})\n",
            rust_string(&port.name),
            rust_string(artifact_kind)
        ));
    }
    if let Some(model_requirement) = &port.model_requirement {
        source.push_str(&format!(
            "        .output_model_requirement({}, {})\n",
            rust_string(&port.name),
            rust_string(model_requirement)
        ));
    }
}
