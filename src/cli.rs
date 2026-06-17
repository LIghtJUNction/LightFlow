use crate::api::{ApiError, ApiService};
use crate::server;
use crate::workflow::{ModelProvider, ModelVariant, WorkflowExecutionOptions, WorkflowSpec};
use serde::Serialize;
use serde_json::json;
use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};
use std::process::Command;
use toml_edit::{DocumentMut, InlineTable, Item, value};

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
        "add-dep" => {
            let options = parse_add_dependency_options(args)?;
            print_json(&add_dependency(Path::new("."), &options)?)?;
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
        "sync" => {
            let options = parse_sync_options(args)?;
            print_json(&sync_project(&service, &options)?)?;
        }
        "run" => {
            let options = parse_run_options(args)?;
            print_json(&service.execute_workflow(&options.workflow_id, options.execution)?)?;
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
        "  lfw add-dep <crate_name> (--path <path>|--git <url>) [--package <package>]",
        "  lfw list [--brief|--detail]",
        "  lfw ls [--brief|--detail]",
        "  lfw workflows list",
        "  lfw workflows get <workflow_id>",
        "  lfw workflows deps <workflow_id>",
        "  lfw workflows validate <json|-|@file>",
        "  lfw workflows save <json|-|@file>",
        "  lfw deps <workflow_id>",
        "  lfw sync [workflow_id] [--model <requirement=variant>] [--apply]",
        "  lfw run <workflow_id> [--input <name=json>] [--disable <node>] [--enable <node>]",
        "  lfw serve [--host <host>] [--port <port>]",
    ]
    .join("\n")
}

/// Run the quick workflow executor from process arguments.
pub async fn run_lfwx_from_env() -> CliResult<()> {
    run_lfwx(env::args().skip(1).collect()).await
}

/// Run the quick workflow executor with explicit arguments.
pub async fn run_lfwx(args: Vec<String>) -> CliResult<()> {
    if args
        .first()
        .is_some_and(|arg| matches!(arg.as_str(), "-h" | "--help" | "help"))
    {
        return Err(CliError::Usage(lfwx_usage()));
    }
    let service = ApiService::new(env::current_dir()?);
    let options = parse_run_options(&args)?;
    print_json(&service.execute_workflow(&options.workflow_id, options.execution)?)?;
    Ok(())
}

fn lfwx_usage() -> String {
    "usage:\n  lfwx <workflow_id> [--input <name=json>] [--disable <node>] [--enable <node>]"
        .to_owned()
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct RunOptions {
    workflow_id: String,
    execution: WorkflowExecutionOptions,
}

fn parse_run_options(args: &[String]) -> CliResult<RunOptions> {
    let workflow_id = required_arg(args, 0, "workflow id")?.to_owned();
    let mut execution = WorkflowExecutionOptions::default();
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--input" | "-i" => {
                let value = required_flag_value(args, index, "--input")?;
                let Some((name, raw_value)) = value.split_once('=') else {
                    return Err(CliError::Usage(
                        "--input must use <name=json-or-text>".to_owned(),
                    ));
                };
                if name.is_empty() {
                    return Err(CliError::Usage("input name cannot be empty".to_owned()));
                }
                execution
                    .inputs
                    .insert(name.to_owned(), parse_input_value(raw_value));
                index += 2;
            }
            "--disable" => {
                execution
                    .disabled_nodes
                    .push(required_flag_value(args, index, "--disable")?.to_owned());
                index += 2;
            }
            "--enable" => {
                execution
                    .enabled_nodes
                    .push(required_flag_value(args, index, "--enable")?.to_owned());
                index += 2;
            }
            value => {
                return Err(CliError::Usage(format!(
                    "unexpected argument for run: {value}"
                )));
            }
        }
    }
    Ok(RunOptions {
        workflow_id,
        execution,
    })
}

