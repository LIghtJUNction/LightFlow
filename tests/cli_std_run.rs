mod support;

use lightflow::api::ApiService;
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;
use support::*;

#[test]
fn repository_std_workflow_is_library_only_and_abstract() -> Result<(), Box<dyn std::error::Error>>
{
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let service = ApiService::new(root);
    let workflow = service.get_workflow("lightflow.std")?;

    assert_eq!(workflow.id, "lightflow.std");
    assert_eq!(workflow.version, "0.1.0");
    assert_eq!(workflow.name, "LightFlow Std Identity");
    assert_eq!(workflow.inputs.len(), 1);
    assert_eq!(workflow.outputs.len(), 1);
    assert!(workflow.dependencies.is_empty());
    assert!(workflow.nodes.is_empty());
    assert!(workflow.edges.is_empty());

    assert_eq!(workflow.category.as_deref(), Some("std"));
    let crate_dir = root.join("workflows/std/std");
    assert!(crate_dir.join("src/lib.rs").exists());
    assert!(!crate_dir.join("src/main.rs").exists());

    let manifest = fs::read_to_string(crate_dir.join("Cargo.toml"))?;
    assert!(manifest.contains("name = \"lightflow-std\""));
    assert!(manifest.contains("lightflow = { version = \"0.1.1\", path = \"../../..\" }"));
    assert!(manifest.contains("[workspace]"));
    assert!(!manifest.contains("publish = false"));

    Ok(())
}

#[test]
fn lfw_publish_plans_publishable_workflow_crates() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    lfw(&root, ["init"])?;

    let workflow_plan = lfw(&root, ["publish", "lightflow.example"])?;
    assert_eq!(workflow_plan["dry_run"], true);
    assert_eq!(workflow_plan["target"]["workflow_id"], "lightflow.example");
    assert_eq!(workflow_plan["package"], "lightflow-example");
    assert_eq!(workflow_plan["version"], "0.1.0");
    assert_eq!(workflow_plan["publishable"], true);
    assert_eq!(workflow_plan["issues"], serde_json::json!([]));
    assert_eq!(
        workflow_plan["command"],
        serde_json::json!([
            "cargo",
            "publish",
            "--manifest-path",
            "workflows/examples/example/Cargo.toml",
            "--dry-run"
        ])
    );

    let workspace_root_publish = Command::new(env!("CARGO_BIN_EXE_lfw"))
        .arg("publish")
        .current_dir(&root)
        .output()?;
    assert!(!workspace_root_publish.status.success());
    assert!(
        String::from_utf8_lossy(&workspace_root_publish.stderr)
            .contains("Cargo manifest is missing package.name")
    );

    let root_plan = lfw(Path::new(env!("CARGO_MANIFEST_DIR")), ["publish"])?;
    assert_eq!(root_plan["package"], "lightflow");
    assert_eq!(root_plan["publishable"], true);

    let extension = root.join("extensions/lightflow-extension");
    write_publishable_extension_crate(&extension)?;
    let extension_plan = lfw(
        &root,
        ["publish", "--crate", "extensions/lightflow-extension"],
    )?;
    assert_eq!(extension_plan["package"], "lightflow-extension");
    assert_eq!(extension_plan["publishable"], true);
    assert_eq!(
        extension_plan["command"],
        serde_json::json!([
            "cargo",
            "publish",
            "--manifest-path",
            "extensions/lightflow-extension/Cargo.toml",
            "--dry-run"
        ])
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn repository_text_plan_dogfoods_std_workflow() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let service = ApiService::new(root);

    let workflow = service.get_workflow("lightflow.text_plan")?;
    assert_eq!(
        workflow
            .dependencies
            .iter()
            .map(|dependency| (
                dependency.workflow_id.as_str(),
                dependency.version.as_deref()
            ))
            .collect::<Vec<_>>(),
        vec![
            ("lightflow.std", Some("0.1.0")),
            ("lightflow.text_prompt", Some("0.1.0")),
            ("lightflow.text_result", Some("0.1.0")),
        ]
    );
    assert!(
        workflow
            .nodes
            .iter()
            .any(|node| node.id == "identity" && node.workflow_id == "lightflow.std")
    );

    let detail = lfw(root, ["ls", "--detail"])?;
    let text_plan = detail["workflows"]
        .as_array()
        .unwrap()
        .iter()
        .find(|workflow| workflow["id"] == "lightflow.text_plan")
        .expect("detailed list includes lightflow.text_plan");
    assert_eq!(text_plan["nodes"][0]["workflow_id"], "lightflow.std");

    let deps = lfw(root, ["deps", "lightflow.text_plan"])?;
    assert_eq!(deps["complete"], true);
    assert_eq!(
        deps["workflows"],
        serde_json::json!([
            "lightflow.std",
            "lightflow.text_plan",
            "lightflow.text_prompt",
            "lightflow.text_result"
        ])
    );
    assert_eq!(
        deps["workflow_order"],
        serde_json::json!([
            "lightflow.std",
            "lightflow.text_prompt",
            "lightflow.text_result",
            "lightflow.text_plan"
        ])
    );

    Ok(())
}

