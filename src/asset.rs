//! Self-contained Rust asset discovery.
//!
//! LightFlow assets are Rust files with metadata and executable definition in
//! the same file. This module scans those files and extracts a deliberately
//! simple `META` constant shape.

use serde::{Deserialize, Serialize};
use std::fmt::{Display, Formatter};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use syn::{
    Expr, ExprCall, ExprField, ExprLit, ExprMethodCall, ExprPath, ExprStruct, FieldValue, File,
    Item, ItemConst, ItemFn, Lit, Member, PathSegment, Stmt,
};

/// Project-owned asset root.
pub const PROJECT_ASSET_ROOT: &str = "lightflow";

/// Engine-owned built-in asset root.
pub const BUILTIN_ASSET_ROOT: &str = "src/builtins";

/// Metadata constant expected in every asset file.
pub const META_CONST: &str = "META";

/// Asset category.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetKind {
    Model,
    Node,
    Composition,
    Workflow,
}

impl AssetKind {
    /// Project asset directory for this kind.
    #[must_use]
    pub const fn project_dir(self) -> &'static str {
        match self {
            Self::Model => "models",
            Self::Node => "nodes",
            Self::Composition => "compositions",
            Self::Workflow => "workflows",
        }
    }

    /// Built-in asset directory for this kind, if built-ins are allowed.
    #[must_use]
    pub const fn builtin_dir(self) -> Option<&'static str> {
        match self {
            Self::Model => None,
            Self::Node => Some("nodes"),
            Self::Composition => Some("compositions"),
            Self::Workflow => Some("workflows"),
        }
    }

    fn rust_path_name(self) -> &'static str {
        match self {
            Self::Model => "Model",
            Self::Node => "Node",
            Self::Composition => "Composition",
            Self::Workflow => "Workflow",
        }
    }
}

impl Display for AssetKind {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.project_dir())
    }
}

/// Asset stability marker.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stability {
    Draft,
    Experimental,
    Stable,
}

impl Stability {
    fn rust_path_name(self) -> &'static str {
        match self {
            Self::Draft => "Draft",
            Self::Experimental => "Experimental",
            Self::Stable => "Stable",
        }
    }
}

/// Metadata embedded in a Rust asset file.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct AssetMeta {
    pub id: &'static str,
    pub title: &'static str,
    pub kind: AssetKind,
    pub description: &'static str,
    pub stability: Stability,
}

/// Owned metadata extracted from a Rust asset file.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AssetMetaOwned {
    pub id: String,
    pub title: String,
    pub kind: AssetKind,
    pub description: String,
    pub stability: Stability,
}

/// Discovered asset plus source location.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct AssetRecord {
    pub meta: AssetMetaOwned,
    pub source_path: PathBuf,
    pub builtin: bool,
}

/// Minimal workflow definition builder used by self-contained workflow assets.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkflowDef {
    pub id: String,
    pub input_schema: Option<String>,
    pub output_schema: Option<String>,
    pub required_models: Vec<String>,
    pub steps: Vec<WorkflowStepDef>,
}

impl WorkflowDef {
    /// Set the public input schema path.
    #[must_use]
    pub fn input_schema(mut self, path: impl Into<String>) -> Self {
        self.input_schema = Some(path.into());
        self
    }

    /// Set the public output schema path.
    #[must_use]
    pub fn output_schema(mut self, path: impl Into<String>) -> Self {
        self.output_schema = Some(path.into());
        self
    }

    /// Require a project model alias.
    #[must_use]
    pub fn required_model(mut self, alias: impl Into<String>) -> Self {
        self.required_models.push(alias.into());
        self
    }

    /// Add a CortexFS API-format step.
    #[must_use]
    pub fn api_step(
        mut self,
        step_id: impl Into<String>,
        node_or_composition_id: impl Into<String>,
        format: impl Into<String>,
    ) -> Self {
        self.steps.push(WorkflowStepDef {
            step_id: step_id.into(),
            node_or_composition_id: node_or_composition_id.into(),
            target: WorkflowStepTarget::Api {
                format: format.into(),
            },
            request: None,
        });
        self
    }

