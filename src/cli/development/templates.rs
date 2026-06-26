use super::{ProjectConfigTemplateOptions, SkillTemplateOptions};
use crate::api::{ApiService, read_workflow_source};
use crate::cli::{CliError, CliResult};
use crate::workflow::{PortSpec, WorkflowSpec};
use std::fs;
use std::path::{Path, PathBuf};

pub(super) fn project_config_template_json(
    service: &ApiService,
    options: &ProjectConfigTemplateOptions,
) -> CliResult<serde_json::Value> {
    let project_config_path = service.project_workspace_config_path();
    let project_config_present = project_config_path.exists();
    let (
        expected,
        optional,
        default_workflow_sources,
        project_config_valid,
        project_config_error,
        project_submodule_update_command,
    ) = match service.project_workspaces() {
        Ok(catalog) => (
            catalog
                .workspaces
                .iter()
                .filter(|workspace| workspace.expected)
                .map(|workspace| workspace.name.clone())
                .collect::<Vec<_>>(),
            catalog.optional_workspace_names,
            catalog.default_workflow_sources,
            catalog.project_config_valid,
            catalog.project_config_error,
            catalog.project_submodule_update_command,
        ),
        Err(error) => {
            let (expected, optional, default_workflow_sources) =
                service.default_project_config_values();
            let project_submodule_update_command = service.project_submodule_update_command(
                expected
                    .iter()
                    .chain(default_workflow_sources.iter())
                    .chain(optional.iter())
                    .map(String::as_str),
            );
            (
                expected,
                optional,
                default_workflow_sources,
                false,
                Some(error.to_string()),
                project_submodule_update_command,
            )
        }
    };
    let source = project_config_template_source(&expected, &optional, &default_workflow_sources);
    let mut value = serde_json::json!({
        "suggested_path": project_config_path,
        "project_config_present": project_config_present,
        "project_config_valid": project_config_valid,
        "project_config_error": project_config_error,
        "project_config_template_command": service.project_config_template_command(),
        "project_config_write_command": service.project_config_write_command(),
        "expected_workspaces": expected,
        "optional_workspaces": optional,
        "default_workflow_sources": default_workflow_sources,
        "project_submodule_update_command": project_submodule_update_command,
        "source": source,
        "written": false,
    });

    if options.write {
        let path = project_config_path;
        if path.exists() && !options.force {
            return Err(CliError::Usage(format!(
                "{} already exists; pass --force to overwrite",
                path.display()
            )));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, &source)?;
        value["written"] = serde_json::Value::Bool(true);
        value["path"] = serde_json::Value::String(path.display().to_string());
    }

    Ok(value)
}

fn project_config_template_source(
    expected: &[String],
    optional: &[String],
    default_sources: &[String],
) -> String {
    format!(
        "[workspaces]\nexpected = {}\noptional = {}\n\n[workflows]\ndefault_sources = {}\n",
        toml_array(expected),
        toml_array(optional),
        toml_array(default_sources)
    )
}

fn toml_array(values: &[String]) -> String {
    if values.is_empty() {
        return "[]".to_owned();
    }
    let items = values
        .iter()
        .map(|value| format!("  {:?},", value))
        .collect::<Vec<_>>()
        .join("\n");
    format!("[\n{items}\n]")
}

pub(super) fn skill_template_json(
    service: &ApiService,
    workflow: &WorkflowSpec,
    options: &SkillTemplateOptions,
) -> CliResult<serde_json::Value> {
    let skill_name = skill_name_from_workflow_id(&workflow.id);
    let source = skill_template_source(workflow);
    let mut value = serde_json::json!({
        "workflow_id": workflow.id,
        "skill_name": skill_name,
        "suggested_path": format!(".agent/skills/{skill_name}/SKILL.md"),
        "source": source,
        "written": false,
    });

    if options.write {
        let crate_dir =
            find_workflow_crate_dir(service.repo_root(), &workflow.id)?.ok_or_else(|| {
                CliError::Usage(format!(
                    "workflow crate for {} could not be located under workflows/ or projects/",
                    workflow.id
                ))
            })?;
        let path = crate_dir
            .join(".agent")
            .join("skills")
            .join(&skill_name)
            .join("SKILL.md");
        let overwritten = path.exists();
        if overwritten && !options.force {
            return Err(CliError::Usage(format!(
                "{} already exists; pass --force to overwrite",
                path.display()
            )));
        }
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(&path, &source)?;
        value["written"] = serde_json::Value::Bool(true);
        value["overwritten"] = serde_json::Value::Bool(overwritten);
        value["path"] = serde_json::Value::String(path.to_string_lossy().into_owned());
    }

    Ok(value)
}

fn find_workflow_crate_dir(root: &Path, workflow_id: &str) -> CliResult<Option<PathBuf>> {
    let mut collections = vec![
        root.join("workflows"),
        root.join("lightflow").join("workflows"),
    ];
    let projects = root.join("projects");
    let mut project_roots = read_sorted_dirs(&projects)?;
    for preferred in preferred_project_names(workflow_id) {
        let project = projects.join(&preferred);
        if project.is_dir() {
            collections.push(project.join("workflows"));
            project_roots
                .retain(|root| root.file_name().and_then(|name| name.to_str()) != Some(&preferred));
        }
    }
    for project in project_roots {
        collections.push(project.join("workflows"));
    }

    for collection in collections {
        for crate_dir in workflow_crates_in_collection(&collection)? {
            let lib = crate_dir.join("src").join("lib.rs");
            let Ok(workflow) = read_workflow_source(&lib) else {
                continue;
            };
            if workflow.id == workflow_id {
                return Ok(Some(crate_dir));
            }
        }
    }

    Ok(None)
}

