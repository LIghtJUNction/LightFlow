use super::{ApiError, ApiResult, workflow_package_identity};
use crate::workflow::WorkflowSpec;
use std::fs;
use std::path::Path;

mod arguments;
mod builder;
mod macro_ports;
use builder::{define_expression, parse_workflow_builder, parse_workflow_builder_with_identity};

pub(super) fn read_optional_workflow_source(path: &Path) -> ApiResult<Option<WorkflowSpec>> {
    read_optional_workflow_source_with_identity(path, None)
}

pub(super) fn read_optional_workflow_source_from_manifest(
    path: &Path,
    manifest: &Path,
) -> ApiResult<Option<WorkflowSpec>> {
    let identity = workflow_package_identity(manifest)?;
    read_optional_workflow_source_with_identity(path, Some(identity))
}

fn read_optional_workflow_source_with_identity(
    path: &Path,
    package_identity: Option<(String, String)>,
) -> ApiResult<Option<WorkflowSpec>> {
    let source = fs::read_to_string(path).map_err(ApiError::from)?;
    let file = syn::parse_file(&source).map_err(|error| {
        ApiError::InvalidRequest(format!(
            "invalid Rust workflow source in {:?}: {error}",
            path
        ))
    })?;
    let Some(define) = file.items.iter().find_map(|item| match item {
        syn::Item::Fn(function) if is_public_workflow_define(function) => Some(function),
        _ => None,
    }) else {
        return Ok(None);
    };
    let expression = define_expression(define).ok_or_else(|| {
        ApiError::InvalidRequest(format!(
            "workflow source {:?} must return a workflow!() builder expression",
            path
        ))
    })?;
    match package_identity {
        Some((id, version)) => {
            parse_workflow_builder_with_identity(expression, path, &id, &version).map(Some)
        }
        None => parse_workflow_builder(expression, path).map(Some),
    }
}

fn is_public_workflow_define(function: &syn::ItemFn) -> bool {
    if function.sig.ident != "define" || !matches!(&function.vis, syn::Visibility::Public(_)) {
        return false;
    }
    let syn::ReturnType::Type(_, ty) = &function.sig.output else {
        return false;
    };
    let syn::Type::Path(path) = ty.as_ref() else {
        return false;
    };
    path.path
        .segments
        .last()
        .is_some_and(|segment| segment.ident == "WorkflowSpec")
}

