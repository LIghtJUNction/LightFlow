//! Framework-independent LightFlow backend service.

use crate::workflow::{
    CargoDependency, CargoDependencySource, ModelProvider, ModelRequirement, ModelVariant,
    NodeExecution, NodeExecutionStatus, PortSpec, ResolvedWorkflowDependency,
    WorkflowDependencyReport, WorkflowDependencyRequirement, WorkflowEdge, WorkflowEndpoint,
    WorkflowExecution, WorkflowExecutionOptions, WorkflowList, WorkflowNode, WorkflowPosition,
    WorkflowSpec, WorkflowSummary, WorkflowValidation, WorkflowVersionMismatch,
};
use petgraph::algo::toposort;
use petgraph::graph::{DiGraph, NodeIndex};
use semver::Version;
use std::collections::{BTreeMap, BTreeSet};
use std::fmt::{Display, Formatter};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use toml_edit::{DocumentMut, Item};

const WORKFLOW_DIR: &str = "workflows";
const LIGHTFLOW_DIR: &str = "lightflow";

/// Backend service state independent of any web framework.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct ApiService {
    repo_root: PathBuf,
    workflow_paths: Vec<PathBuf>,
}

impl ApiService {
    /// Create a service rooted at a LightFlow repository.
    #[must_use]
    pub fn new(repo_root: impl Into<PathBuf>) -> Self {
        Self {
            repo_root: repo_root.into(),
            workflow_paths: Vec::new(),
        }
    }

    /// Add workflow search paths. Each path can point at a workflow collection,
    /// a LightFlow project root, or one workflow crate.
    #[must_use]
    pub fn with_workflow_paths(mut self, workflow_paths: Vec<PathBuf>) -> Self {
        self.workflow_paths = workflow_paths;
        self
    }

    /// Repository root used for project file discovery.
    #[must_use]
    pub fn repo_root(&self) -> &Path {
        &self.repo_root
    }

    /// List workflow specs.
    pub fn list_workflows(&self) -> ApiResult<WorkflowList> {
        let workflows = self
            .workflow_specs()?
            .into_values()
            .map(WorkflowSummary::from)
            .collect();
        Ok(WorkflowList { workflows })
    }

    /// Read one workflow spec.
    pub fn get_workflow(&self, workflow_id: &str) -> ApiResult<WorkflowSpec> {
        self.workflow_specs()?
            .remove(workflow_id)
            .ok_or_else(|| ApiError::NotFound(format!("workflow {workflow_id}")))
    }

    /// Save one workflow spec under `lightflow/workflows/<id>/src/lib.rs`.
    pub fn save_workflow(&self, workflow: WorkflowSpec) -> ApiResult<WorkflowSpec> {
        let validation = self.validate_workflow(&workflow);
        if !validation.valid {
            return Err(ApiError::InvalidRequest(validation.issues.join("; ")));
        }
        let path = self.workflow_path(&workflow.id)?;
        write_text_atomic(&path, &workflow_source(&workflow))?;
        Ok(workflow)
    }

    /// Validate a workflow against current workflow specs.
    pub fn validate_workflow(&self, workflow: &WorkflowSpec) -> WorkflowValidation {
        let mut workflows = self.workflow_specs().unwrap_or_default();
        workflows.insert(workflow.id.clone(), workflow.clone());
        let mut validation = validate_workflow_spec(workflow, &workflows);
        let dependencies = dependency_report(&workflow.id, &workflows);
        for cycle in dependencies.cycles {
            validation
                .issues
                .push(format!("workflow dependency cycle: {}", cycle.join(" -> ")));
        }
        for mismatch in dependencies.version_mismatches {
            validation.issues.push(format!(
                "workflow {} requires version {} but found {}",
                mismatch.workflow_id, mismatch.required, mismatch.found
            ));
        }
        validation.valid = validation.issues.is_empty();
        validation
    }

    /// Resolve the recursive workflow dependencies for a workflow.
    pub fn workflow_dependencies(&self, workflow_id: &str) -> ApiResult<WorkflowDependencyReport> {
        let workflows = self.workflow_specs()?;
        if !workflows.contains_key(workflow_id) {
            return Err(ApiError::NotFound(format!("workflow {workflow_id}")));
        }
        Ok(dependency_report(workflow_id, &workflows))
    }

    /// Execute a workflow using the current lightweight graph runner.
    pub fn execute_workflow(
        &self,
        workflow_id: &str,
        options: WorkflowExecutionOptions,
    ) -> ApiResult<WorkflowExecution> {
        let workflows = self.workflow_specs()?;
        let workflow = workflows
            .get(workflow_id)
            .ok_or_else(|| ApiError::NotFound(format!("workflow {workflow_id}")))?;
        execute_workflow_spec(workflow, &workflows, options)
    }