fn parse_input_value(value: &str) -> serde_json::Value {
    serde_json::from_str(value).unwrap_or_else(|_| serde_json::Value::String(value.to_owned()))
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct AddDependencyOptions {
    crate_name: String,
    source: DependencySource,
    package: Option<String>,
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum DependencySource {
    Path(String),
    Git(String),
}

fn parse_add_dependency_options(args: &[String]) -> CliResult<AddDependencyOptions> {
    let crate_name = required_arg(args, 0, "crate name")?.to_owned();
    let mut source = None;
    let mut package = None;
    let mut index = 1;
    while index < args.len() {
        match args[index].as_str() {
            "--path" => {
                ensure_single_dependency_source(&source)?;
                source = Some(DependencySource::Path(
                    required_flag_value(args, index, "--path")?.to_owned(),
                ));
                index += 2;
            }
            "--git" => {
                ensure_single_dependency_source(&source)?;
                source = Some(DependencySource::Git(
                    required_flag_value(args, index, "--git")?.to_owned(),
                ));
                index += 2;
            }
            "--package" => {
                if package.is_some() {
                    return Err(CliError::Usage("duplicate flag --package".to_owned()));
                }
                package = Some(required_flag_value(args, index, "--package")?.to_owned());
                index += 2;
            }
            value => {
                return Err(CliError::Usage(format!(
                    "unexpected argument for add-dep: {value}"
                )));
            }
        }
    }
    let source = source.ok_or_else(|| {
        CliError::Usage("add-dep requires either --path <path> or --git <url>".to_owned())
    })?;
    Ok(AddDependencyOptions {
        crate_name,
        source,
        package,
    })
}

fn ensure_single_dependency_source(source: &Option<DependencySource>) -> CliResult<()> {
    if source.is_some() {
        Err(CliError::Usage(
            "add-dep accepts only one dependency source".to_owned(),
        ))
    } else {
        Ok(())
    }
}

fn add_dependency(root: &Path, options: &AddDependencyOptions) -> CliResult<serde_json::Value> {
    let manifest_path = root.join("Cargo.toml");
    if !manifest_path.exists() {
        fs::write(&manifest_path, workspace_manifest())?;
    }
    let source = fs::read_to_string(&manifest_path)?;
    let mut document = source
        .parse::<DocumentMut>()
        .map_err(|error| CliError::Usage(format!("invalid Cargo manifest: {error}")))?;
    ensure_workspace_dependencies_table(&mut document);
    let dependency = dependency_item(options);
    document["workspace"]["dependencies"][&options.crate_name] = dependency;
    fs::write(&manifest_path, document.to_string())?;
    Ok(json!({
        "manifest": manifest_path,
        "dependency": options.crate_name,
        "source": match &options.source {
            DependencySource::Path(path) => json!({ "path": path }),
            DependencySource::Git(git) => json!({ "git": git }),
        },
        "package": options.package,
    }))
}

fn ensure_workspace_dependencies_table(document: &mut DocumentMut) {
    if !document["workspace"].is_table() {
        document["workspace"] = Item::Table(toml_edit::Table::new());
    }
    if !document["workspace"]["dependencies"].is_table() {
        document["workspace"]["dependencies"] = Item::Table(toml_edit::Table::new());
    }
}

fn dependency_item(options: &AddDependencyOptions) -> Item {
    let mut table = InlineTable::new();
    match &options.source {
        DependencySource::Path(path) => {
            table.insert("path", value(path).into_value().unwrap());
        }
        DependencySource::Git(git) => {
            table.insert("git", value(git).into_value().unwrap());
        }
    }
    if let Some(package) = &options.package {
        table.insert("package", value(package).into_value().unwrap());
    }
    Item::Value(toml_edit::Value::InlineTable(table))
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct SyncOptions {
    workflow_id: Option<String>,
    model_selections: BTreeMap<String, String>,
    apply: bool,
}

fn parse_sync_options(args: &[String]) -> CliResult<SyncOptions> {
    let mut workflow_id = None;
    let mut model_selections = BTreeMap::new();
    let mut apply = false;
    let mut index = 0;
    while index < args.len() {
        match args[index].as_str() {
            "--apply" => {
                apply = true;
                index += 1;
            }
            "--dry-run" => {
                apply = false;
                index += 1;
            }
            "--model" => {
                let value = required_flag_value(args, index, "--model")?;
                let Some((requirement, variant)) = value.split_once('=') else {
                    return Err(CliError::Usage(
                        "--model must use <requirement=variant>".to_owned(),
                    ));
                };
                if requirement.is_empty() || variant.is_empty() {
                    return Err(CliError::Usage(
                        "--model must use <requirement=variant>".to_owned(),
                    ));
                }
                model_selections.insert(requirement.to_owned(), variant.to_owned());
                index += 2;
            }
            value if value.starts_with("--") => {
                return Err(CliError::Usage(format!(
                    "unexpected argument for sync: {value}"
                )));
            }
            value => {
                if workflow_id.is_some() {
                    return Err(CliError::Usage(format!(
                        "unexpected argument for sync: {value}"
                    )));
                }
                workflow_id = Some(value.to_owned());
                index += 1;
            }
        }
    }
    Ok(SyncOptions {
        workflow_id,
        model_selections,
        apply,
    })
}

fn sync_project(service: &ApiService, options: &SyncOptions) -> CliResult<serde_json::Value> {
    let workflows = if let Some(workflow_id) = &options.workflow_id {
        let deps = service.workflow_dependencies(workflow_id)?;
        deps.workflows
            .into_iter()
            .map(|workflow_id| service.get_workflow(&workflow_id))
            .collect::<Result<Vec<_>, _>>()?
    } else {
        service
            .list_workflows()?
            .workflows
            .into_iter()
            .map(|summary| service.get_workflow(&summary.id))
            .collect::<Result<Vec<_>, _>>()?
    };
    let model_requirements = workflows
        .iter()
        .flat_map(|workflow| {
            workflow.models.iter().map(|model| {
                json!({
                    "workflow_id": workflow.id,
                    "id": model.id,
                    "capability": model.capability,
                    "variants": model.variants.iter().map(model_variant_json).collect::<Vec<_>>()
                })
            })
        })
        .collect::<Vec<_>>();
    let selected_models = select_model_variants(&workflows, &options.model_selections)?;
    let hf_downloads = selected_models
        .iter()
        .filter(|selection| selection.variant.provider == ModelProvider::HuggingFace)
        .map(|selection| hf_download_plan(selection))
        .collect::<Vec<_>>();
    let unresolved_models = workflows
        .iter()
        .flat_map(|workflow| {
            workflow.models.iter().filter_map(|model| {
                if options.model_selections.contains_key(&model.id) {
                    return None;
                }
                Some(json!({
                    "workflow_id": workflow.id,
                    "id": model.id,
                    "capability": model.capability,
                    "variants": model.variants.iter().map(model_variant_json).collect::<Vec<_>>(),
                    "reason": if model.variants.is_empty() { "no concrete variants declared" } else { "model variant not selected" }
                }))
            })
        })
        .collect::<Vec<_>>();

    let mut executed = Vec::new();
    if options.apply {
        run_status(Command::new("cargo").arg("fetch"))?;
        executed.push(json!({ "command": ["cargo", "fetch"] }));
        for download in &hf_downloads {
            let command = download["command"].as_array().unwrap();
            let mut process = Command::new(command[0].as_str().unwrap());
            for arg in &command[1..] {
                process.arg(arg.as_str().unwrap());
            }
            run_status(&mut process)?;
            executed.push(download.clone());
        }
    }

    Ok(json!({
        "dry_run": !options.apply,
        "workflow_scope": options.workflow_id,
        "module_dependencies": {
            "manager": "cargo",
            "command": ["cargo", "fetch"],
            "note": "Cargo resolves Rust workflow module dependencies."
        },
        "model_requirements": model_requirements,
        "unresolved_models": unresolved_models,
        "hf_downloads": hf_downloads,
        "executed": executed
    }))
}

struct SelectedModel<'a> {
    requirement_id: &'a str,
    variant: &'a ModelVariant,
}

fn select_model_variants<'a>(
    workflows: &'a [WorkflowSpec],
    selections: &BTreeMap<String, String>,
) -> CliResult<Vec<SelectedModel<'a>>> {
    let mut selected = Vec::new();
    for (requirement_id, variant_id) in selections {
        let Some(model) = workflows
            .iter()
            .flat_map(|workflow| workflow.models.iter())
            .find(|model| model.id == *requirement_id)
        else {
            return Err(CliError::Usage(format!(
                "unknown model requirement: {requirement_id}"
            )));
        };
        let Some(variant) = model
            .variants
            .iter()
            .find(|variant| variant.id == *variant_id)
        else {
            return Err(CliError::Usage(format!(
                "unknown variant {variant_id} for model requirement {requirement_id}"
            )));
        };
        selected.push(SelectedModel {
            requirement_id: &model.id,
            variant,
        });
    }
    Ok(selected)
}

fn model_variant_json(variant: &ModelVariant) -> serde_json::Value {
    json!({
        "id": variant.id,
        "provider": variant.provider.as_str(),
        "format": variant.format,
        "repo": variant.repo,
        "file": variant.file,
    })
}

fn hf_download_plan(selection: &SelectedModel<'_>) -> serde_json::Value {
    let mut command = vec![
        "hf".to_owned(),
        "download".to_owned(),
        selection.variant.repo.clone(),
    ];
    if let Some(file) = &selection.variant.file {
        command.push(file.clone());
    }
    json!({
        "requirement_id": selection.requirement_id,
        "variant_id": selection.variant.id,
        "provider": selection.variant.provider.as_str(),
        "format": selection.variant.format,
        "repo": selection.variant.repo,
        "file": selection.variant.file,
        "command": command,
    })
}

fn run_status(command: &mut Command) -> CliResult<()> {
    let status = command.status()?;
    if status.success() {
        Ok(())
    } else {
        Err(CliError::Usage(format!(
            "command failed with status {status}"
        )))
    }
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
