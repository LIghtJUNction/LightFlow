use crate::api::{ApiError, ApiService};
use crate::server;
use crate::workflow::WorkflowSpec;
use serde::Serialize;
use serde_json::json;
use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

/// Run the LightFlow CLI from process arguments.
pub async fn run_from_env() -> CliResult<()> {
    run(env::args().skip(1).collect()).await
}

/// Run the LightFlow CLI with explicit arguments.
pub async fn run(args: Vec<String>) -> CliResult<()> {
    let Some(command) = args.first().map(String::as_str) else {
        return Err(CliError::Usage(usage()));
    };
    let args = &args[1..];
    let service = ApiService::new(env::current_dir()?);

    match command {
        "init" => {
            let root = args
                .first()
                .map(PathBuf::from)
                .unwrap_or(env::current_dir()?);
            ensure_no_extra_args(args, 1, "init")?;
            print_json(&init_project(&root)?)?;
        }
        "add" => {
            let workflow_id = normalize_workflow_id(required_arg(args, 0, "workflow id")?);
            let name = optional_name(args, 1, "add")?;
            print_json(&add_workflow(
                Path::new("."),
                &workflow_id,
                name.as_deref(),
            )?)?;
        }
        "list" | "ls" => {
            let mode = parse_list_mode(args)?;
            print_json(&list_workflows(&service, mode)?)?;
        }
        "workflows" => {
            let action = required_arg(args, 0, "workflow action")?;
            match action {
                "list" => {
                    ensure_no_extra_args(args, 1, "workflows list")?;
                    print_json(&service.list_workflows()?)?;
                }
                "get" => {
                    let workflow_id = required_arg(args, 1, "workflow id")?;
                    ensure_no_extra_args(args, 2, "workflows get")?;
                    print_json(&service.get_workflow(workflow_id)?)?;
                }
                "deps" | "dependencies" => {
                    let workflow_id = required_arg(args, 1, "workflow id")?;
                    ensure_no_extra_args(args, 2, "workflows deps")?;
                    print_json(&service.workflow_dependencies(workflow_id)?)?;
                }
                "validate" => {
                    let workflow: WorkflowSpec = serde_json::from_value(request_json(
                        required_arg(args, 1, "workflow json")?,
                    )?)?;
                    ensure_no_extra_args(args, 2, "workflows validate")?;
                    print_json(&service.validate_workflow(&workflow))?;
                }
                "save" => {
                    let workflow: WorkflowSpec = serde_json::from_value(request_json(
                        required_arg(args, 1, "workflow json")?,
                    )?)?;
                    ensure_no_extra_args(args, 2, "workflows save")?;
                    print_json(&service.save_workflow(workflow)?)?;
                }
                _ => {
                    return Err(CliError::Usage(
                        "workflow action must be list|get|deps|validate|save".to_owned(),
                    ));
                }
            }
        }
        "deps" | "dependencies" => {
            let workflow_id = required_arg(args, 0, "workflow id")?;
            ensure_no_extra_args(args, 1, "deps")?;
            print_json(&service.workflow_dependencies(workflow_id)?)?;
        }
        "serve" => {
            let bind = parse_bind_addr(args, command)?;
            server::serve(service, &bind).await?;
        }
        "-h" | "--help" | "help" => return Err(CliError::Usage(usage())),
        _ => return Err(CliError::Usage(usage())),
    }

    Ok(())
}

fn print_json(value: &impl Serialize) -> CliResult<()> {
    let stdout = io::stdout();
    let mut handle = stdout.lock();
    serde_json::to_writer_pretty(&mut handle, value)?;
    println!();
    Ok(())
}

fn request_body(argument: &str) -> CliResult<String> {
    if argument == "-" {
        let mut body = String::new();
        io::stdin().read_to_string(&mut body)?;
        return Ok(body);
    }
    if let Some(path) = argument.strip_prefix('@') {
        return std::fs::read_to_string(path).map_err(CliError::from);
    }
    Ok(argument.to_owned())
}

