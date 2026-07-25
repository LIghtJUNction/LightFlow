use crate::api::{ApiError, ApiResult};
use crate::workflow::{
    PortSpec, WorkflowEdge, WorkflowEndpoint, WorkflowNode, WorkflowNodeKind, WorkflowPosition,
    WorkflowSpec,
};
use std::path::Path;
use syn::parse::{Parse, ParseStream};
use syn::{Expr, Ident, LitBool, LitStr, Token, braced, bracketed, token};

pub(super) fn apply_workflow_macro_ports(
    workflow: &mut WorkflowSpec,
    call: &syn::ExprMacro,
    path: &Path,
) -> ApiResult<()> {
    let body = syn::parse2::<WorkflowMacroBody>(call.mac.tokens.clone()).map_err(|error| {
        ApiError::InvalidRequest(format!("invalid workflow! block in {:?}: {error}", path))
    })?;
    for item in body.0 {
        match item {
            WorkflowItem::Name(value) => workflow.name = value,
            WorkflowItem::Category(value) => workflow.category = Some(value),
            WorkflowItem::Description(value) => workflow.description = Some(value),
            WorkflowItem::Port(parsed) => {
                let mut port = PortSpec::new(parsed.name, parsed.ty);
                port.description = parsed.metadata.description;
                port.required = parsed.metadata.required;
                port.default = parsed.metadata.default;
                if let Some([min, max, step]) = parsed.metadata.range {
                    port.min = Some(min);
                    port.max = Some(max);
                    port.step = Some(step);
                }
                port.enum_values = parsed.metadata.choices.unwrap_or_default();
                port.widget = parsed.metadata.widget;
                port.artifact_kind = parsed.metadata.artifact;
                port.model_requirement = parsed.metadata.model;
                match parsed.kind {
                    PortKind::Input => workflow.inputs.push(port),
                    PortKind::Output => workflow.outputs.push(port),
                }
            }
            WorkflowItem::Node {
                id,
                workflow_id,
                version,
            } => {
                workflow.nodes.push(WorkflowNode {
                    id,
                    kind: WorkflowNodeKind::Workflow,
                    workflow_id: workflow_id.clone(),
                    condition: None,
                    then_workflow_id: None,
                    else_workflow_id: None,
                    title: None,
                    disabled: false,
                    position: WorkflowPosition::default(),
                    config: serde_json::Value::Null,
                });
                register_dependency(workflow, workflow_id, version);
            }
            WorkflowItem::Edge {
                from_node,
                from_port,
                to_node,
                to_port,
            } => {
                workflow.edges.push(WorkflowEdge {
                    from: WorkflowEndpoint {
                        node: from_node,
                        port: from_port,
                    },
                    to: WorkflowEndpoint {
                        node: to_node,
                        port: to_port,
                    },
                });
            }
        }
    }
    Ok(())
}

fn register_dependency(workflow: &mut WorkflowSpec, workflow_id: String, version: Option<String>) {
    workflow.merge_dependency(workflow_id, version, None);
}

struct WorkflowMacroBody(Vec<WorkflowItem>);

enum WorkflowItem {
    Name(String),
    Category(String),
    Description(String),
    Port(ParsedPort),
    Node {
        id: String,
        workflow_id: String,
        version: Option<String>,
    },
    Edge {
        from_node: String,
        from_port: String,
        to_node: String,
        to_port: String,
    },
}