    fn workflow_specs(&self) -> ApiResult<BTreeMap<String, WorkflowSpec>> {
        let mut workflows = BTreeMap::new();
        for workflow in read_workflow_sources(&self.repo_root, &self.workflow_paths)? {
            validate_workflow_shape(&workflow)?;
            workflows.insert(workflow.id.clone(), workflow);
        }
        Ok(workflows)
    }

    fn workflow_path(&self, workflow_id: &str) -> ApiResult<PathBuf> {
        validate_id_segment(workflow_id, "workflow id")?;
        Ok(self
            .repo_root
            .join(LIGHTFLOW_DIR)
            .join(WORKFLOW_DIR)
            .join(workflow_id)
            .join("src")
            .join("lib.rs"))
    }
}

/// API-level error.
#[derive(Debug)]
pub enum ApiError {
    InvalidRequest(String),
    NotFound(String),
    Io(io::Error),
}

impl ApiError {
    /// HTTP-style status code for adapters.
    #[must_use]
    pub const fn status_code(&self) -> u16 {
        match self {
            Self::InvalidRequest(_) => 400,
            Self::NotFound(_) => 404,
            Self::Io(_) => 500,
        }
    }
}

impl Display for ApiError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidRequest(message) => write!(f, "invalid request: {message}"),
            Self::NotFound(message) => write!(f, "not found: {message}"),
            Self::Io(error) => Display::fmt(error, f),
        }
    }
}

impl std::error::Error for ApiError {}

impl From<io::Error> for ApiError {
    fn from(error: io::Error) -> Self {
        if error.kind() == io::ErrorKind::NotFound {
            Self::NotFound(error.to_string())
        } else {
            Self::Io(error)
        }
    }
}

/// Service result.
pub type ApiResult<T> = Result<T, ApiError>;

fn read_workflow_sources(root: &Path, workflow_paths: &[PathBuf]) -> ApiResult<Vec<WorkflowSpec>> {
    let mut workflows = Vec::new();
    let mut manifests = BTreeSet::new();
    let mut visited_libs = BTreeSet::new();
    read_workflow_collection(
        &root.join(LIGHTFLOW_DIR).join(WORKFLOW_DIR),
        true,
        &mut workflows,
        &mut manifests,
        &mut visited_libs,
    )?;

    for path in workflow_paths {
        read_workflow_search_path(path, &mut workflows, &mut manifests, &mut visited_libs)?;
    }

    let root_manifest = root.join("Cargo.toml");
    if root_manifest.exists() {
        manifests.insert(normalize_existing_path(&root_manifest)?);
    }
    read_path_dependency_workflows(&mut workflows, &mut manifests, &mut visited_libs)?;

    Ok(workflows)
}

fn read_workflow_search_path(
    path: &Path,
    workflows: &mut Vec<WorkflowSpec>,
    manifests: &mut BTreeSet<PathBuf>,
    visited_libs: &mut BTreeSet<PathBuf>,
) -> ApiResult<()> {
    if !path.exists() {
        return Ok(());
    }
    if path.join(LIGHTFLOW_DIR).join(WORKFLOW_DIR).is_dir() {
        return read_workflow_collection(
            &path.join(LIGHTFLOW_DIR).join(WORKFLOW_DIR),
            false,
            workflows,
            manifests,
            visited_libs,
        );
    }
    if path.join(WORKFLOW_DIR).is_dir() {
        return read_workflow_collection(
            &path.join(WORKFLOW_DIR),
            false,
            workflows,
            manifests,
            visited_libs,
        );
    }
    if path.join("src").join("lib.rs").exists() {
        return read_one_workflow_crate(path, false, workflows, manifests, visited_libs);
    }
    read_workflow_collection(path, false, workflows, manifests, visited_libs)
}