#[test]
fn lfwx_runs_workflow_and_temporarily_toggles_nodes() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    write_project_specs(&root)?;

    let run = lfwx(&root, ["lightflow.parent", "--input", "in=hello"])?;
    assert_eq!(run["workflow_id"], "lightflow.parent");
    assert_eq!(run["inputs"]["in"], "hello");
    assert_eq!(run["outputs"]["out"], "hello");
    assert_eq!(run["nodes"][0]["node_id"], "nested");
    assert_eq!(run["nodes"][0]["status"], "completed");
    assert!(run["nodes"][0]["duration_ms"].is_number());
    assert_eq!(run["nodes"][0]["attempts"], 1);
    assert_eq!(run["nodes"][1]["node_id"], "sink");
    assert_eq!(run["nodes"][1]["status"], "completed");
    assert!(run["nodes"][1]["duration_ms"].is_number());
    assert_eq!(run["nodes"][1]["attempts"], 1);

    let disabled = lfwx(
        &root,
        [
            "lightflow.parent",
            "--input",
            "in=hello",
            "--disable",
            "nested",
        ],
    )?;
    assert_eq!(disabled["nodes"][0]["node_id"], "nested");
    assert_eq!(disabled["nodes"][0]["status"], "skipped");
    assert_eq!(disabled["nodes"][0]["attempts"], 0);
    assert_eq!(disabled["outputs"]["out"], Value::Null);

    let enabled = lfw(
        &root,
        [
            "run",
            "lightflow.parent",
            "--input",
            "in=hello",
            "--disable",
            "nested",
            "--enable",
            "nested",
        ],
    )?;
    assert_eq!(enabled["nodes"][0]["status"], "completed");
    assert_eq!(enabled["outputs"]["out"], "hello");

    write_workflow_crate(
        &root,
        "lightflow.parent",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.parent")
        .version("0.1.0")
        .name("Parent")
        .input("in", "json")
        .output("out", "json")
        .depends_on("lightflow.child", "0.1.0")
        .disabled_node("nested", "lightflow.child")
        .node("sink", "lightflow.sink")
        .edge("nested", "out", "sink", "in")
        .build()
}
"#,
    )?;
    let default_disabled = lfwx(&root, ["lightflow.parent", "--input", "in=hello"])?;
    assert_eq!(default_disabled["nodes"][0]["status"], "skipped");
    assert_eq!(default_disabled["outputs"]["out"], Value::Null);

    let enabled_from_source = lfwx(
        &root,
        [
            "lightflow.parent",
            "--input",
            "in=hello",
            "--enable",
            "nested",
        ],
    )?;
    assert_eq!(enabled_from_source["nodes"][0]["status"], "completed");
    assert_eq!(enabled_from_source["outputs"]["out"], "hello");

    write_workflow_crate(
        &root,
        "lightflow.io",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.io")
        .version("0.1.0")
        .name("IO")
        .input("text", "text")
        .input("prompt", "text")
        .input("image_path", "path")
        .input("output_path", "path")
        .output("text", "text")
        .output("prompt", "text")
        .output("image_path", "path")
        .output("output_path", "path")
        .build()
}
"#,
    )?;
    let output_path = root.join("generated/image.png");
    let rich_run = lfw(
        &root,
        [
            "run",
            "lightflow.io",
            "--inputs",
            r#"{"text":"from-json"}"#,
            "--prompt",
            "a small house",
            "--image",
            "input/photo.png",
            "--output",
            output_path.to_str().unwrap(),
            "-i",
            "prompt=from-short-input",
        ],
    )?;
    assert_eq!(rich_run["inputs"]["text"], "from-json");
    assert_eq!(rich_run["inputs"]["prompt"], "from-short-input");
    assert_eq!(rich_run["inputs"]["image_path"], "input/photo.png");
    assert_eq!(
        rich_run["inputs"]["output_path"],
        output_path.to_str().unwrap()
    );
    assert_eq!(rich_run["outputs"]["prompt"], "from-short-input");
    assert_eq!(
        rich_run["outputs"]["output_path"],
        output_path.to_str().unwrap()
    );

    let lfx_run = lfx(
        &root,
        [
            "lightflow.io",
            "--text",
            "from-lfx",
            "--input",
            "prompt=from-input",
            "-o",
            "out.png",
        ],
    )?;
    assert_eq!(lfx_run["inputs"]["text"], "from-lfx");
    assert_eq!(lfx_run["inputs"]["prompt"], "from-input");
    assert_eq!(lfx_run["inputs"]["output_path"], "out.png");

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_run_records_trace_and_replays_history() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    write_project_specs(&root)?;

    let run = lfw(&root, ["run", "lightflow.parent", "--input", "in=hello"])?;
    let run_id = run["run_id"]
        .as_str()
        .expect("run output includes run_id")
        .to_owned();
    let run_dir = root.join(".lightflow/runs").join(&run_id);
    assert!(run_dir.join("manifest.json").exists());
    assert!(run_dir.join("execution.json").exists());
    assert!(run_dir.join("events.jsonl").exists());
    assert_eq!(run["workflow_id"], "lightflow.parent");
    assert_eq!(run["outputs"]["out"], "hello");

    let trace = lfw(&root, ["trace", "last"])?;
    assert_eq!(trace["run_id"], run_id);
    assert_eq!(
        trace["manifest"]["stages"][0]["workflow_id"],
        "lightflow.parent"
    );
    assert_eq!(trace["execution"]["workflow_id"], "lightflow.parent");
    assert_eq!(trace["execution"]["outputs"]["out"], "hello");
    assert_eq!(trace["events"][0]["event"], "run_started");
    assert_eq!(trace["events"][1]["event"], "node_completed");
    assert_eq!(trace["events"][1]["node_id"], "nested");
    assert_eq!(trace["events"][1]["workflow_id"], "lightflow.parent");
    assert_eq!(trace["events"][1]["attempts"], 1);
    assert!(trace["events"][1]["duration_ms"].is_number());
    assert_eq!(trace["events"][2]["event"], "node_completed");
    assert_eq!(trace["events"][2]["node_id"], "sink");
    assert_eq!(trace["events"][3]["event"], "run_finished");

    let replay = lfw(&root, ["replay", run_id.as_str()])?;
    assert_eq!(replay["workflow_id"], "lightflow.parent");
    assert_eq!(replay["outputs"]["out"], "hello");
    assert_ne!(replay["run_id"], run_id);

    let replay_trace = lfw(&root, ["trace", replay["run_id"].as_str().unwrap()])?;
    assert_eq!(replay_trace["execution"]["outputs"]["out"], "hello");

    let runs = lfw(&root, ["runs", "list"])?;
    assert_eq!(runs["last"], replay["run_id"]);
    let runs_array = runs["runs"].as_array().unwrap();
    assert_eq!(runs_array.len(), 2);
    assert_eq!(runs_array[0]["run_id"], replay["run_id"]);
    assert_eq!(runs_array[0]["status"], "completed");
    assert_eq!(runs_array[0]["workflow_id"], "lightflow.parent");
    assert_eq!(runs_array[1]["run_id"], run_id);

    let run_detail = lfw(&root, ["runs", "get", run_id.as_str()])?;
    assert_eq!(run_detail["run_id"], run_id);
    assert_eq!(run_detail["execution"]["outputs"]["out"], "hello");

    let removed = lfw(&root, ["runs", "rm", run_id.as_str()])?;
    assert_eq!(removed["removed"], true);
    assert!(!root.join(".lightflow/runs").join(&run_id).exists());
    let runs_after_remove = lfw(&root, ["runs", "list"])?;
    assert_eq!(runs_after_remove["runs"].as_array().unwrap().len(), 1);

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_run_applies_patch_files_at_node_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    write_project_specs(&root)?;
    write_workflow_crate(
        &root,
        "lightflow.replacement",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.replacement")
        .version("0.1.0")
        .name("Replacement")
        .input("in", "json")
        .output("out", "json")
        .build()
}
"#,
    )?;
    write_workflow_crate(
        &root,
        "lightflow.fallback",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.fallback")
        .version("0.1.0")
        .name("Fallback")
        .input("in", "json")
        .output("out", "json")
        .build()
}
"#,
    )?;

    let patch_path = root.join("patch.json");
    fs::write(
        &patch_path,
        r#"{
  "nodes": {
    "nested": {
      "replace_with": "lightflow.replacement",
      "retry": 2,
      "timeout_ms": 1000
    }
  }
}
"#,
    )?;
    let saved_patch = lfw(
        &root,
        [
            "patch",
            "save",
            "qa-debug",
            &format!("@{}", patch_path.display()),
        ],
    )?;
    assert_eq!(saved_patch["saved"], true);
    assert_eq!(saved_patch["name"], "qa-debug");
    assert!(root.join(".lightflow/patches/qa-debug.json").exists());

    let patches = lfw(&root, ["patch", "list"])?;
    assert_eq!(patches["patches"][0]["name"], "qa-debug");

    let registered_patch = lfw(&root, ["patch", "get", "qa-debug"])?;
    assert_eq!(
        registered_patch["patch"]["nodes"]["nested"]["replace_with"],
        "lightflow.replacement"
    );
    let validated_patch = lfw(&root, ["patch", "validate", "qa-debug"])?;
    assert_eq!(validated_patch["valid"], true);
    assert_eq!(
        validated_patch["patch"]["nodes"]["nested"]["replace_with"],
        "lightflow.replacement"
    );

    let patched = lfw(
        &root,
        [
            "run",
            "lightflow.parent",
            "--input",
            "in=hello",
            "--patch",
            "qa-debug",
        ],
    )?;
    assert_eq!(patched["outputs"]["out"], "hello");
    assert_eq!(patched["nodes"][0]["node_id"], "nested");
    assert_eq!(patched["nodes"][0]["workflow_id"], "lightflow.child");
    assert_eq!(
        patched["nodes"][0]["selected_workflow_id"],
        "lightflow.replacement"
    );
    assert_eq!(patched["nodes"][0]["attempts"], 1);

    let trace = lfw(&root, ["trace", patched["run_id"].as_str().unwrap()])?;
    assert_eq!(
        trace["manifest"]["stages"][0]["execution"]["patch"]["nodes"]["nested"]["replace_with"],
        "lightflow.replacement"
    );
    assert_eq!(
        trace["manifest"]["stages"][0]["execution"]["patch"]["nodes"]["nested"]["retry"],
        2
    );
    assert_eq!(trace["events"][1]["event"], "node_completed");
    assert_eq!(trace["events"][1]["node_id"], "nested");
    assert_eq!(
        trace["events"][1]["selected_workflow_id"],
        "lightflow.replacement"
    );

    let fallback_patch = r#"{
  "nodes": {
    "nested": {
      "disable": true,
      "fallback_workflow_id": "lightflow.fallback"
    }
  }
}"#;
    let fallback = lfw(
        &root,
        [
            "run",
            "lightflow.parent",
            "--input",
            "in=hello",
            "--patch",
            fallback_patch,
        ],
    )?;
    assert_eq!(fallback["nodes"][0]["status"], "completed");
    assert_eq!(
        fallback["nodes"][0]["selected_workflow_id"],
        "lightflow.fallback"
    );
    assert_eq!(fallback["outputs"]["out"], "hello");

    let disabled = lfw(
        &root,
        [
            "run",
            "lightflow.parent",
            "--input",
            "in=hello",
            "--patch",
            r#"{"nodes":{"nested":{"disable":true}}}"#,
        ],
    )?;
    assert_eq!(disabled["nodes"][0]["status"], "skipped");
    assert!(disabled["nodes"][0]["duration_ms"].is_number());
    assert_eq!(disabled["nodes"][0]["attempts"], 0);
    assert_eq!(disabled["outputs"]["out"], Value::Null);

    let disabled_trace = lfw(&root, ["trace", disabled["run_id"].as_str().unwrap()])?;
    assert_eq!(disabled_trace["events"][1]["event"], "node_skipped");
    assert_eq!(disabled_trace["events"][1]["node_id"], "nested");

    let enabled = lfw(
        &root,
        [
            "run",
            "lightflow.parent",
            "--input",
            "in=hello",
            "--disable",
            "nested",
            "--patch",
            r#"{"nodes":{"nested":{"enable":true}}}"#,
        ],
    )?;
    assert_eq!(enabled["nodes"][0]["status"], "completed");
    assert_eq!(enabled["outputs"]["out"], "hello");

    let removed_patch = lfw(&root, ["patch", "rm", "qa-debug"])?;
    assert_eq!(removed_patch["removed"], true);
    assert!(!root.join(".lightflow/patches/qa-debug.json").exists());

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_run_chains_workflows_with_pipe() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    fs::write(
        root.join("Cargo.toml"),
        r#"[workspace]
resolver = "3"
members = ["workflows/*/*"]

[workspace.dependencies]
lightflow = { path = "." }
"#,
    )?;
    write_workflow_crate(
        &root,
        "lightflow.first",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.first")
        .version("0.1.0")
        .name("First")
        .input("text", "text")
        .output("text", "text")
        .build()
}
"#,
    )?;
    write_workflow_crate(
        &root,
        "lightflow.second",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.second")
        .version("0.1.0")
        .name("Second")
        .input("text", "text")
        .output("text", "text")
        .build()
}
"#,
    )?;

    let chained = lfw(
        &root,
        [
            "run",
            "lightflow.first",
            "-i",
            "text=hello",
            "|",
            "lightflow.second",
        ],
    )?;
    assert_eq!(chained["pipeline"], true);
    assert_eq!(chained["outputs"]["text"], "hello");
    assert_eq!(chained["stages"][0]["workflow_id"], "lightflow.first");
    assert_eq!(chained["stages"][1]["workflow_id"], "lightflow.second");
    assert_eq!(chained["stages"][1]["inputs"]["text"], "hello");

    let overridden = lfw(
        &root,
        [
            "run",
            "lightflow.first",
            "-i",
            "text=hello",
            "|",
            "lightflow.second",
            "-i",
            "text=override",
        ],
    )?;
    assert_eq!(overridden["outputs"]["text"], "override");
    assert_eq!(overridden["stages"][1]["inputs"]["text"], "override");

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_run_executes_if_node_selected_branch() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    fs::write(
        root.join("Cargo.toml"),
        r#"[workspace]
resolver = "3"
members = ["workflows/*/*"]

[workspace.dependencies]
lightflow = { path = "." }
"#,
    )?;
    write_workflow_crate(
        &root,
        "lightflow.then_branch",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.then_branch")
        .version("0.1.0")
        .name("Then Branch")
        .input("flag", "boolean")
        .input("value", "text")
        .output("value", "text")
        .build()
}
"#,
    )?;
    write_workflow_crate(
        &root,
        "lightflow.else_branch",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.else_branch")
        .version("0.1.0")
        .name("Else Branch")
        .input("flag", "boolean")
        .input("value", "text")
        .output("value", "text")
        .build()
}
"#,
    )?;
    write_workflow_crate(
        &root,
        "lightflow.conditional",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.conditional")
        .version("0.1.0")
        .name("Conditional")
        .input("flag", "boolean")
        .input("value", "text")
        .output("value", "text")
        .if_node("gate", "flag", true, "lightflow.then_branch", "lightflow.else_branch")
        .build()
}
"#,
    )?;

    let then_run = lfw(
        &root,
        [
            "run",
            "lightflow.conditional",
            "-i",
            "flag=true",
            "-i",
            "value=then",
        ],
    )?;
    assert_eq!(then_run["outputs"]["value"], "then");
    assert_eq!(
        then_run["nodes"][0]["selected_workflow_id"],
        "lightflow.then_branch"
    );

    let else_run = lfw(
        &root,
        [
            "run",
            "lightflow.conditional",
            "-i",
            "flag=false",
            "-i",
            "value=else",
        ],
    )?;
    assert_eq!(else_run["outputs"]["value"], "else");
    assert_eq!(
        else_run["nodes"][0]["selected_workflow_id"],
        "lightflow.else_branch"
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_run_rejects_unknown_leaf_runtime() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    write_workflow_crate(
        &root,
        "lightflow.unknown_runtime",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.unknown_runtime")
        .version("0.1.0")
        .name("Unknown Runtime")
        .input("prompt", "text")
        .output("image", "artifact")
        .runtime("runtime", "lightflow.image.inpaint")
        .build()
}
"#,
    )?;

    let output = Command::new(env!("CARGO_BIN_EXE_lfw"))
        .args(["run", "lightflow.unknown_runtime", "--input", "prompt=test"])
        .current_dir(&root)
        .env("HOME", &root)
        .env("SHELL", "/bin/zsh")
        .env("XDG_CONFIG_HOME", root.join(".test-xdg/config"))
        .env("XDG_DATA_HOME", root.join(".test-xdg/data"))
        .env_remove("LFW_PATH")
        .output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("has no executor"));
    assert!(stderr.contains("lightflow.image.inpaint"));
    assert!(stderr.contains("run_id: run-"));
    assert!(stderr.contains("trace_path:"));

    let trace = lfw(&root, ["trace", "last"])?;
    assert_eq!(trace["manifest"]["status"], "failed");
    assert_eq!(trace["execution"]["status"], "failed");
    assert!(
        trace["execution"]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("lightflow.image.inpaint")
    );
    assert_eq!(trace["events"][0]["event"], "run_started");
    assert_eq!(trace["events"][1]["event"], "run_failed");
    assert!(
        trace["events"][1]["error"]["message"]
            .as_str()
            .unwrap()
            .contains("has no executor")
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}