pub(crate) fn read_workflow_source(path: &Path) -> ApiResult<WorkflowSpec> {
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
            "workflow source {:?} must return a workflow!() builder expression",
            path
        ))
    })?;
    parse_workflow_builder(expression, path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn qualified_workflow_macro_uses_manifest_identity() {
        let root = tempfile::tempdir().expect("tempdir");
        let crate_dir = root.path().join("flow");
        fs::create_dir_all(crate_dir.join("src")).expect("source dir");
        fs::write(
            crate_dir.join("Cargo.toml"),
            "[package]\nname = \"lightflow-qualified-flow\"\nversion = \"3.2.1\"\n",
        )
        .expect("manifest");
        let source = crate_dir.join("src/lib.rs");
        fs::write(
            &source,
            r#"pub fn define() -> lightflow::workflow::WorkflowSpec {
    lightflow::workflow!().name("Qualified").build()
}
"#,
        )
        .expect("source");

        let workflow = read_workflow_source(&source).expect("workflow");
        assert_eq!(workflow.id, "lightflow.qualified_flow");
        assert_eq!(workflow.version, "3.2.1");
    }

    #[test]
    fn workflow_builder_parses_macro_ports_and_metadata() {
        let root = tempfile::tempdir().expect("tempdir");
        let crate_dir = root.path().join("categorized");
        fs::create_dir_all(crate_dir.join("src")).expect("source dir");
        fs::write(
            crate_dir.join("Cargo.toml"),
            "[package]\nname = \"lightflow-categorized\"\nversion = \"0.1.0\"\n",
        )
        .expect("manifest");
        let source = crate_dir.join("src/lib.rs");
        fs::write(
            &source,
            r#"pub fn define() -> WorkflowSpec {
    workflow! {
        output "image": "artifact" {
            description: "Generated image.",
            artifact: "image",
            model: "image_model",
        }
        input "condition": "boolean" {
            description: "Whether to render.",
            required: true,
            default: false,
            widget: "checkbox",
        }
        input "strength": "number" {
            range: [-1.0, 1.0, 0.05],
            choices: [0.25, 0.5, 0.75, 1.0],
        }
        input "config": "json" {
            default: {
                "enabled": true,
                "items": [null, false, {"offset": -2,},],
            },
        }
        input "negative": "integer" { default: -1, }
        input "nothing": "json" { default: null, }
        input "items": "json" { default: [1, "two", false], }
        input "label": "text" { default: "demo", }
    }
    .category("media")
    .name("Categorized")
    .build()
}
"#,
        )
        .expect("source");

        let workflow = read_workflow_source(&source).expect("workflow");

        assert_eq!(workflow.category.as_deref(), Some("media"));
        assert_eq!(workflow.inputs.len(), 7);
        assert_eq!(workflow.outputs.len(), 1);
        assert_eq!(workflow.inputs[0].required, Some(true));
        assert_eq!(workflow.inputs[0].default, Some(serde_json::json!(false)));
        assert_eq!(workflow.inputs[1].min, Some(-1.0));
        assert_eq!(workflow.inputs[1].step, Some(0.05));
        assert_eq!(workflow.inputs[1].enum_values.len(), 4);
        assert_eq!(
            workflow.inputs[2].default,
            Some(serde_json::json!({
                "enabled": true,
                "items": [null, false, {"offset": -2}],
            }))
        );
        assert_eq!(workflow.inputs[3].default, Some(serde_json::json!(-1)));
        assert_eq!(workflow.inputs[4].default, Some(serde_json::Value::Null));
        assert_eq!(
            workflow.inputs[5].default,
            Some(serde_json::json!([1, "two", false]))
        );
        assert_eq!(workflow.inputs[6].default, Some(serde_json::json!("demo")));
        assert_eq!(
            workflow.outputs[0].model_requirement.as_deref(),
            Some("image_model")
        );
    }

    #[test]
    fn workflow_builder_rejects_invalid_macro_ports() {
        for (name, body, expected) in [
            (
                "output-required",
                r#"output "value": "json" { required: true }"#,
                "unsupported output metadata key: required",
            ),
            (
                "unknown-port",
                r#"stream "value": "json""#,
                "must start with input or output",
            ),
            (
                "non-string-name",
                r#"input value: "json""#,
                "expected string literal",
            ),
            (
                "choices-not-array",
                r#"input "value": "json" { choices: true }"#,
                "choices must be a JSON array",
            ),
            (
                "unknown-input-key",
                r#"input "value": "json" { mystery: true }"#,
                "unsupported input metadata key: mystery",
            ),
            (
                "required-expression",
                r#"input "value": "json" { required: bool::default() }"#,
                "expected boolean literal",
            ),
            (
                "range-expression",
                r#"input "value": "number" { range: [minimum(), 1, 1] }"#,
                "range values must be numeric literals",
            ),
            (
                "duplicate-metadata",
                r#"input "value": "json" { required: true, required: false }"#,
                "duplicate metadata key: required",
            ),
            (
                "nested-expression",
                r#"input "value": "json" { default: {"items": [bool::default()]} }"#,
                "strict JSON literal syntax",
            ),
            (
                "choices-expression",
                r#"input "value": "json" { choices: [1, make_value()] }"#,
                "strict JSON literal syntax",
            ),
            (
                "unquoted-object-key",
                r#"input "value": "json" { default: {enabled: true} }"#,
                "expected string literal",
            ),
            (
                "arithmetic-default",
                r#"input "value": "json" { default: 1 + 2 }"#,
                "expected `,`",
            ),
            (
                "suffixed-number",
                r#"input "value": "json" { default: 1u32 }"#,
                "JSON numbers cannot use Rust suffixes",
            ),
        ] {
            let root = tempfile::tempdir().expect("tempdir");
            let crate_dir = root.path().join(name);
            fs::create_dir_all(crate_dir.join("src")).expect("source dir");
            fs::write(
                crate_dir.join("Cargo.toml"),
                format!("[package]\nname = {name:?}\nversion = \"0.1.0\"\n"),
            )
            .expect("manifest");
            let source = crate_dir.join("src/lib.rs");
            fs::write(
                &source,
                format!(
                    "pub fn define() -> WorkflowSpec {{ workflow! {{ {body} }}.name(\"Invalid\").build() }}\n"
                ),
            )
            .expect("source");

            let error = read_workflow_source(&source).expect_err("invalid macro port");

            assert!(
                error.message().contains(expected),
                "{name}: expected {expected:?}, got {:?}",
                error.message()
            );
        }
    }

    #[test]
    fn optional_discovery_ignores_non_public_or_wrong_return_define() {
        for (name, source) in [
            (
                "private",
                "fn define() -> WorkflowSpec { workflow!().name(\"Private\").build() }\n",
            ),
            (
                "wrong-return",
                "pub fn define() -> String { workflow!().name(\"Wrong\").build() }\n",
            ),
        ] {
            let root = tempfile::tempdir().expect("tempdir");
            let crate_dir = root.path().join(name);
            fs::create_dir_all(crate_dir.join("src")).expect("source dir");
            fs::write(
                crate_dir.join("Cargo.toml"),
                format!("[package]\nname = {name:?}\nversion = \"0.1.0\"\n"),
            )
            .expect("manifest");
            let lib = crate_dir.join("src/lib.rs");
            fs::write(&lib, source).expect("source");

            assert!(
                read_optional_workflow_source(&lib)
                    .expect("optional workflow")
                    .is_none(),
                "{name} define must not be discovered"
            );
        }
    }
}