fn read_workflow_collection(
    collection: &Path,
    strict: bool,
    workflows: &mut Vec<WorkflowSpec>,
    manifests: &mut BTreeSet<PathBuf>,
    visited_libs: &mut BTreeSet<PathBuf>,
) -> ApiResult<()> {
    match fs::read_dir(collection) {
        Ok(entries) => {
            for entry in entries {
                let path = entry.map_err(ApiError::from)?.path();
                if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                    if path.is_dir() {
                        read_one_workflow_crate(&path, strict, workflows, manifests, visited_libs)?;
                    }
                    continue;
                }
                let lib = normalize_existing_path(&path)?;
                if !visited_libs.insert(lib.clone()) {
                    continue;
                }
                workflows.push(read_workflow_source(&lib)?);
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(ApiError::from(error)),
    }
    Ok(())
}

fn read_one_workflow_crate(
    crate_dir: &Path,
    strict: bool,
    workflows: &mut Vec<WorkflowSpec>,
    manifests: &mut BTreeSet<PathBuf>,
    visited_libs: &mut BTreeSet<PathBuf>,
) -> ApiResult<()> {
    let lib = crate_dir.join("src").join("lib.rs");
    if !lib.exists() {
        return Ok(());
    }
    let lib = normalize_existing_path(&lib)?;
    if !visited_libs.insert(lib.clone()) {
        return Ok(());
    }
    let workflow = if strict {
        Some(read_workflow_source(&lib)?)
    } else {
        read_optional_workflow_source(&lib)?
    };
    if let Some(workflow) = workflow {
        workflows.push(workflow);
        let manifest = crate_dir.join("Cargo.toml");
        if manifest.exists() {
            manifests.insert(normalize_existing_path(&manifest)?);
        }
    }
    Ok(())
}

fn read_path_dependency_workflows(
    workflows: &mut Vec<WorkflowSpec>,
    manifests: &mut BTreeSet<PathBuf>,
    visited_libs: &mut BTreeSet<PathBuf>,
) -> ApiResult<()> {
    let mut scanned = BTreeSet::new();
    while let Some(manifest) = manifests
        .iter()
        .find(|manifest| !scanned.contains(*manifest))
        .cloned()
    {
        scanned.insert(manifest.clone());
        for dependency_dir in cargo_path_dependencies(&manifest)? {
            let manifest = dependency_dir.join("Cargo.toml");
            if manifest.exists() {
                manifests.insert(normalize_existing_path(&manifest)?);
            }

            let lib = dependency_dir.join("src").join("lib.rs");
            if !lib.exists() {
                continue;
            }
            let lib = normalize_existing_path(&lib)?;
            if !visited_libs.insert(lib.clone()) {
                continue;
            }
            if let Some(workflow) = read_optional_workflow_source(&lib)? {
                workflows.push(workflow);
            }
        }
    }
    Ok(())
}

fn cargo_path_dependencies(manifest: &Path) -> ApiResult<Vec<PathBuf>> {
    let manifest_dir = manifest.parent().ok_or_else(|| {
        ApiError::InvalidRequest(format!("manifest {:?} has no parent", manifest))
    })?;
    let source = fs::read_to_string(manifest).map_err(ApiError::from)?;
    let document = source.parse::<DocumentMut>().map_err(|error| {
        ApiError::InvalidRequest(format!("invalid Cargo manifest {:?}: {error}", manifest))
    })?;
    let mut paths = Vec::new();
    collect_dependency_paths(manifest_dir, document.get("dependencies"), &mut paths)?;
    collect_dependency_paths(
        manifest_dir,
        document
            .get("workspace")
            .and_then(|workspace| workspace.get("dependencies")),
        &mut paths,
    )?;
    Ok(paths)
}

fn collect_dependency_paths(
    manifest_dir: &Path,
    dependencies: Option<&Item>,
    paths: &mut Vec<PathBuf>,
) -> ApiResult<()> {
    let Some(dependencies) = dependencies.and_then(Item::as_table_like) else {
        return Ok(());
    };
    for (_name, dependency) in dependencies.iter() {
        let Some(path) = dependency.get("path").and_then(Item::as_str) else {
            continue;
        };
        let path = manifest_dir.join(path);
        if path.exists() {
            paths.push(normalize_existing_path(&path)?);
        }
    }
    Ok(())
}

fn read_optional_workflow_source(path: &Path) -> ApiResult<Option<WorkflowSpec>> {
    let source = fs::read_to_string(path).map_err(ApiError::from)?;
    let file = syn::parse_file(&source).map_err(|error| {
        ApiError::InvalidRequest(format!(
            "invalid Rust workflow source in {:?}: {error}",
            path
        ))
    })?;
    let Some(define) = file.items.iter().find_map(|item| match item {
        syn::Item::Fn(function) if function.sig.ident == "define" => Some(function),
        _ => None,
    }) else {
        return Ok(None);
    };
    let expression = define_expression(define).ok_or_else(|| {
        ApiError::InvalidRequest(format!(
            "workflow source {:?} must return a workflow(...) builder expression",
            path
        ))
    })?;
    parse_workflow_builder(expression, path).map(Some)
}

fn normalize_existing_path(path: &Path) -> ApiResult<PathBuf> {
    path.canonicalize().map_err(ApiError::from)
}

fn read_workflow_source(path: &Path) -> ApiResult<WorkflowSpec> {
    let source = fs::read_to_string(path).map_err(ApiError::from)?;
    let file = syn::parse_file(&source).map_err(|error| {
        ApiError::InvalidRequest(format!(
            "invalid Rust workflow source in {:?}: {error}",
            path
        ))
    })?;
    let define = file
        .items
        .iter()
        .find_map(|item| match item {
            syn::Item::Fn(function) if function.sig.ident == "define" => Some(function),
            _ => None,
        })
        .ok_or_else(|| {
            ApiError::InvalidRequest(format!(
                "workflow source {:?} must define pub fn define() -> WorkflowSpec",
                path
            ))
        })?;
    let expression = define_expression(define).ok_or_else(|| {
        ApiError::InvalidRequest(format!(
            "workflow source {:?} must return a workflow(...) builder expression",
            path
        ))
    })?;
    parse_workflow_builder(expression, path)
}

fn define_expression(function: &syn::ItemFn) -> Option<&syn::Expr> {
    function
        .block
        .stmts
        .iter()
        .rev()
        .find_map(|statement| match statement {
            syn::Stmt::Expr(syn::Expr::Return(return_expr), _) => return_expr.expr.as_deref(),
            syn::Stmt::Expr(expression, _) => Some(expression),
            _ => None,
        })
}

fn parse_workflow_builder(expression: &syn::Expr, path: &Path) -> ApiResult<WorkflowSpec> {
    match expression {
        syn::Expr::MethodCall(call) => {
            let mut workflow = parse_workflow_builder(&call.receiver, path)?;
            let method = call.method.to_string();
            match method.as_str() {
                "build" => expect_arg_len(&call.args, 0, &method, path)?,
                "version" => {
                    workflow.version = string_arg(&call.args, 0, &method, path)?;
                    expect_arg_len(&call.args, 1, &method, path)?;
                }
                "name" => {
                    workflow.name = string_arg(&call.args, 0, &method, path)?;
                    expect_arg_len(&call.args, 1, &method, path)?;
                }
                "description" => {
                    workflow.description = Some(string_arg(&call.args, 0, &method, path)?);
                    expect_arg_len(&call.args, 1, &method, path)?;
                }
                "input" => {
                    workflow.inputs.push(PortSpec {
                        name: string_arg(&call.args, 0, &method, path)?,
                        ty: string_arg(&call.args, 1, &method, path)?,
                    });
                    expect_arg_len(&call.args, 2, &method, path)?;
                }
                "output" => {
                    workflow.outputs.push(PortSpec {
                        name: string_arg(&call.args, 0, &method, path)?,
                        ty: string_arg(&call.args, 1, &method, path)?,
                    });
                    expect_arg_len(&call.args, 2, &method, path)?;
                }
                "depends_on" => {
                    workflow.dependencies.push(WorkflowDependencyRequirement {
                        workflow_id: string_arg(&call.args, 0, &method, path)?,
                        version: Some(string_arg(&call.args, 1, &method, path)?),
                        install: None,
                    });
                    expect_arg_len(&call.args, 2, &method, path)?;
                }
                "depends_on_crate" => {
                    let workflow_id = string_arg(&call.args, 0, &method, path)?;
                    let version = string_arg(&call.args, 1, &method, path)?;
                    let crate_name = string_arg(&call.args, 2, &method, path)?;
                    workflow.dependencies.push(WorkflowDependencyRequirement {
                        workflow_id,
                        version: Some(version.clone()),
                        install: Some(CargoDependency {
                            crate_name,
                            version: Some(version),
                            source: None,
                            package: None,
                        }),
                    });
                    expect_arg_len(&call.args, 3, &method, path)?;
                }
                "depends_on_path" => {
                    let workflow_id = string_arg(&call.args, 0, &method, path)?;
                    let version = string_arg(&call.args, 1, &method, path)?;
                    let crate_name = string_arg(&call.args, 2, &method, path)?;
                    let dependency_path = string_arg(&call.args, 3, &method, path)?;
                    workflow.dependencies.push(WorkflowDependencyRequirement {
                        workflow_id,
                        version: Some(version.clone()),
                        install: Some(CargoDependency {
                            crate_name,
                            version: Some(version),
                            source: Some(CargoDependencySource::Path(dependency_path)),
                            package: None,
                        }),
                    });
                    expect_arg_len(&call.args, 4, &method, path)?;
                }
                "depends_on_git" => {
                    let workflow_id = string_arg(&call.args, 0, &method, path)?;
                    let version = string_arg(&call.args, 1, &method, path)?;
                    let crate_name = string_arg(&call.args, 2, &method, path)?;
                    let git = string_arg(&call.args, 3, &method, path)?;
                    let package = string_arg(&call.args, 4, &method, path)?;
                    workflow.dependencies.push(WorkflowDependencyRequirement {
                        workflow_id,
                        version: Some(version.clone()),
                        install: Some(CargoDependency {
                            crate_name,
                            version: Some(version),
                            source: Some(CargoDependencySource::Git(git)),
                            package: Some(package).filter(|package| !package.is_empty()),
                        }),
                    });
                    expect_arg_len(&call.args, 5, &method, path)?;
                }
                "model" => {
                    workflow.models.push(ModelRequirement {
                        id: string_arg(&call.args, 0, &method, path)?,
                        capability: string_arg(&call.args, 1, &method, path)?,
                        variants: Vec::new(),
                    });
                    expect_arg_len(&call.args, 2, &method, path)?;
                }
                "hf_model" => {
                    push_hf_model_variant(
                        &mut workflow,
                        string_arg(&call.args, 0, &method, path)?,
                        string_arg(&call.args, 1, &method, path)?,
                        string_arg(&call.args, 2, &method, path)?,
                        string_arg(&call.args, 3, &method, path)?,
                        string_arg(&call.args, 4, &method, path)?,
                        string_arg(&call.args, 5, &method, path)?,
                    );
                    expect_arg_len(&call.args, 6, &method, path)?;
                }
                "node" => {
                    workflow.nodes.push(WorkflowNode {
                        id: string_arg(&call.args, 0, &method, path)?,
                        workflow_id: string_arg(&call.args, 1, &method, path)?,
                        title: None,
                        disabled: false,
                        position: WorkflowPosition::default(),
                        config: serde_json::Value::Null,
                    });
                    expect_arg_len(&call.args, 2, &method, path)?;
                }
                "disabled_node" => {
                    workflow.nodes.push(WorkflowNode {
                        id: string_arg(&call.args, 0, &method, path)?,
                        workflow_id: string_arg(&call.args, 1, &method, path)?,
                        title: None,
                        disabled: true,
                        position: WorkflowPosition::default(),
                        config: serde_json::Value::Null,
                    });
                    expect_arg_len(&call.args, 2, &method, path)?;
                }
                "edge" => {
                    workflow.edges.push(WorkflowEdge {
                        from: WorkflowEndpoint {
                            node: string_arg(&call.args, 0, &method, path)?,
                            port: string_arg(&call.args, 1, &method, path)?,
                        },
                        to: WorkflowEndpoint {
                            node: string_arg(&call.args, 2, &method, path)?,
                            port: string_arg(&call.args, 3, &method, path)?,
                        },
                    });
                    expect_arg_len(&call.args, 4, &method, path)?;
                }
                _ => {
                    return Err(ApiError::InvalidRequest(format!(
                        "unsupported workflow builder method .{method}(...) in {:?}",
                        path
                    )));
                }
            }
            Ok(workflow)
        }
        syn::Expr::Call(call) if is_workflow_constructor(call) => {
            expect_arg_len(&call.args, 1, "workflow", path)?;
            Ok(WorkflowSpec {
                id: string_arg(&call.args, 0, "workflow", path)?,
                version: "0.1.0".to_owned(),
                name: String::new(),
                description: None,
                inputs: Vec::new(),
                outputs: Vec::new(),
                config_schema: serde_json::Value::Null,
                dependencies: Vec::new(),
                models: Vec::new(),
                nodes: Vec::new(),
                edges: Vec::new(),
            })
        }
        _ => Err(ApiError::InvalidRequest(format!(
            "unsupported workflow definition expression in {:?}",
            path
        ))),
    }
}

fn push_hf_model_variant(
    workflow: &mut WorkflowSpec,
    requirement_id: String,
    variant_id: String,
    capability: String,
    format: String,
    repo: String,
    file: String,
) {
    let variant = ModelVariant {
        id: variant_id,
        provider: ModelProvider::HuggingFace,
        format,
        repo,
        file: Some(file).filter(|file| !file.is_empty()),
    };
    if let Some(requirement) = workflow
        .models
        .iter_mut()
        .find(|requirement| requirement.id == requirement_id)
    {
        requirement.variants.push(variant);
    } else {
        workflow.models.push(ModelRequirement {
            id: requirement_id,
            capability,
            variants: vec![variant],
        });
    }
}

fn is_workflow_constructor(call: &syn::ExprCall) -> bool {
    match call.func.as_ref() {
        syn::Expr::Path(path) => path
            .path
            .segments
            .last()
            .is_some_and(|segment| segment.ident == "workflow"),
        _ => false,
    }
}

fn string_arg(
    args: &syn::punctuated::Punctuated<syn::Expr, syn::token::Comma>,
    index: usize,
    method: &str,
    path: &Path,
) -> ApiResult<String> {
    let Some(argument) = args.iter().nth(index) else {
        return Err(ApiError::InvalidRequest(format!(
            "workflow builder .{method}(...) in {:?} is missing argument {}",
            path,
            index + 1
        )));
    };
    match argument {
        syn::Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Str(value),
            ..
        }) => Ok(value.value()),
        _ => Err(ApiError::InvalidRequest(format!(
            "workflow builder .{method}(...) argument {} in {:?} must be a string literal",
            index + 1,
            path
        ))),
    }
}