fn request_json(argument: &str) -> CliResult<serde_json::Value> {
    let body = request_body(argument)?;
    serde_json::from_str(&body).map_err(CliError::from)
}

fn required_arg<'a>(args: &'a [String], index: usize, label: &str) -> CliResult<&'a str> {
    args.get(index)
        .map(String::as_str)
        .ok_or_else(|| CliError::Usage(format!("missing {label}")))
}

fn parse_bind_addr(args: &[String], command: &str) -> CliResult<String> {
    let mut host = "127.0.0.1".to_owned();
    let mut port = "5174".to_owned();
    let mut index = 0;
    while index < args.len() {
        let flag = args[index].as_str();
        match flag {
            "--host" => host = required_flag_value(args, index, flag)?.to_owned(),
            "--port" => port = required_flag_value(args, index, flag)?.to_owned(),
            _ => {
                return Err(CliError::Usage(format!(
                    "unexpected argument for {command}: {flag}"
                )));
            }
        }
        index += 2;
    }
    Ok(format!("{host}:{port}"))
}

fn required_flag_value<'a>(args: &'a [String], index: usize, flag: &str) -> CliResult<&'a str> {
    let value = args
        .get(index + 1)
        .map(String::as_str)
        .ok_or_else(|| CliError::Usage(format!("missing value for {flag}")))?;
    if value.starts_with("--") {
        return Err(CliError::Usage(format!("missing value for {flag}")));
    }
    Ok(value)
}

fn ensure_no_extra_args(args: &[String], max_len: usize, command: &str) -> CliResult<()> {
    if let Some(extra) = args.get(max_len) {
        return Err(CliError::Usage(format!(
            "unexpected argument for {command}: {extra}"
        )));
    }
    Ok(())
}

fn usage() -> String {
    [
        "usage:",
        "  lfw init [path]",
        "  lfw add <workflow_id> [--name <name>]",
        "  lfw list [--brief|--detail]",
        "  lfw ls [--brief|--detail]",
        "  lfw workflows list",
        "  lfw workflows get <workflow_id>",
        "  lfw workflows deps <workflow_id>",
        "  lfw workflows validate <json|-|@file>",
        "  lfw workflows save <json|-|@file>",
        "  lfw deps <workflow_id>",
        "  lfw serve [--host <host>] [--port <port>]",
    ]
    .join("\n")
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum ListMode {
    Brief,
    Detail,
}

fn parse_list_mode(args: &[String]) -> CliResult<ListMode> {
    let mut mode = ListMode::Brief;
    for arg in args {
        match arg.as_str() {
            "--brief" | "--short" => mode = ListMode::Brief,
            "--detail" | "--detailed" | "-l" => mode = ListMode::Detail,
            _ => {
                return Err(CliError::Usage(format!(
                    "unexpected argument for list: {arg}"
                )));
            }
        }
    }
    Ok(mode)
}

fn list_workflows(service: &ApiService, mode: ListMode) -> CliResult<serde_json::Value> {
    match mode {
        ListMode::Brief => Ok(serde_json::to_value(service.list_workflows()?)?),
        ListMode::Detail => {
            let workflows = service
                .list_workflows()?
                .workflows
                .into_iter()
                .map(|summary| service.get_workflow(&summary.id))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(json!({ "workflows": workflows }))
        }
    }
}

/// CLI result type.
pub type CliResult<T> = Result<T, CliError>;

/// CLI error with stable exit-code mapping.
#[derive(Debug)]
pub enum CliError {
    Usage(String),
    Api(ApiError),
    Io(io::Error),
    Json(serde_json::Error),
}

impl CliError {
    /// Process exit code for this error.
    #[must_use]
    pub const fn exit_code(&self) -> i32 {
        match self {
            Self::Usage(_) | Self::Api(_) => 2,
            Self::Io(_) | Self::Json(_) => 1,
        }
    }
}

impl std::fmt::Display for CliError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Usage(message) => f.write_str(message),
            Self::Api(error) => std::fmt::Display::fmt(error, f),
            Self::Io(error) => std::fmt::Display::fmt(error, f),
            Self::Json(error) => std::fmt::Display::fmt(error, f),
        }
    }
}

