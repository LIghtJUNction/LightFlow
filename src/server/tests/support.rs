use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::Method;
use serde_json::json;
use tower::ServiceExt;

pub(crate) async fn request_json(app: &Router, path: &str) -> serde_json::Value {
    request_json_with_body(app, Method::GET, path, Body::empty()).await
}

pub(crate) async fn request_json_post(
    app: &Router,
    path: &str,
    body: serde_json::Value,
) -> serde_json::Value {
    request_json_with_body(app, Method::POST, path, Body::from(body.to_string())).await
}

pub(crate) async fn request_json_delete(app: &Router, path: &str) -> serde_json::Value {
    request_json_with_body(app, Method::DELETE, path, Body::empty()).await
}

pub(crate) async fn request_json_with_body(
    app: &Router,
    method: Method,
    path: &str,
    body: Body,
) -> serde_json::Value {
    let response = app
        .clone()
        .oneshot(
            axum::http::Request::builder()
                .method(method)
                .uri(path)
                .header("content-type", "application/json")
                .body(body)
                .expect("request"),
        )
        .await
        .expect("response");
    let status = response.status();
    let body = to_bytes(response.into_body(), usize::MAX)
        .await
        .expect("body");
    json!({
        "status": status.as_u16(),
        "body": serde_json::from_slice::<serde_json::Value>(&body).expect("json"),
    })
}

pub(crate) fn assert_required_fields(openapi: &str, schema: &str, body: &serde_json::Value) {
    for field in openapi_required_fields(openapi, schema) {
        assert!(
            body.get(&field).is_some(),
            "{schema} requires field `{field}`, but body was {body}"
        );
    }
}

pub(crate) fn assert_schema_property(openapi: &str, schema: &str, property: &str) {
    let schema_header = format!("    {schema}:");
    let property_line = format!("        {property}:");
    let mut in_schema = false;

    for line in openapi.lines() {
        if line == schema_header {
            in_schema = true;
            continue;
        }
        if !in_schema {
            continue;
        }
        if line.starts_with("    ") && !line.starts_with("      ") && line.ends_with(':') {
            break;
        }
        if line == property_line {
            return;
        }
    }

    panic!("{schema} must document property `{property}`");
}

pub(crate) fn assert_openapi_parameter_enum(
    openapi: &str,
    path: &str,
    parameter: &str,
    expected: &[&str],
) {
    let path_header = format!("  {path}:");
    let parameter_line = format!("        - name: {parameter}");
    let mut in_path = false;
    let mut in_parameter = false;

    for line in openapi.lines() {
        if line == path_header {
            in_path = true;
            continue;
        }
        if !in_path {
            continue;
        }
        if line.starts_with("  ") && !line.starts_with("    ") && line.ends_with(':') {
            break;
        }
        if line == parameter_line {
            in_parameter = true;
            continue;
        }
        if in_parameter {
            if line.starts_with("        - name: ") {
                break;
            }
            let trimmed = line.trim();
            if let Some(values) = trimmed.strip_prefix("enum: [") {
                let values = values.trim_end_matches(']');
                let actual = values.split(',').map(str::trim).collect::<Vec<_>>();
                assert_eq!(actual, expected, "{path} {parameter} enum");
                return;
            }
        }
    }

    panic!("{path} must document `{parameter}` enum {expected:?}");
}

pub(crate) fn openapi_required_fields(openapi: &str, schema: &str) -> Vec<String> {
    let schema_header = format!("    {schema}:");
    let mut in_schema = false;
    let mut required = Vec::new();
    let mut reading_block_list = false;

    for line in openapi.lines() {
        if line == schema_header {
            in_schema = true;
            continue;
        }
        if !in_schema {
            continue;
        }
        if line.starts_with("    ") && !line.starts_with("      ") && line.ends_with(':') {
            break;
        }
        let trimmed = line.trim();
        if reading_block_list {
            if let Some(item) = trimmed.strip_prefix("- ") {
                required.push(item.to_owned());
                continue;
            }
            if !trimmed.is_empty() {
                reading_block_list = false;
            }
        }
        if let Some(inline) = trimmed.strip_prefix("required: [") {
            let inline = inline.trim_end_matches(']');
            required.extend(inline.split(',').map(|field| field.trim().to_owned()));
        } else if trimmed == "required:" {
            reading_block_list = true;
        }
    }

    assert!(!required.is_empty(), "{schema} must define required fields");
    required
}

pub(crate) fn openapi_path_keys(openapi: &str) -> Vec<&str> {
    let mut in_paths = false;
    let mut paths = Vec::new();
    for line in openapi.lines() {
        if line == "paths:" {
            in_paths = true;
            continue;
        }
        if in_paths && !line.starts_with(' ') && line.ends_with(':') {
            break;
        }
        if in_paths && line.starts_with("  /") && line.ends_with(':') {
            paths.push(line.trim().trim_end_matches(':'));
        }
    }
    paths
}

pub(crate) fn temp_root(name: &str) -> std::path::PathBuf {
    std::env::temp_dir().join(format!("lightflow-server-{name}-{}", std::process::id()))
}

pub(crate) fn write_publishable_project_workflow(root: &std::path::Path) {
    let crate_dir = root.join("projects/lightflow-std/workflows/std/http_publish");
    std::fs::create_dir_all(crate_dir.join("src")).expect("workflow crate dir");
    std::fs::write(
        crate_dir.join("Cargo.toml"),
        r#"[package]
name = "lightflow-http-publish"
version = "0.1.0"
edition = "2024"
description = "HTTP publish fixture."
license = "MIT OR Apache-2.0"

[dependencies]
"#,
    )
    .expect("workflow manifest");
    std::fs::write(
        crate_dir.join("src/lib.rs"),
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
        .name("HTTP Publish")
        .description("HTTP publish readiness fixture.")
        .input("value", "json")
        .input_description("value", "Fixture input value.")
        .output("value", "json")
        .output_description("value", "Fixture output value.")
        .build()
}
"#,
    )
    .expect("workflow source");
}

pub(crate) fn std_project_path() -> std::path::PathBuf {
    std::env::current_dir()
        .expect("current dir")
        .join("projects/lightflow-std")
}

pub(crate) fn git_ok<const N: usize>(cwd: &std::path::Path, args: [&str; N]) {
    let mut command_args = Vec::with_capacity(N + 6);
    if args.contains(&"commit") {
        command_args.extend([
            "-c",
            "user.name=LightFlow Test",
            "-c",
            "user.email=lightflow-test@example.invalid",
            "-c",
            "commit.gpgsign=false",
        ]);
    }
    command_args.extend_from_slice(&args);

    let output = std::process::Command::new("git")
        .args(command_args)
        .current_dir(cwd)
        .output()
        .expect("git command");
    assert!(
        output.status.success(),
        "git failed in {}: {}\nstdout:\n{}\nstderr:\n{}",
        cwd.display(),
        output.status,
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