fn expect_arg_len(
    args: &syn::punctuated::Punctuated<syn::Expr, syn::token::Comma>,
    expected: usize,
    method: &str,
    path: &Path,
) -> ApiResult<()> {
    if args.len() == expected {
        Ok(())
    } else {
        Err(ApiError::InvalidRequest(format!(
            "workflow builder .{method}(...) in {:?} expects {expected} arguments, got {}",
            path,
            args.len()
        )))
    }
}

fn validate_workflow_shape(workflow: &WorkflowSpec) -> ApiResult<()> {
    let mut issues = Vec::new();
    push_id_issue(&mut issues, &workflow.id, "workflow id");
    if workflow.version.trim().is_empty() {
        issues.push(format!("workflow {} must have a version", workflow.id));
    } else if Version::parse(&workflow.version).is_err() {
        issues.push(format!(
            "workflow {} version {} must be semantic version",
            workflow.id, workflow.version
        ));
    }
    if workflow.name.trim().is_empty() {
        issues.push(format!("workflow {} must have a name", workflow.id));
    }
    push_duplicate_port_issues(
        &mut issues,
        "workflow input",
        &workflow.id,
        &workflow.inputs,
    );
    push_duplicate_port_issues(
        &mut issues,
        "workflow output",
        &workflow.id,
        &workflow.outputs,
    );
    if issues.is_empty() {
        Ok(())
    } else {
        Err(ApiError::InvalidRequest(issues.join("; ")))
    }
}