impl std::error::Error for CliError {}

impl From<ApiError> for CliError {
    fn from(error: ApiError) -> Self {
        Self::Api(error)
    }
}

impl From<io::Error> for CliError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for CliError {
    fn from(error: serde_json::Error) -> Self {
        Self::Json(error)
    }
}

fn optional_name(args: &[String], start: usize, command: &str) -> CliResult<Option<String>> {
    let mut name = None;
    let mut index = start;
    while index < args.len() {
        let flag = args[index].as_str();
        match flag {
            "--name" => {
                if name.is_some() {
                    return Err(CliError::Usage("duplicate flag --name".to_owned()));
                }
                name = Some(required_flag_value(args, index, flag)?.to_owned());
            }
            _ => {
                return Err(CliError::Usage(format!(
                    "unexpected argument for {command}: {flag}"
                )));
            }
        }
        index += 2;
    }
    Ok(name)
}

fn init_project(root: &Path) -> CliResult<serde_json::Value> {
    let lightflow = root.join("lightflow");
    let workflows = lightflow.join("workflows");
    fs::create_dir_all(&workflows)?;

    let mut created = Vec::new();
    write_new_text(
        &root.join("Cargo.toml"),
        &workspace_manifest(),
        &mut created,
    )?;
    write_new_text(
        &lightflow.join("README.md"),
        "# LightFlow Project\n\nThis repository is a Cargo workspace for source-controlled Rust workflow crates.\n",
        &mut created,
    )?;
    write_new_text(
        &workflows.join("README.md"),
        "# Workflows\n\nEach directory is one workflow crate. The workflow definition lives in `src/lib.rs`.\n",
        &mut created,
    )?;
    write_new_text(
        &workflow_manifest_path(root, "lightflow.example"),
        &workflow_manifest("lightflow.example"),
        &mut created,
    )?;
    write_new_text(
        &workflow_source_path(root, "lightflow.example"),
        &example_workflow_source("lightflow.example", "Example Workflow"),
        &mut created,
    )?;

    Ok(json!({
        "project_root": root,
        "created": created
    }))
}

fn add_workflow(
    root: &Path,
    workflow_id: &str,
    name: Option<&str>,
) -> CliResult<serde_json::Value> {
    validate_spec_id(workflow_id, "workflow id")?;
    let mut created = Vec::new();
    ensure_workspace_manifest(root, &mut created)?;
    let manifest_path = workflow_manifest_path(root, workflow_id);
    let source_path = workflow_source_path(root, workflow_id);
    let workflow_source =
        example_workflow_source(workflow_id, name.unwrap_or(&title_from_id(workflow_id)));
    write_new_text(
        &manifest_path,
        &workflow_manifest(workflow_id),
        &mut created,
    )?;
    write_new_text(&source_path, &workflow_source, &mut created)?;
    Ok(json!({
        "workflow_id": workflow_id,
        "crate_dir": workflow_crate_dir(root, workflow_id),
        "path": source_path,
        "created": created
    }))
}

fn ensure_workspace_manifest(root: &Path, created: &mut Vec<String>) -> CliResult<()> {
    let manifest_path = root.join("Cargo.toml");
    if manifest_path.exists() {
        return Ok(());
    }
    write_new_text(&manifest_path, &workspace_manifest(), created)
}

fn workspace_manifest() -> String {
    format!(
        "[workspace]\nresolver = \"3\"\nmembers = [\"lightflow/workflows/*\"]\n\n[workspace.dependencies]\nlightflow = {{ git = {:?} }}\n",
        env!("CARGO_PKG_REPOSITORY")
    )
}