fn preferred_project_names(workflow_id: &str) -> Vec<String> {
    let mut names = Vec::new();
    if workflow_id == "lightflow.std" || workflow_id.starts_with("lightflow.") {
        names.push("lightflow-std".to_owned());
    }
    if workflow_id.starts_with("lightflow.flux.") {
        names.insert(0, "lightflow-flux".to_owned());
    }
    if workflow_id.starts_with("lightflow.rig.") {
        names.insert(0, "lightflow-rig".to_owned());
    }
    names.dedup();
    names
}

fn workflow_crates_in_collection(collection: &Path) -> CliResult<Vec<PathBuf>> {
    let mut crates = Vec::new();
    for category_path in read_sorted_dirs(collection)? {
        if !category_path.is_dir() {
            continue;
        }
        if category_path.join("Cargo.toml").is_file()
            && category_path.join("src").join("lib.rs").is_file()
        {
            crates.push(category_path);
            continue;
        }
        for crate_dir in read_sorted_dirs(&category_path)? {
            if crate_dir.join("Cargo.toml").is_file()
                && crate_dir.join("src").join("lib.rs").is_file()
            {
                crates.push(crate_dir);
            }
        }
    }
    crates.sort();
    Ok(crates)
}

fn read_sorted_dirs(path: &Path) -> CliResult<Vec<PathBuf>> {
    let Ok(entries) = fs::read_dir(path) else {
        return Ok(Vec::new());
    };
    let mut paths = entries
        .map(|entry| entry.map(|entry| entry.path()))
        .collect::<Result<Vec<_>, _>>()?;
    paths.sort();
    Ok(paths)
}

fn skill_template_source(workflow: &WorkflowSpec) -> String {
    let title = if workflow.name.trim().is_empty() {
        title_from_workflow_id(&workflow.id)
    } else {
        workflow.name.clone()
    };
    let input_lines = port_lines(&workflow.inputs, "Input");
    let output_lines = port_lines(&workflow.outputs, "Output");
    let cli_command = format!("lfw run {}", workflow.id);
    let api_body = serde_json::json!({ "inputs": sample_inputs(workflow) }).to_string();
    format!(
        "---\nname: {title}\ndescription: Use this skill when working with the {workflow_id} LightFlow workflow, configuring its inputs, running it through lfw or HTTP, or composing it with other workflows.\nversion: 0.1.0\n---\n\n# {title}\n\nUse this skill to understand the workflow contract for `{workflow_id}`.\n\n## Workflow\n\n- Workflow id: `{workflow_id}`\n{input_lines}{output_lines}\n## CLI Usage\n\n```bash\n{cli_command}\n```\n\n## API Usage\n\nStart `lfw serve`, then call the workflow through the shared HTTP run contract:\n\n```bash\ncurl -sS -X POST http://127.0.0.1:5174/workflows/{workflow_id}/run \\\n  -H 'content-type: application/json' \\\n  -d '{api_body}'\n```\n\nRun `lfw help {workflow_id}` to inspect the generated Node Schema v1 contract.\n",
        workflow_id = workflow.id
    )
}

fn port_lines(ports: &[PortSpec], label: &str) -> String {
    if ports.is_empty() {
        return String::new();
    }
    ports
        .iter()
        .map(|port| {
            let mut parts = vec![format!("type `{}`", port.ty)];
            if port.required == Some(true) {
                parts.push("required".to_owned());
            }
            if let Some(widget) = &port.widget {
                parts.push(format!("widget `{widget}`"));
            }
            if let Some(kind) = &port.artifact_kind {
                parts.push(format!("artifact kind `{kind}`"));
            }
            if let Some(model) = &port.model_requirement {
                parts.push(format!("model requirement `{model}`"));
            }
            let description = port.description.as_deref().unwrap_or("No description.");
            format!(
                "- {label} `{}`: {}; {description}\n",
                port.name,
                parts.join("; ")
            )
        })
        .collect()
}

fn sample_inputs(workflow: &WorkflowSpec) -> serde_json::Map<String, serde_json::Value> {
    workflow
        .inputs
        .iter()
        .map(|port| {
            (
                port.name.clone(),
                port.default
                    .clone()
                    .unwrap_or_else(|| sample_input_value(&port.ty)),
            )
        })
        .collect()
}

fn sample_input_value(ty: &str) -> serde_json::Value {
    match ty {
        "text" | "string" | "path" => "TODO".into(),
        "integer" | "int" => 0.into(),
        "number" | "float" => serde_json::json!(0.0),
        "boolean" | "bool" => false.into(),
        "json" => serde_json::json!({}),
        _ => serde_json::Value::Null,
    }
}

fn skill_name_from_workflow_id(workflow_id: &str) -> String {
    workflow_id
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .split('-')
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join("-")
}

fn title_from_workflow_id(workflow_id: &str) -> String {
    workflow_id
        .rsplit('.')
        .next()
        .unwrap_or(workflow_id)
        .split(['_', '-'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => first.to_uppercase().collect::<String>() + chars.as_str(),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}