fn validate_workflow_spec(
    workflow: &WorkflowSpec,
    workflows: &BTreeMap<String, WorkflowSpec>,
) -> WorkflowValidation {
    let mut issues = match validate_workflow_shape(workflow) {
        Ok(()) => Vec::new(),
        Err(ApiError::InvalidRequest(message)) => vec![message],
        Err(error) => vec![error.to_string()],
    };

    for dependency in &workflow.dependencies {
        if let Some(version) = &dependency.version
            && !is_supported_version_requirement(version)
        {
            issues.push(format!(
                "workflow {} declares unsupported version requirement {} for {}",
                workflow.id, version, dependency.workflow_id
            ));
        }
        if !workflows.contains_key(&dependency.workflow_id) {
            issues.push(format!(
                "workflow {} declares missing dependency {}",
                workflow.id, dependency.workflow_id
            ));
        }
    }

    let mut nodes = BTreeMap::new();
    for node in &workflow.nodes {
        push_id_issue(&mut issues, &node.id, "node id");
        if nodes.insert(node.id.as_str(), node).is_some() {
            issues.push(format!("duplicate node id {}", node.id));
        }
        if node.workflow_id == workflow.id {
            issues.push(format!(
                "workflow {} cannot directly nest itself",
                workflow.id
            ));
        } else if !workflows.contains_key(&node.workflow_id) {
            issues.push(format!(
                "node {} references missing workflow {}",
                node.id, node.workflow_id
            ));
        }
    }

    let mut graph = DiGraph::<&str, ()>::new();
    let mut graph_nodes = BTreeMap::<&str, NodeIndex>::new();
    for node in &workflow.nodes {
        graph_nodes
            .entry(node.id.as_str())
            .or_insert_with(|| graph.add_node(node.id.as_str()));
    }

    for edge in &workflow.edges {
        let Some(from_node) = nodes.get(edge.from.node.as_str()) else {
            issues.push(format!(
                "edge references missing source node {}",
                edge.from.node
            ));
            continue;
        };
        if !node_outputs(from_node, workflows)
            .iter()
            .any(|port| port.name == edge.from.port)
        {
            issues.push(format!(
                "edge source {}.{} is not an output port",
                edge.from.node, edge.from.port
            ));
        }
        let Some(to_node) = nodes.get(edge.to.node.as_str()) else {
            issues.push(format!(
                "edge references missing target node {}",
                edge.to.node
            ));
            continue;
        };
        if !node_inputs(to_node, workflows)
            .iter()
            .any(|port| port.name == edge.to.port)
        {
            issues.push(format!(
                "edge target {}.{} is not an input port",
                edge.to.node, edge.to.port
            ));
        }
        if let (Some(from), Some(to)) = (
            graph_nodes.get(edge.from.node.as_str()),
            graph_nodes.get(edge.to.node.as_str()),
        ) {
            graph.add_edge(*from, *to, ());
        }
    }

    let topological_order = match toposort(&graph, None) {
        Ok(order) => order
            .into_iter()
            .filter_map(|node| graph.node_weight(node).copied())
            .map(ToOwned::to_owned)
            .collect(),
        Err(cycle) => {
            let node = graph
                .node_weight(cycle.node_id())
                .copied()
                .unwrap_or("unknown");
            issues.push(format!(
                "workflow {} contains a cycle involving node {node}",
                workflow.id
            ));
            Vec::new()
        }
    };

    WorkflowValidation {
        valid: issues.is_empty(),
        issues,
        topological_order,
    }
}