    /// Render the previous step as an OpenAI chat request from one string input field.
    #[must_use]
    pub fn openai_chat_input(
        mut self,
        model_alias: impl Into<String>,
        input_field: impl Into<String>,
    ) -> Self {
        if let Some(step) = self.steps.last_mut() {
            step.request = Some(WorkflowRequestTemplate::OpenAiChatPrompt {
                model_alias: model_alias.into(),
                input_field: input_field.into(),
            });
        }
        self
    }

    /// Add a CortexFS tool invocation step.
    #[must_use]
    pub fn tool_step(
        mut self,
        step_id: impl Into<String>,
        node_or_composition_id: impl Into<String>,
        tool_id: impl Into<String>,
    ) -> Self {
        self.steps.push(WorkflowStepDef {
            step_id: step_id.into(),
            node_or_composition_id: node_or_composition_id.into(),
            target: WorkflowStepTarget::Tool {
                tool_id: tool_id.into(),
            },
            request: None,
        });
        self
    }

    /// Add a CortexFS thread message step.
    #[must_use]
    pub fn thread_step(
        mut self,
        step_id: impl Into<String>,
        node_or_composition_id: impl Into<String>,
        thread_id: impl Into<String>,
    ) -> Self {
        self.steps.push(WorkflowStepDef {
            step_id: step_id.into(),
            node_or_composition_id: node_or_composition_id.into(),
            target: WorkflowStepTarget::Thread {
                thread_id: thread_id.into(),
            },
            request: None,
        });
        self
    }
}

/// Start a workflow definition.
#[must_use]
pub fn workflow(id: impl Into<String>) -> WorkflowDef {
    WorkflowDef {
        id: id.into(),
        input_schema: None,
        output_schema: None,
        required_models: Vec::new(),
        steps: Vec::new(),
    }
}

/// One planned workflow step.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
pub struct WorkflowStepDef {
    pub step_id: String,
    pub node_or_composition_id: String,
    pub target: WorkflowStepTarget,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request: Option<WorkflowRequestTemplate>,
}

/// CortexFS-backed target for a workflow step.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkflowStepTarget {
    Api { format: String },
    Tool { tool_id: String },
    Thread { thread_id: String },
}

/// Optional LightFlow request rendering rule for a planned CortexFS step.
#[derive(Debug, Clone, Eq, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum WorkflowRequestTemplate {
    #[serde(rename = "openai_chat_prompt")]
    OpenAiChatPrompt {
        model_alias: String,
        input_field: String,
    },
}

/// Scan project and built-in roots for one asset kind.
pub fn discover_assets(root: &Path, kind: AssetKind) -> Result<Vec<AssetRecord>, AssetError> {
    let mut records = Vec::new();
    records.extend(scan_asset_dir(
        &root.join(PROJECT_ASSET_ROOT).join(kind.project_dir()),
        kind,
        false,
    )?);
    if let Some(dir) = kind.builtin_dir() {
        records.extend(scan_asset_dir(
            &root.join(BUILTIN_ASSET_ROOT).join(dir),
            kind,
            true,
        )?);
    }
    records.sort_by(|left, right| left.meta.id.cmp(&right.meta.id));
    Ok(records)
}