impl Parse for WorkflowMacroBody {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut items = Vec::new();
        while !input.is_empty() {
            while input.peek(Token![,]) {
                input.parse::<Token![,]>()?;
            }
            if input.is_empty() {
                break;
            }
            let keyword: Ident = input.parse()?;
            match keyword.to_string().as_str() {
                "name" => {
                    input.parse::<Token![:]>()?;
                    items.push(WorkflowItem::Name(input.parse::<LitStr>()?.value()));
                }
                "category" => {
                    input.parse::<Token![:]>()?;
                    items.push(WorkflowItem::Category(input.parse::<LitStr>()?.value()));
                }
                "description" => {
                    input.parse::<Token![:]>()?;
                    items.push(WorkflowItem::Description(input.parse::<LitStr>()?.value()));
                }
                "input" => items.push(WorkflowItem::Port(parse_port(input, PortKind::Input)?)),
                "output" => items.push(WorkflowItem::Port(parse_port(input, PortKind::Output)?)),
                "node" => {
                    let id: Ident = input.parse()?;
                    input.parse::<Token![:]>()?;
                    let workflow_id = input.parse::<LitStr>()?.value();
                    let version = if input.peek(Token![@]) {
                        input.parse::<Token![@]>()?;
                        Some(input.parse::<LitStr>()?.value())
                    } else {
                        None
                    };
                    items.push(WorkflowItem::Node {
                        id: id.to_string(),
                        workflow_id,
                        version,
                    });
                }
                "edge" => {
                    let from_node: Ident = input.parse()?;
                    input.parse::<Token![.]>()?;
                    let from_port: Ident = input.parse()?;
                    input.parse::<Token![->]>()?;
                    let to_node: Ident = input.parse()?;
                    input.parse::<Token![.]>()?;
                    let to_port: Ident = input.parse()?;
                    items.push(WorkflowItem::Edge {
                        from_node: from_node.to_string(),
                        from_port: from_port.to_string(),
                        to_node: to_node.to_string(),
                        to_port: to_port.to_string(),
                    });
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        keyword,
                        "workflow! item must be name, category, description, input, output, node, or edge",
                    ));
                }
            }
        }
        Ok(Self(items))
    }
}

fn parse_port(input: ParseStream<'_>, kind: PortKind) -> syn::Result<ParsedPort> {
    let name: LitStr = input.parse()?;
    input.parse::<Token![:]>()?;
    let ty: LitStr = input.parse()?;
    let metadata = if input.peek(token::Brace) {
        let content;
        braced!(content in input);
        parse_metadata(&content, kind)?
    } else {
        PortMetadata::default()
    };
    Ok(ParsedPort {
        kind,
        name: name.value(),
        ty: ty.value(),
        metadata,
    })
}

#[derive(Clone, Copy)]
enum PortKind {
    Input,
    Output,
}

struct ParsedPort {
    kind: PortKind,
    name: String,
    ty: String,
    metadata: PortMetadata,
}

#[derive(Default)]
struct PortMetadata {
    description: Option<String>,
    required: Option<bool>,
    default: Option<serde_json::Value>,
    range: Option<[f64; 3]>,
    choices: Option<Vec<serde_json::Value>>,
    widget: Option<String>,
    artifact: Option<String>,
    model: Option<String>,
}

fn parse_metadata(input: ParseStream<'_>, kind: PortKind) -> syn::Result<PortMetadata> {
    let mut metadata = PortMetadata::default();
    while !input.is_empty() {
        let key: Ident = input.parse()?;
        input.parse::<Token![:]>()?;
        let key_name = key.to_string();
        match (kind, key_name.as_str()) {
            (_, "description") => set_once(
                &mut metadata.description,
                &key,
                input.parse::<LitStr>()?.value(),
            )?,
            (PortKind::Input, "required") => set_once(
                &mut metadata.required,
                &key,
                input.parse::<LitBool>()?.value,
            )?,
            (PortKind::Input, "default") => {
                let value = parse_json_value(input, &key)?;
                set_once(&mut metadata.default, &key, value)?;
            }
            (PortKind::Input, "range") => {
                let content;
                bracketed!(content in input);
                let min = parse_number(&content.parse::<Expr>()?)?;
                content.parse::<Token![,]>()?;
                let max = parse_number(&content.parse::<Expr>()?)?;
                content.parse::<Token![,]>()?;
                let step = parse_number(&content.parse::<Expr>()?)?;
                if !content.is_empty() {
                    return Err(content.error("range expects exactly [min, max, step]"));
                }
                set_once(&mut metadata.range, &key, [min, max, step])?;
            }
            (PortKind::Input, "choices") => {
                let value = parse_json_value(input, &key)?;
                let serde_json::Value::Array(values) = value else {
                    return Err(syn::Error::new_spanned(key, "choices must be a JSON array"));
                };
                set_once(&mut metadata.choices, &key, values)?;
            }
            (PortKind::Input, "widget") => {
                set_once(&mut metadata.widget, &key, input.parse::<LitStr>()?.value())?
            }
            (_, "artifact") => set_once(
                &mut metadata.artifact,
                &key,
                input.parse::<LitStr>()?.value(),
            )?,
            (_, "model") => set_once(&mut metadata.model, &key, input.parse::<LitStr>()?.value())?,
            (PortKind::Output, _) => {
                return Err(syn::Error::new_spanned(
                    key,
                    format!("unsupported output metadata key: {key_name}"),
                ));
            }
            (PortKind::Input, _) => {
                return Err(syn::Error::new_spanned(
                    key,
                    format!("unsupported input metadata key: {key_name}"),
                ));
            }
        }
        if input.is_empty() {
            break;
        }
        input.parse::<Token![,]>()?;
    }
    Ok(metadata)
}