fn execute_workflow_spec(
    workflow: &WorkflowSpec,
    workflows: &BTreeMap<String, WorkflowSpec>,
    options: WorkflowExecutionOptions,
) -> ApiResult<WorkflowExecution> {
    let validation = validate_workflow_spec(workflow, workflows);
    if !validation.valid {
        return Err(ApiError::InvalidRequest(validation.issues.join("; ")));
    }

    let disabled_nodes = options.disabled_nodes.into_iter().collect::<BTreeSet<_>>();
    let enabled_nodes = options.enabled_nodes.into_iter().collect::<BTreeSet<_>>();
    let nodes_by_id = workflow
        .nodes
        .iter()
        .map(|node| (node.id.as_str(), node))
        .collect::<BTreeMap<_, _>>();
    let mut node_outputs = BTreeMap::<String, serde_json::Map<String, serde_json::Value>>::new();
    let mut executions = Vec::new();

    for node_id in validation.topological_order {
        let Some(node) = nodes_by_id.get(node_id.as_str()) else {
            continue;
        };
        let node_inputs =
            collect_node_inputs(node, workflow, workflows, &options.inputs, &node_outputs);
        let is_disabled = (node.disabled || disabled_nodes.contains(&node.id))
            && !enabled_nodes.contains(&node.id);
        if is_disabled {
            executions.push(NodeExecution {
                node_id: node.id.clone(),
                workflow_id: node.workflow_id.clone(),
                status: NodeExecutionStatus::Skipped,
                inputs: node_inputs,
                outputs: serde_json::Map::new(),
            });
            continue;
        }

        let outputs = execute_passthrough_node(node, workflows, &node_inputs);
        node_outputs.insert(node.id.clone(), outputs.clone());
        executions.push(NodeExecution {
            node_id: node.id.clone(),
            workflow_id: node.workflow_id.clone(),
            status: NodeExecutionStatus::Completed,
            inputs: node_inputs,
            outputs,
        });
    }

    let outputs = collect_workflow_outputs(workflow, &options.inputs, &node_outputs);
    Ok(WorkflowExecution {
        workflow_id: workflow.id.clone(),
        version: workflow.version.clone(),
        inputs: options.inputs,
        outputs,
        nodes: executions,
    })
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

fn execute_passthrough_node(
    node: &WorkflowNode,
    workflows: &BTreeMap<String, WorkflowSpec>,
    inputs: &serde_json::Map<String, serde_json::Value>,
) -> serde_json::Map<String, serde_json::Value> {
    let output_ports = node_outputs(node, workflows);
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
        outputs.insert(output.name, value);
    }
    outputs
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

fn dependency_report(
    workflow_id: &str,
    workflows: &BTreeMap<String, WorkflowSpec>,
) -> WorkflowDependencyReport {
    let mut collector = DependencyCollector {
        workflows,
        states: BTreeMap::new(),
        stack: Vec::new(),
        workflow_ids: BTreeSet::new(),
        resolved: BTreeMap::new(),
        workflow_order: Vec::new(),
        missing_workflows: BTreeSet::new(),
        version_mismatches: Vec::new(),
        cycles: Vec::new(),
    };
    collector.visit_workflow(workflow_id);
    let missing_workflows = collector.missing_workflows.into_iter().collect::<Vec<_>>();
    let complete = missing_workflows.is_empty()
        && collector.version_mismatches.is_empty()
        && collector.cycles.is_empty();
    WorkflowDependencyReport {
        workflow_id: workflow_id.to_owned(),
        complete,
        workflows: collector.workflow_ids.into_iter().collect(),
        resolved: collector.resolved.into_values().collect(),
        workflow_order: collector.workflow_order,
        missing_workflows,
        version_mismatches: collector.version_mismatches,
        cycles: collector.cycles,
    }
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum VisitState {
    Visiting,
    Visited,
}

struct DependencyCollector<'a> {
    workflows: &'a BTreeMap<String, WorkflowSpec>,
    states: BTreeMap<String, VisitState>,
    stack: Vec<String>,
    workflow_ids: BTreeSet<String>,
    resolved: BTreeMap<String, ResolvedWorkflowDependency>,
    workflow_order: Vec<String>,
    missing_workflows: BTreeSet<String>,
    version_mismatches: Vec<WorkflowVersionMismatch>,
    cycles: Vec<Vec<String>>,
}

impl DependencyCollector<'_> {
    fn visit_workflow(&mut self, workflow_id: &str) {
        match self.states.get(workflow_id).copied() {
            Some(VisitState::Visited) => return,
            Some(VisitState::Visiting) => {
                if let Some(index) = self.stack.iter().position(|id| id == workflow_id) {
                    let mut cycle = self.stack[index..].to_vec();
                    cycle.push(workflow_id.to_owned());
                    if !self.cycles.contains(&cycle) {
                        self.cycles.push(cycle);
                    }
                }
                return;
            }
            None => {}
        }

        let Some(workflow) = self.workflows.get(workflow_id) else {
            self.missing_workflows.insert(workflow_id.to_owned());
            return;
        };

        self.workflow_ids.insert(workflow_id.to_owned());
        self.resolved.insert(
            workflow_id.to_owned(),
            ResolvedWorkflowDependency {
                workflow_id: workflow_id.to_owned(),
                version: workflow.version.clone(),
            },
        );
        self.states
            .insert(workflow_id.to_owned(), VisitState::Visiting);
        self.stack.push(workflow_id.to_owned());

        for dependency in &workflow.dependencies {
            self.record_workflow_requirement(
                &dependency.workflow_id,
                dependency.version.as_deref(),
                workflow_id,
            );
            self.visit_workflow(&dependency.workflow_id);
        }

        for node in &workflow.nodes {
            self.record_workflow_requirement(&node.workflow_id, None, workflow_id);
            self.visit_workflow(&node.workflow_id);
        }

        self.stack.pop();
        self.states
            .insert(workflow_id.to_owned(), VisitState::Visited);
        if !self
            .workflow_order
            .iter()
            .any(|ordered| ordered == workflow_id)
        {
            self.workflow_order.push(workflow_id.to_owned());
        }
    }

    fn record_workflow_requirement(
        &mut self,
        workflow_id: &str,
        required: Option<&str>,
        required_by: &str,
    ) {
        let Some(workflow) = self.workflows.get(workflow_id) else {
            self.missing_workflows.insert(workflow_id.to_owned());
            return;
        };
        if let Some(required) = required
            && !version_satisfies(&workflow.version, required)
        {
            self.version_mismatches.push(WorkflowVersionMismatch {
                workflow_id: workflow_id.to_owned(),
                required: required.to_owned(),
                found: workflow.version.clone(),
                required_by: required_by.to_owned(),
            });
        }
    }
}