fn scan_asset_dir(
    dir: &Path,
    expected_kind: AssetKind,
    builtin: bool,
) -> Result<Vec<AssetRecord>, AssetError> {
    let mut records = Vec::new();
    match fs::read_dir(dir) {
        Ok(entries) => {
            for entry in entries {
                let entry = entry.map_err(AssetError::Io)?;
                let path = entry.path();
                if path.extension().and_then(|extension| extension.to_str()) != Some("rs") {
                    continue;
                }
                let meta = read_asset_meta(&path)?;
                if meta.kind != expected_kind {
                    return Err(AssetError::KindMismatch {
                        path,
                        expected: expected_kind,
                        actual: meta.kind,
                    });
                }
                records.push(AssetRecord {
                    meta,
                    source_path: path,
                    builtin,
                });
            }
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => return Err(AssetError::Io(error)),
    }
    Ok(records)
}

/// Read the `META` constant from one Rust asset file.
pub fn read_asset_meta(path: &Path) -> Result<AssetMetaOwned, AssetError> {
    let file = parse_asset_file(path)?;
    meta_from_file(&file).ok_or_else(|| AssetError::MissingMeta {
        path: path.to_path_buf(),
    })
}

/// Read the `define()` workflow builder from one Rust workflow asset file.
pub fn read_workflow_def(path: &Path) -> Result<WorkflowDef, AssetError> {
    let file = parse_asset_file(path)?;
    let meta = meta_from_file(&file).ok_or_else(|| AssetError::MissingMeta {
        path: path.to_path_buf(),
    })?;
    let define = file
        .items
        .iter()
        .find_map(|item| match item {
            Item::Fn(item_fn) if item_fn.sig.ident == "define" => Some(item_fn),
            _ => None,
        })
        .ok_or_else(|| AssetError::MissingDefinition {
            path: path.to_path_buf(),
        })?;
    let definition =
        workflow_def_from_fn(define, &meta).ok_or_else(|| AssetError::UnsupportedDefinition {
            path: path.to_path_buf(),
        })?;
    if definition.id != meta.id {
        return Err(AssetError::DefinitionMismatch {
            path: path.to_path_buf(),
            meta_id: meta.id,
            definition_id: definition.id,
        });
    }
    Ok(definition)
}

fn parse_asset_file(path: &Path) -> Result<File, AssetError> {
    let source = fs::read_to_string(path).map_err(AssetError::Io)?;
    syn::parse_file(&source).map_err(|error| AssetError::Parse {
        path: path.to_path_buf(),
        message: error.to_string(),
    })
}

fn meta_from_file(file: &File) -> Option<AssetMetaOwned> {
    file.items.iter().find_map(|item| match item {
        Item::Const(item_const) if item_const.ident == META_CONST => meta_from_const(item_const),
        _ => None,
    })
}

fn meta_from_const(item_const: &ItemConst) -> Option<AssetMetaOwned> {
    let Expr::Struct(expr_struct) = item_const.expr.as_ref() else {
        return None;
    };
    if path_last_ident(&expr_struct.path)? != "AssetMeta" {
        return None;
    }
    meta_from_struct(expr_struct)
}

fn meta_from_struct(expr_struct: &ExprStruct) -> Option<AssetMetaOwned> {
    let mut id = None;
    let mut title = None;
    let mut kind = None;
    let mut description = None;
    let mut stability = None;

    for field in &expr_struct.fields {
        match field_name(field)?.as_str() {
            "id" => id = string_lit(&field.expr),
            "title" => title = string_lit(&field.expr),
            "kind" => kind = asset_kind_expr(&field.expr),
            "description" => description = string_lit(&field.expr),
            "stability" => stability = stability_expr(&field.expr),
            _ => {}
        }
    }

    Some(AssetMetaOwned {
        id: id?,
        title: title?,
        kind: kind?,
        description: description?,
        stability: stability?,
    })
}

fn field_name(field: &FieldValue) -> Option<String> {
    match &field.member {
        Member::Named(ident) => Some(ident.to_string()),
        Member::Unnamed(_) => None,
    }
}

fn string_lit(expr: &Expr) -> Option<String> {
    let Expr::Lit(ExprLit {
        lit: Lit::Str(value),
        ..
    }) = expr
    else {
        return None;
    };
    Some(value.value())
}

fn asset_kind_expr(expr: &Expr) -> Option<AssetKind> {
    let name = enum_variant_name(expr)?;
    [
        AssetKind::Model,
        AssetKind::Node,
        AssetKind::Composition,
        AssetKind::Workflow,
    ]
    .into_iter()
    .find(|kind| kind.rust_path_name() == name.as_str())
}

fn stability_expr(expr: &Expr) -> Option<Stability> {
    let name = enum_variant_name(expr)?;
    [Stability::Draft, Stability::Experimental, Stability::Stable]
        .into_iter()
        .find(|stability| stability.rust_path_name() == name.as_str())
}

fn enum_variant_name(expr: &Expr) -> Option<String> {
    let Expr::Path(ExprPath { path, .. }) = expr else {
        return None;
    };
    path_last_ident(path)
}

fn path_last_ident(path: &syn::Path) -> Option<String> {
    let PathSegment { ident, .. } = path.segments.last()?;
    Some(ident.to_string())
}

fn workflow_def_from_fn(item_fn: &ItemFn, meta: &AssetMetaOwned) -> Option<WorkflowDef> {
    let expr = item_fn.block.stmts.last().and_then(stmt_expr)?;
    workflow_def_from_expr(expr, meta)
}

fn stmt_expr(stmt: &Stmt) -> Option<&Expr> {
    match stmt {
        Stmt::Expr(expr, _) => Some(expr),
        _ => None,
    }
}

fn workflow_def_from_expr(expr: &Expr, meta: &AssetMetaOwned) -> Option<WorkflowDef> {
    match expr {
        Expr::Call(call) => workflow_def_from_call(call, meta),
        Expr::MethodCall(method_call) => workflow_def_from_method_call(method_call, meta),
        _ => None,
    }
}

fn workflow_def_from_call(call: &ExprCall, meta: &AssetMetaOwned) -> Option<WorkflowDef> {
    let Expr::Path(function) = call.func.as_ref() else {
        return None;
    };
    if path_last_ident(&function.path)? != "workflow" || call.args.len() != 1 {
        return None;
    }
    let id = workflow_string_arg(call.args.first()?, meta)?;
    Some(workflow(id))
}

fn workflow_def_from_method_call(
    method_call: &ExprMethodCall,
    meta: &AssetMetaOwned,
) -> Option<WorkflowDef> {
    let mut definition = workflow_def_from_expr(&method_call.receiver, meta)?;
    let method = method_call.method.to_string();
    match method.as_str() {
        "input_schema" if method_call.args.len() == 1 => {
            definition =
                definition.input_schema(workflow_string_arg(method_call.args.first()?, meta)?);
        }
        "output_schema" if method_call.args.len() == 1 => {
            definition =
                definition.output_schema(workflow_string_arg(method_call.args.first()?, meta)?);
        }
        "required_model" if method_call.args.len() == 1 => {
            definition =
                definition.required_model(workflow_string_arg(method_call.args.first()?, meta)?);
        }
        "api_step" if method_call.args.len() == 3 => {
            definition = definition.api_step(
                workflow_string_arg(method_call.args.first()?, meta)?,
                workflow_string_arg(method_call.args.iter().nth(1)?, meta)?,
                workflow_string_arg(method_call.args.iter().nth(2)?, meta)?,
            );
        }
        "openai_chat_input" if method_call.args.len() == 2 => {
            definition = definition.openai_chat_input(
                workflow_string_arg(method_call.args.first()?, meta)?,
                workflow_string_arg(method_call.args.iter().nth(1)?, meta)?,
            );
        }
        "tool_step" if method_call.args.len() == 3 => {
            definition = definition.tool_step(
                workflow_string_arg(method_call.args.first()?, meta)?,
                workflow_string_arg(method_call.args.iter().nth(1)?, meta)?,
                workflow_string_arg(method_call.args.iter().nth(2)?, meta)?,
            );
        }
        "thread_step" if method_call.args.len() == 3 => {
            definition = definition.thread_step(
                workflow_string_arg(method_call.args.first()?, meta)?,
                workflow_string_arg(method_call.args.iter().nth(1)?, meta)?,
                workflow_string_arg(method_call.args.iter().nth(2)?, meta)?,
            );
        }
        _ => return None,
    }
    Some(definition)
}

fn workflow_string_arg(expr: &Expr, meta: &AssetMetaOwned) -> Option<String> {
    string_lit(expr).or_else(|| meta_field_string_arg(expr, meta))
}

fn meta_field_string_arg(expr: &Expr, meta: &AssetMetaOwned) -> Option<String> {
    let Expr::Field(ExprField { base, member, .. }) = expr else {
        return None;
    };
    let Expr::Path(base_path) = base.as_ref() else {
        return None;
    };
    if path_last_ident(&base_path.path)? != "META" {
        return None;
    }
    match member {
        Member::Named(ident) if ident == "id" => Some(meta.id.clone()),
        _ => None,
    }
}

/// Asset discovery failure.
#[derive(Debug)]
pub enum AssetError {
    Io(io::Error),
    Parse {
        path: PathBuf,
        message: String,
    },
    MissingMeta {
        path: PathBuf,
    },
    MissingDefinition {
        path: PathBuf,
    },
    UnsupportedDefinition {
        path: PathBuf,
    },
    DefinitionMismatch {
        path: PathBuf,
        meta_id: String,
        definition_id: String,
    },
    KindMismatch {
        path: PathBuf,
        expected: AssetKind,
        actual: AssetKind,
    },
}

impl Display for AssetError {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(error) => write!(f, "asset I/O error: {error}"),
            Self::Parse { path, message } => {
                write!(f, "failed to parse asset {}: {message}", path.display())
            }
            Self::MissingMeta { path } => {
                write!(f, "asset {} does not define META", path.display())
            }
            Self::MissingDefinition { path } => {
                write!(
                    f,
                    "workflow asset {} does not define define()",
                    path.display()
                )
            }
            Self::UnsupportedDefinition { path } => write!(
                f,
                "workflow asset {} uses an unsupported define() shape",
                path.display()
            ),
            Self::DefinitionMismatch {
                path,
                meta_id,
                definition_id,
            } => write!(
                f,
                "workflow asset {} define() id {} does not match META id {}",
                path.display(),
                definition_id,
                meta_id
            ),
            Self::KindMismatch {
                path,
                expected,
                actual,
            } => write!(
                f,
                "asset {} has kind {}, expected {}",
                path.display(),
                actual,
                expected
            ),
        }
    }
}