fn parse_json_value(input: ParseStream<'_>, key: &Ident) -> syn::Result<serde_json::Value> {
    if input.peek(Token![-]) {
        input.parse::<Token![-]>()?;
        return parse_json_literal(input.parse()?, true, key);
    }
    if input.peek(token::Bracket) {
        let content;
        bracketed!(content in input);
        let mut values = Vec::new();
        while !content.is_empty() {
            values.push(parse_json_value(&content, key)?);
            if content.is_empty() {
                break;
            }
            content.parse::<Token![,]>()?;
        }
        return Ok(serde_json::Value::Array(values));
    }
    if input.peek(token::Brace) {
        let content;
        braced!(content in input);
        let mut object = serde_json::Map::new();
        while !content.is_empty() {
            let object_key: LitStr = content.parse()?;
            if !object_key.token().to_string().starts_with('"') {
                return Err(syn::Error::new_spanned(
                    object_key,
                    "JSON object keys must use ordinary quoted string literals",
                ));
            }
            content.parse::<Token![:]>()?;
            object.insert(object_key.value(), parse_json_value(&content, key)?);
            if content.is_empty() {
                break;
            }
            content.parse::<Token![,]>()?;
        }
        return Ok(serde_json::Value::Object(object));
    }
    if input.peek(syn::Lit) {
        return parse_json_literal(input.parse()?, false, key);
    }
    if input.peek(Ident) {
        let ident: Ident = input.parse()?;
        return match ident.to_string().as_str() {
            "null" => Ok(serde_json::Value::Null),
            "true" => Ok(serde_json::Value::Bool(true)),
            "false" => Ok(serde_json::Value::Bool(false)),
            _ => Err(syn::Error::new_spanned(
                ident,
                "value must use strict JSON literal syntax",
            )),
        };
    }
    Err(input.error(format!(
        "metadata {key} value must use strict JSON literal syntax"
    )))
}

fn parse_json_literal(
    literal: syn::Lit,
    negative: bool,
    key: &Ident,
) -> syn::Result<serde_json::Value> {
    let source = match literal {
        syn::Lit::Str(value) if !negative => {
            let source = value.token().to_string();
            if !source.starts_with('"') {
                return Err(syn::Error::new_spanned(
                    value,
                    "JSON strings must use ordinary quoted string literals",
                ));
            }
            source
        }
        syn::Lit::Bool(value) if !negative => return Ok(serde_json::Value::Bool(value.value)),
        syn::Lit::Int(value) => {
            if !value.suffix().is_empty() {
                return Err(syn::Error::new_spanned(
                    value,
                    "JSON numbers cannot use Rust suffixes",
                ));
            }
            format!(
                "{}{}",
                if negative { "-" } else { "" },
                value.base10_digits()
            )
        }
        syn::Lit::Float(value) => {
            if !value.suffix().is_empty() {
                return Err(syn::Error::new_spanned(
                    value,
                    "JSON numbers cannot use Rust suffixes",
                ));
            }
            format!(
                "{}{}",
                if negative { "-" } else { "" },
                value.base10_digits()
            )
        }
        other => {
            return Err(syn::Error::new_spanned(
                other,
                "value must be a JSON string, boolean, null, or number literal",
            ));
        }
    };
    serde_json::from_str(&source).map_err(|error| {
        syn::Error::new_spanned(key, format!("metadata value must be valid JSON: {error}"))
    })
}

fn parse_number(expression: &Expr) -> syn::Result<f64> {
    match expression {
        Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Float(value),
            ..
        }) => value.base10_parse(),
        Expr::Lit(syn::ExprLit {
            lit: syn::Lit::Int(value),
            ..
        }) => value.base10_parse(),
        Expr::Unary(syn::ExprUnary {
            op: syn::UnOp::Neg(_),
            expr,
            ..
        }) => Ok(-parse_number(expr)?),
        _ => Err(syn::Error::new_spanned(
            expression,
            "range values must be numeric literals",
        )),
    }
}

fn set_once<T>(slot: &mut Option<T>, key: &Ident, value: T) -> syn::Result<()> {
    if slot.is_some() {
        return Err(syn::Error::new_spanned(
            key,
            format!("duplicate metadata key: {key}"),
        ));
    }
    *slot = Some(value);
    Ok(())
}