fn version_satisfies(found: &str, required: &str) -> bool {
    if required == "*" {
        return true;
    }
    let Ok(found) = Version::parse(found) else {
        return false;
    };
    let Ok(required) = Version::parse(required) else {
        return false;
    };
    found == required
}

fn is_supported_version_requirement(required: &str) -> bool {
    required == "*" || Version::parse(required).is_ok()
}

fn node_inputs(
    node: &crate::workflow::WorkflowNode,
    workflows: &BTreeMap<String, WorkflowSpec>,
) -> Vec<PortSpec> {
    workflows
        .get(&node.workflow_id)
        .map(|workflow| workflow.inputs.clone())
        .unwrap_or_default()
}

fn node_outputs(
    node: &crate::workflow::WorkflowNode,
    workflows: &BTreeMap<String, WorkflowSpec>,
) -> Vec<PortSpec> {
    workflows
        .get(&node.workflow_id)
        .map(|workflow| workflow.outputs.clone())
        .unwrap_or_default()
}

fn push_id_issue(issues: &mut Vec<String>, value: &str, label: &str) {
    if let Err(error) = validate_id_segment(value, label) {
        issues.push(error.to_string());
    }
}

fn push_duplicate_port_issues(
    issues: &mut Vec<String>,
    direction: &str,
    owner_id: &str,
    ports: &[PortSpec],
) {
    let mut names = BTreeSet::new();
    for port in ports {
        if port.name.trim().is_empty() {
            issues.push(format!("{owner_id} has an empty {direction} port name"));
        }
        if port.ty.trim().is_empty() {
            issues.push(format!("{owner_id} port {} has an empty type", port.name));
        }
        if !names.insert(port.name.as_str()) {
            issues.push(format!(
                "{owner_id} has duplicate {direction} port {}",
                port.name
            ));
        }
    }
}

fn validate_id_segment(value: &str, label: &str) -> ApiResult<()> {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains('/')
        || value.contains('\\')
    {
        return Err(ApiError::InvalidRequest(format!(
            "invalid {label} path segment: {value}"
        )));
    }
    Ok(())
}

fn write_text_atomic(path: &Path, body: &str) -> ApiResult<()> {
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

fn workflow_source(workflow: &WorkflowSpec) -> String {
    let mut source = String::from("use lightflow::workflow::*;\n\n");
    source.push_str("pub fn define() -> WorkflowSpec {\n");
    source.push_str(&format!("    workflow({})\n", rust_string(&workflow.id)));
    source.push_str(&format!(
        "        .version({})\n",
        rust_string(&workflow.version)
    ));
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
    }
    for output in &workflow.outputs {
        source.push_str(&format!(
            "        .output({}, {})\n",
            rust_string(&output.name),
            rust_string(&output.ty)
        ));
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