impl std::error::Error for AssetError {}

impl From<io::Error> for AssetError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AssetError, AssetKind, AssetMeta, AssetMetaOwned, Stability, discover_assets,
        read_asset_meta, read_workflow_def, workflow,
    };
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn reads_meta_from_self_contained_rust_asset() -> Result<(), Box<dyn std::error::Error>> {
        let root = unique_temp_root();
        let asset_dir = root.join("lightflow").join("workflows");
        fs::create_dir_all(&asset_dir)?;
        let asset_path = asset_dir.join("text_plan.rs");
        fs::write(
            &asset_path,
            r#"
use lightflow::asset::*;

pub const META: AssetMeta = AssetMeta {
    id: "workflow.text_plan",
    title: "Text Plan",
    kind: AssetKind::Workflow,
    description: "Draft a plan from text.",
    stability: Stability::Draft,
};
"#,
        )?;

        let meta = read_asset_meta(&asset_path)?;

        assert_eq!(
            meta,
            AssetMetaOwned {
                id: "workflow.text_plan".to_owned(),
                title: "Text Plan".to_owned(),
                kind: AssetKind::Workflow,
                description: "Draft a plan from text.".to_owned(),
                stability: Stability::Draft,
            }
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn asset_meta_and_workflow_builder_support_documented_asset_shape() {
        const META: AssetMeta = AssetMeta {
            id: "workflow.example",
            title: "Example",
            kind: AssetKind::Workflow,
            description: "Example workflow.",
            stability: Stability::Draft,
        };

        let definition = workflow(META.id)
            .input_schema("schemas/example.input.json")
            .output_schema("schemas/example.output.json")
            .required_model("llm.planner")
            .api_step("draft", "node.llm_prompt", "openai.chat")
            .openai_chat_input("llm.planner", "prompt");

        assert_eq!(META.kind, AssetKind::Workflow);
        assert_eq!(definition.id, "workflow.example");
        assert_eq!(
            definition.input_schema.as_deref(),
            Some("schemas/example.input.json")
        );
        assert_eq!(
            definition.output_schema.as_deref(),
            Some("schemas/example.output.json")
        );
        assert_eq!(definition.required_models, ["llm.planner"]);
        assert_eq!(definition.steps.len(), 1);
        assert_eq!(
            serde_json::to_value(&definition.steps[0].request).unwrap(),
            serde_json::json!({
                "kind": "openai_chat_prompt",
                "model_alias": "llm.planner",
                "input_field": "prompt"
            })
        );
    }

    #[test]
    fn reads_workflow_request_template_from_builder_chain() -> Result<(), Box<dyn std::error::Error>>
    {
        let root = unique_temp_root();
        let asset_path = root
            .join("lightflow")
            .join("workflows")
            .join("text_plan.rs");
        let parent = asset_path.parent().ok_or("asset path has no parent")?;
        fs::create_dir_all(parent)?;
        fs::write(
            &asset_path,
            r#"
use lightflow::asset::*;

pub const META: AssetMeta = AssetMeta {
    id: "workflow.text_plan",
    title: "Text Plan",
    kind: AssetKind::Workflow,
    description: "Draft a plan from text.",
    stability: Stability::Draft,
};

pub fn define() -> WorkflowDef {
    workflow(META.id)
        .input_schema("schemas/text_plan.input.json")
        .output_schema("schemas/text_plan.output.json")
        .required_model("llm.planner")
        .api_step("draft", "node.llm_prompt", "openai.chat")
        .openai_chat_input("llm.planner", "prompt")
}
"#,
        )?;

        let definition = read_workflow_def(&asset_path)?;

        assert_eq!(
            serde_json::to_value(&definition.steps[0].request).unwrap(),
            serde_json::json!({
                "kind": "openai_chat_prompt",
                "model_alias": "llm.planner",
                "input_field": "prompt"
            })
        );

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn discovers_project_and_builtin_assets_without_sidecar_registry()
    -> Result<(), Box<dyn std::error::Error>> {
        let root = unique_temp_root();
        write_asset(
            &root.join("lightflow").join("nodes").join("prompt.rs"),
            "node.prompt",
            "Prompt",
            "Node",
        )?;
        write_asset(
            &root.join("src/builtins").join("nodes").join("echo.rs"),
            "node.echo",
            "Echo",
            "Node",
        )?;
        fs::write(
            root.join("lightflow").join("nodes").join("ignored.txt"),
            "not rust",
        )?;

        let records = discover_assets(&root, AssetKind::Node)?;

        assert_eq!(records.len(), 2);
        assert_eq!(records[0].meta.id, "node.echo");
        assert!(records[0].builtin);
        assert_eq!(records[1].meta.id, "node.prompt");
        assert!(!records[1].builtin);

        fs::remove_dir_all(root)?;
        Ok(())
    }

    #[test]
    fn rejects_kind_mismatch_for_scanned_directory() -> Result<(), Box<dyn std::error::Error>> {
        let root = unique_temp_root();
        write_asset(
            &root.join("lightflow").join("nodes").join("wrong.rs"),
            "workflow.wrong",
            "Wrong",
            "Workflow",
        )?;

        let error = discover_assets(&root, AssetKind::Node).unwrap_err();

        assert!(matches!(error, AssetError::KindMismatch { .. }));

        fs::remove_dir_all(root)?;
        Ok(())
    }

    fn write_asset(
        path: &std::path::Path,
        id: &str,
        title: &str,
        kind: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let parent = path.parent().ok_or("asset path has no parent")?;
        fs::create_dir_all(parent)?;
        fs::write(
            path,
            format!(
                r#"
use lightflow::asset::*;

pub const META: AssetMeta = AssetMeta {{
    id: "{id}",
    title: "{title}",
    kind: AssetKind::{kind},
    description: "Test asset.",
    stability: Stability::Experimental,
}};

pub fn define() -> WorkflowDef {{
    workflow(META.id).api_step("draft", "node.llm_prompt", "openai.chat")
}}
"#
            ),
        )?;
        Ok(())
    }

    fn unique_temp_root() -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!(
            "lightflow-assets-test-{}-{nanos}",
            std::process::id()
        ))
    }
}