fn workflow_manifest(workflow_id: &str) -> String {
    format!(
        "[package]\nname = {:?}\nversion = \"0.1.0\"\nedition = \"2024\"\npublish = false\n\n[dependencies]\nlightflow = {{ workspace = true }}\n",
        package_name_from_id(workflow_id)
    )
}

fn workflow_crate_dir(root: &Path, workflow_id: &str) -> PathBuf {
    root.join("lightflow").join("workflows").join(workflow_id)
}

fn workflow_manifest_path(root: &Path, workflow_id: &str) -> PathBuf {
    workflow_crate_dir(root, workflow_id).join("Cargo.toml")
}

fn workflow_source_path(root: &Path, workflow_id: &str) -> PathBuf {
    workflow_crate_dir(root, workflow_id)
        .join("src")
        .join("lib.rs")
}

fn example_workflow_source(workflow_id: &str, name: &str) -> String {
    format!(
        "use lightflow::workflow::*;\n\npub fn define() -> WorkflowSpec {{\n    workflow({})\n        .version(\"0.1.0\")\n        .name({})\n        .description(\"TODO: describe this workflow.\")\n        .input(\"value\", \"json\")\n        .output(\"value\", \"json\")\n        .build()\n}}\n",
        rust_string(workflow_id),
        rust_string(name)
    )
}

fn validate_spec_id(value: &str, label: &str) -> CliResult<()> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
    {
        return Err(CliError::Usage(format!("invalid {label}: {value}")));
    }
    Ok(())
}

fn normalize_workflow_id(value: &str) -> String {
    let value = value.strip_suffix(".rs").unwrap_or(value);
    if value.starts_with("lightflow.") {
        value.to_owned()
    } else {
        format!("lightflow.{value}")
    }
}

fn write_new_text(path: &Path, body: &str, created: &mut Vec<String>) -> CliResult<()> {
    if path.exists() {
        return Err(CliError::Usage(format!(
            "{} already exists; refusing to overwrite",
            path.display()
        )));
    }
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, body)?;
    created.push(path.to_string_lossy().into_owned());
    Ok(())
}

fn rust_string(value: &str) -> String {
    format!("{value:?}")
}

fn package_name_from_id(id: &str) -> String {
    let mut name = String::new();
    let mut previous_dash = false;
    for character in id.chars() {
        if character.is_ascii_alphanumeric() {
            name.push(character.to_ascii_lowercase());
            previous_dash = false;
        } else if !previous_dash {
            name.push('-');
            previous_dash = true;
        }
    }
    let name = name.trim_matches('-');
    if name.is_empty() {
        "workflow".to_owned()
    } else {
        name.to_owned()
    }
}

fn title_from_id(id: &str) -> String {
    let suffix = id.rsplit('.').next().unwrap_or(id);
    suffix
        .split(['_', '-'])
        .filter(|part| !part.is_empty())
        .map(|part| {
            let mut chars = part.chars();
            match chars.next() {
                Some(first) => format!("{}{}", first.to_ascii_uppercase(), chars.as_str()),
                None => String::new(),
            }
        })
        .collect::<Vec<_>>()
        .join(" ")
}

#[cfg(test)]
mod tests {
    use super::parse_bind_addr;

    #[test]
    fn serve_bind_addr_supports_custom_host_and_port() {
        assert_eq!(parse_bind_addr(&[], "serve").unwrap(), "127.0.0.1:5174");
        assert_eq!(
            parse_bind_addr(
                &[
                    "--host".to_owned(),
                    "0.0.0.0".to_owned(),
                    "--port".to_owned(),
                    "8080".to_owned(),
                ],
                "serve"
            )
            .unwrap(),
            "0.0.0.0:8080"
        );
    }
}
