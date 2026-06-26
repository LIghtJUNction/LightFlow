mod support;

use lightflow::api::ApiService;
use serde_json::Value;
use std::fs;
use std::os::unix::fs::PermissionsExt;
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
    let crate_dir = root.join("projects/lightflow-std/workflows/std/std");
    assert!(crate_dir.join("src/lib.rs").exists());
    assert!(!crate_dir.join("src/main.rs").exists());

    let manifest = fs::read_to_string(crate_dir.join("Cargo.toml"))?;
    assert!(manifest.contains("name = \"lightflow-std\""));
    assert!(manifest.contains("lightflow = { workspace = true }"));
    assert!(manifest.contains("repository = \"https://github.com/lightjunction/lightflow-std\""));
    assert!(!manifest.contains("publish = false"));

    Ok(())
}

#[test]
fn repository_std_project_workflows_are_discovered_by_default()
-> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let service = ApiService::new(root);

    assert_eq!(
        service.get_workflow("lightflow.std")?.category.as_deref(),
        Some("std")
    );
    assert!(service.get_workflow("lightflow.text.template").is_ok());

    Ok(())
}

#[test]
fn sibling_project_workflows_are_discovered_from_explicit_paths()
-> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let lfw_path = format!(
        "{}:{}",
        root.join("projects/lightflow-flux").display(),
        root.join("projects/lightflow-rig").display()
    );
    let output = lfw_with_env_values(root, ["list", "--brief"], [("LFW_PATH", lfw_path.as_str())])?;
    let workflow_ids = output["workflows"]
        .as_array()
        .expect("workflow list")
        .iter()
        .filter_map(|workflow| workflow["id"].as_str())
        .collect::<Vec<_>>();

    assert!(workflow_ids.contains(&"lightflow.flux.text_to_image"));
    assert!(workflow_ids.contains(&"lightflow.rig.llm"));

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
    assert_eq!(workflow_plan["publishable"], false);
    assert!(
        workflow_plan["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue == "workflow.description contains unresolved TODO")
    );
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
    complete_generated_workflow_metadata(&root, "examples", "example")?;
    let workflow_plan = lfw(&root, ["publish", "lightflow.example"])?;
    assert_eq!(workflow_plan["publishable"], true);
    assert_eq!(workflow_plan["issues"], serde_json::json!([]));

    let git_init = Command::new("git")
        .arg("init")
        .current_dir(&root)
        .output()?;
    assert!(
        git_init.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&git_init.stderr)
    );
    let git_add = Command::new("git")
        .args(["add", "."])
        .current_dir(&root)
        .output()?;
    assert!(
        git_add.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&git_add.stderr)
    );
    let git_commit = Command::new("git")
        .args([
            "-c",
            "user.email=lightflow@example.invalid",
            "-c",
            "user.name=LightFlow Test",
            "commit",
            "-m",
            "fixture",
        ])
        .current_dir(&root)
        .output()?;
    assert!(
        git_commit.status.success(),
        "stderr:\n{}",
        String::from_utf8_lossy(&git_commit.stderr)
    );

    let fake_bin = root.join("fake-bin");
    fs::create_dir_all(&fake_bin)?;
    let cargo_log = root.join("cargo-publish.log");
    let cargo_path = fake_bin.join("cargo");
    fs::write(
        &cargo_path,
        format!(
            "#!/bin/sh\nprintf '%s\\n' \"$*\" >> '{}'\n",
            cargo_log.display()
        ),
    )?;
    let mut permissions = fs::metadata(&cargo_path)?.permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(&cargo_path, permissions)?;
    let path = format!(
        "{}:{}",
        fake_bin.display(),
        std::env::var("PATH").unwrap_or_default()
    );
    let applied = Command::new(env!("CARGO_BIN_EXE_lfw"))
        .args(["publish", "lightflow.example", "--apply"])
        .current_dir(&root)
        .env("HOME", &root)
        .env("SHELL", "/bin/zsh")
        .env("XDG_CONFIG_HOME", root.join(".test-xdg/config"))
        .env("XDG_DATA_HOME", root.join(".test-xdg/data"))
        .env("PATH", path)
        .env_remove("LFW_PATH")
        .output()?;
    assert!(
        applied.status.success(),
        "stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&applied.stdout),
        String::from_utf8_lossy(&applied.stderr)
    );
    let applied_json: serde_json::Value = serde_json::from_slice(&applied.stdout)?;
    assert_eq!(applied_json["dry_run"], false);
    assert_eq!(
        applied_json["executed"].as_array().expect("executed").len(),
        2
    );
    assert_eq!(
        applied_json["preflight_commands"][0],
        serde_json::json!([
            "cargo",
            "publish",
            "--manifest-path",
            "workflows/examples/example/Cargo.toml",
            "--dry-run"
        ])
    );
    let cargo_lines = fs::read_to_string(&cargo_log)?
        .lines()
        .map(ToOwned::to_owned)
        .collect::<Vec<_>>();
    assert_eq!(cargo_lines.len(), 2);
    assert!(cargo_lines[0].contains("--dry-run"));
    assert!(!cargo_lines[1].contains("--dry-run"));

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
    assert_eq!(run["model_locks"], serde_json::json!([]));

    let trace = lfw(&root, ["trace", "last"])?;
    assert_eq!(trace["run_id"], run_id);
    assert_eq!(
        trace["manifest"]["stages"][0]["workflow_id"],
        "lightflow.parent"
    );
    assert_eq!(trace["execution"]["workflow_id"], "lightflow.parent");
    assert_eq!(trace["execution"]["outputs"]["out"], "hello");
    assert_eq!(trace["events"][0]["event"], "run_started");
    assert_eq!(trace["events"][0]["surface"], "cli");
    assert_eq!(trace["events"][1]["event"], "node_completed");
    assert_eq!(trace["events"][1]["node_id"], "nested");
    assert_eq!(trace["events"][1]["workflow_id"], "lightflow.parent");
    assert_eq!(trace["events"][1]["attempts"], 1);
    assert!(trace["events"][1]["duration_ms"].is_number());
    assert_eq!(
        trace["execution"]["nodes"][0]["runtime"]["executor_id"],
        "passthrough"
    );
    assert_eq!(
        trace["execution"]["nodes"][0]["runtime"]["data_policy"],
        "json_values"
    );
    assert_eq!(
        trace["execution"]["nodes"][0]["runtime"]["capabilities"],
        serde_json::json!(["lightflow.data.copy"])
    );
    assert_eq!(trace["events"][1]["runtime"]["executor_id"], "passthrough");
    assert_eq!(trace["events"][1]["runtime"]["data_policy"], "json_values");
    assert_eq!(trace["events"][2]["event"], "node_completed");
    assert_eq!(trace["events"][2]["node_id"], "sink");
    assert_eq!(trace["events"][3]["event"], "run_finished");
    assert_eq!(trace["events"][3]["surface"], "cli");

    let replay = lfw(&root, ["replay"])?;
    assert_eq!(replay["workflow_id"], "lightflow.parent");
    assert_eq!(replay["outputs"]["out"], "hello");
    assert_ne!(replay["run_id"], run_id);
    assert_eq!(replay["replayed_from"], "last");
    assert_eq!(replay["replay"]["runtime_changed"], false);
    assert_eq!(replay["replay"]["model_lock_changed"], false);
    assert_eq!(
        replay["replay"]["original_runtime"],
        replay["replay"]["replayed_runtime"]
    );
    assert_eq!(
        replay["replay"]["original_model_locks"],
        replay["replay"]["replayed_model_locks"]
    );
    assert_eq!(
        replay["replay"]["original_runtime"][0]["runtime"]["executor_id"],
        "passthrough"
    );

    let replay_trace = lfw(&root, ["trace", replay["run_id"].as_str().unwrap()])?;
    assert_eq!(replay_trace["execution"]["outputs"]["out"], "hello");
    assert_eq!(
        replay_trace["execution"]["replay"]["runtime_changed"],
        false
    );
    assert_eq!(replay_trace["events"][0]["surface"], "cli");

    let runs = lfw(&root, ["runs", "list"])?;
    assert_eq!(runs["last"], replay["run_id"]);
    assert_eq!(runs["total"], 2);
    assert_eq!(runs["completed_count"], 2);
    assert_eq!(runs["failed_count"], 0);
    assert_eq!(runs["unknown_count"], 0);
    assert_eq!(runs["unknown_run_ids"], serde_json::json!([]));
    let runs_array = runs["runs"].as_array().unwrap();
    assert_eq!(runs_array.len(), 2);
    assert_eq!(runs_array[0]["run_id"], replay["run_id"]);
    assert_eq!(runs_array[0]["status"], "completed");
    assert!(runs_array[0]["duration_ms"].is_number());
    assert_eq!(runs_array[0]["surface"], "cli");
    assert_eq!(runs_array[0]["workflow_id"], "lightflow.parent");
    assert_eq!(
        runs_array[0]["workflow_ids"],
        serde_json::json!(["lightflow.parent"])
    );
    assert_eq!(runs_array[1]["run_id"], run_id);

    let run_detail = lfw(&root, ["runs", "get", run_id.as_str()])?;
    assert_eq!(run_detail["run_id"], run_id);
    assert_eq!(run_detail["execution"]["outputs"]["out"], "hello");

    let namespaced_replay = lfw(&root, ["runs", "replay", run_id.as_str()])?;
    assert_eq!(namespaced_replay["workflow_id"], "lightflow.parent");
    assert_eq!(namespaced_replay["outputs"]["out"], "hello");
    assert_eq!(namespaced_replay["replayed_from"], run_id);
    assert_ne!(namespaced_replay["run_id"], run_id);
    assert_eq!(namespaced_replay["replay"]["runtime_changed"], false);
    let namespaced_replay_trace = lfw(
        &root,
        [
            "runs",
            "get",
            namespaced_replay["run_id"].as_str().expect("replay run id"),
        ],
    )?;
    assert_eq!(namespaced_replay_trace["events"][0]["surface"], "cli");

    let removed = lfw(&root, ["runs", "rm", run_id.as_str()])?;
    assert_eq!(removed["removed"], true);
    assert!(!root.join(".lightflow/runs").join(&run_id).exists());
    let runs_after_remove = lfw(&root, ["runs", "list"])?;
    assert_eq!(runs_after_remove["runs"].as_array().unwrap().len(), 2);

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_replay_reports_model_lock_drift() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    write_project_specs(&root)?;
    write_workflow_crate(
        &root,
        "lightflow.model_passthrough",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.model_passthrough")
        .version("0.1.0")
        .name("Model Passthrough")
        .input("value", "json")
        .output("value", "json")
        .model("weights", "text-to-image")
        .build()
}
"#,
    )?;
    write_workflow_crate(
        &root,
        "lightflow.echo_value",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.echo_value")
        .version("0.1.0")
        .name("Echo Value")
        .input("value", "json")
        .output("value", "json")
        .build()
}
"#,
    )?;
    let model_path = root.join("models/tiny.gguf");
    fs::create_dir_all(model_path.parent().unwrap())?;
    fs::write(&model_path, b"tiny")?;
    write_model_lock(&root, &model_path, "abc123")?;

    let run = lfw(
        &root,
        [
            "run",
            "lightflow.model_passthrough",
            "--input",
            "value=hello",
        ],
    )?;
    let run_id = run["run_id"].as_str().expect("run id").to_owned();
    assert_eq!(run["outputs"]["value"], "hello");
    assert_eq!(
        run["model_locks"][0]["workflow_id"],
        "lightflow.model_passthrough"
    );
    assert_eq!(run["model_locks"][0]["requirement_id"], "weights");
    assert_eq!(run["model_locks"][0]["lock"]["status"], "available");
    assert_eq!(run["model_locks"][0]["lock"]["sha256"], "abc123");

    write_model_lock(&root, &model_path, "def456")?;
    let replay = lfw(&root, ["replay", run_id.as_str()])?;
    assert_eq!(replay["outputs"]["value"], "hello");
    assert_eq!(replay["replay"]["runtime_changed"], false);
    assert_eq!(replay["replay"]["model_lock_changed"], true);
    assert_eq!(
        replay["replay"]["original_model_locks"][0]["lock"]["sha256"],
        "abc123"
    );
    assert_eq!(
        replay["replay"]["replayed_model_locks"][0]["lock"]["sha256"],
        "def456"
    );

    let replay_trace = lfw(&root, ["trace", replay["run_id"].as_str().unwrap()])?;
    assert_eq!(
        replay_trace["execution"]["replay"]["model_lock_changed"],
        true
    );

    write_model_lock(&root, &model_path, "abc123")?;
    let pipeline = lfw(
        &root,
        [
            "run",
            "lightflow.echo_value",
            "--input",
            "value=hello",
            "|",
            "lightflow.model_passthrough",
        ],
    )?;
    let pipeline_run_id = pipeline["run_id"].as_str().expect("pipeline run id");
    assert_eq!(pipeline["model_locks"][0]["stage_index"], 1);
    assert_eq!(
        pipeline["model_locks"][0]["workflow_id"],
        "lightflow.model_passthrough"
    );

    write_model_lock(&root, &model_path, "def456")?;
    let pipeline_replay = lfw(&root, ["replay", pipeline_run_id])?;
    assert_eq!(pipeline_replay["replay"]["model_lock_changed"], true);
    assert_eq!(
        pipeline_replay["replay"]["original_model_locks"][0]["stage_index"],
        1
    );
    assert_eq!(
        pipeline_replay["replay"]["replayed_model_locks"][0]["stage_index"],
        1
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

fn write_model_lock(
    root: &Path,
    model_path: &Path,
    sha256: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    fs::write(
        root.join("lfw.lock"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "version": 2,
            "models": {
                "lightflow.model_passthrough::weights": {
                    "requirement_id": "weights",
                    "variant_id": "tiny",
                    "repo": "example/tiny",
                    "file": "tiny.gguf",
                    "format": "gguf",
                    "sha256": sha256,
                    "hash_algorithm": "sha256",
                    "size_bytes": 4,
                    "snapshot_revision": "rev1",
                    "local_paths": [model_path],
                }
            }
        }))?,
    )?;
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
    write_workflow_crate(
        &root,
        "lightflow.no_output",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.no_output")
        .version("0.1.0")
        .name("No Output")
        .input("in", "json")
        .build()
}
"#,
    )?;
    write_workflow_crate(
        &root,
        "lightflow.extra_required",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.extra_required")
        .version("0.1.0")
        .name("Extra Required")
        .input("in", "json")
        .input("extra", "json")
        .input_required("extra", true)
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
    assert_eq!(validated_patch["issues"], serde_json::json!([]));
    assert_eq!(
        validated_patch["patch"]["nodes"]["nested"]["replace_with"],
        "lightflow.replacement"
    );
    let selected_validation = lfw(
        &root,
        [
            "patch",
            "validate",
            "qa-debug",
            "--workflow",
            "lightflow.parent",
        ],
    )?;
    assert_eq!(selected_validation["valid"], true);

    lfw(
        &root,
        [
            "patch",
            "save",
            "bad-debug",
            r#"{"nodes":{"missing":{"replace_with":"lightflow.nope","retry":0}}}"#,
        ],
    )?;
    let invalid_patch = lfw_command(&root)
        .args(["patch", "validate", "bad-debug"])
        .output()?;
    assert!(!invalid_patch.status.success());
    let invalid_stderr = String::from_utf8_lossy(&invalid_patch.stderr);
    assert!(
        invalid_stderr.contains("patch node missing does not match any available workflow node"),
        "stderr:\n{invalid_stderr}"
    );
    assert!(
        invalid_stderr
            .contains("patch node missing replacement workflow lightflow.nope is not available"),
        "stderr:\n{invalid_stderr}"
    );
    assert!(
        invalid_stderr.contains("patch node missing retry must be greater than zero"),
        "stderr:\n{invalid_stderr}"
    );
    let bad_loop = lfw_command(&root).args(["loop", "check"]).output()?;
    assert!(!bad_loop.status.success());
    let bad_loop_stderr = String::from_utf8_lossy(&bad_loop.stderr);
    assert!(
        bad_loop_stderr.contains("saved patches are invalid: bad-debug"),
        "stderr:\n{bad_loop_stderr}"
    );
    assert!(
        bad_loop_stderr.contains("patch node missing does not match any available workflow node"),
        "stderr:\n{bad_loop_stderr}"
    );
    lfw(&root, ["patch", "rm", "bad-debug"])?;

    let wrong_workflow_patch = lfw_command(&root)
        .args([
            "run",
            "lightflow.child",
            "--input",
            "in=hello",
            "--patch",
            r#"{"nodes":{"nested":{"disable":true}}}"#,
        ])
        .output()?;
    assert!(!wrong_workflow_patch.status.success());
    let wrong_workflow_stderr = String::from_utf8_lossy(&wrong_workflow_patch.stderr);
    assert!(
        wrong_workflow_stderr
            .contains("patch node nested does not match any node in workflow lightflow.child"),
        "stderr:\n{wrong_workflow_stderr}"
    );

    let unknown_patch_node = lfw_command(&root)
        .args([
            "run",
            "lightflow.parent",
            "--input",
            "in=hello",
            "--patch",
            r#"{"nodes":{"missing":{"disable":true}}}"#,
        ])
        .output()?;
    assert!(!unknown_patch_node.status.success());
    let unknown_patch_stderr = String::from_utf8_lossy(&unknown_patch_node.stderr);
    assert!(
        unknown_patch_stderr
            .contains("patch node missing does not match any node in workflow lightflow.parent"),
        "stderr:\n{unknown_patch_stderr}"
    );

    let unknown_toggle = lfw_command(&root)
        .args([
            "run",
            "lightflow.parent",
            "--input",
            "in=hello",
            "--disable",
            "missing",
        ])
        .output()?;
    assert!(!unknown_toggle.status.success());
    let unknown_toggle_stderr = String::from_utf8_lossy(&unknown_toggle.stderr);
    assert!(
        unknown_toggle_stderr
            .contains("disabled node missing does not match any node in workflow lightflow.parent"),
        "stderr:\n{unknown_toggle_stderr}"
    );

    let incompatible_replacement = lfw_command(&root)
        .args([
            "run",
            "lightflow.parent",
            "--input",
            "in=hello",
            "--patch",
            r#"{"nodes":{"nested":{"replace_with":"lightflow.no_output"}}}"#,
        ])
        .output()?;
    assert!(!incompatible_replacement.status.success());
    let incompatible_replacement_stderr = String::from_utf8_lossy(&incompatible_replacement.stderr);
    assert!(
        incompatible_replacement_stderr.contains(
            "patch node nested replacement workflow lightflow.no_output is missing output port out"
        ),
        "stderr:\n{incompatible_replacement_stderr}"
    );

    let incompatible_preflight = lfw_command(&root)
        .args([
            "patch",
            "validate",
            r#"{"nodes":{"nested":{"replace_with":"lightflow.no_output"}}}"#,
            "--workflow",
            "lightflow.parent",
        ])
        .output()?;
    assert!(!incompatible_preflight.status.success());
    let incompatible_preflight_stderr = String::from_utf8_lossy(&incompatible_preflight.stderr);
    assert!(
        incompatible_preflight_stderr.contains(
            "patch node nested replacement workflow lightflow.no_output is missing output port out"
        ),
        "stderr:\n{incompatible_preflight_stderr}"
    );

    lfw(
        &root,
        [
            "patch",
            "save",
            "wrong-shape",
            r#"{"nodes":{"nested":{"replace_with":"lightflow.no_output"}}}"#,
        ],
    )?;
    let selected_loop_output = lfw_command(&root)
        .args(["loop", "check", "lightflow.parent"])
        .output()?;
    assert!(!selected_loop_output.status.success());
    let selected_loop = serde_json::from_slice::<serde_json::Value>(&selected_loop_output.stderr)?;
    assert!(
        selected_loop["checks"]
            .as_array()
            .expect("selected loop checks")
            .iter()
            .any(|check| {
                check["id"] == "loop.selected.patches"
                    && check["status"] == "warning"
                    && check["message"].as_str().unwrap().contains("wrong-shape")
                    && check["message"]
                        .as_str()
                        .unwrap()
                        .contains("missing output port out")
            }),
        "selected loop checks:\n{selected_loop}"
    );
    lfw(&root, ["patch", "rm", "wrong-shape"])?;

    let unsatisfied_extra_input = lfw_command(&root)
        .args([
            "run",
            "lightflow.parent",
            "--input",
            "in=hello",
            "--patch",
            r#"{"nodes":{"nested":{"replace_with":"lightflow.extra_required"}}}"#,
        ])
        .output()?;
    assert!(!unsatisfied_extra_input.status.success());
    let unsatisfied_extra_input_stderr = String::from_utf8_lossy(&unsatisfied_extra_input.stderr);
    assert!(
        unsatisfied_extra_input_stderr.contains(
            "patch node nested replacement workflow lightflow.extra_required has unsatisfied required input port extra"
        ),
        "stderr:\n{unsatisfied_extra_input_stderr}"
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
fn patch_registry_rejects_path_traversal_names() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    let patch = r#"{"nodes":{}}"#;

    for args in [
        vec!["patch", "get", "../outside"],
        vec!["patch", "save", "../outside", patch],
        vec!["patch", "validate", "../outside"],
        vec!["patch", "rm", "../outside"],
    ] {
        let output = lfw_command(&root).args(args).output()?;
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("patch name must be a single non-empty file name"),
            "stderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    assert!(!root.join(".lightflow/outside.json").exists());
    assert!(!root.join("outside.json").exists());

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
    write_workflow_crate(
        &root,
        "lightflow.broken",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.broken")
        .version("0.1.0")
        .name("Broken")
        .input("text", "text")
        .output("text", "text")
        .runtime("runtime", "lightflow.missing.executor")
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
    let chained_run_id = chained["run_id"].as_str().expect("pipeline run id");
    let chained_trace = lfw(&root, ["trace", chained_run_id])?;
    assert_eq!(
        chained_trace["manifest"]["stage_input_resolution"],
        "resolved"
    );
    assert_eq!(
        chained_trace["manifest"]["stages"][1]["execution"]["inputs"]["text"],
        "hello"
    );
    let replayed = lfw(&root, ["replay", chained_run_id])?;
    assert_eq!(replayed["outputs"]["text"], "hello");
    let replayed_trace = lfw(&root, ["trace", replayed["run_id"].as_str().unwrap()])?;
    assert_eq!(
        replayed_trace["manifest"]["stages"][1]["execution"]["inputs"]["text"],
        "hello"
    );
    let runs = lfw(&root, ["runs", "list"])?;
    assert!(
        runs["runs"].as_array().expect("runs").iter().any(|run| {
            run["run_id"] == chained_run_id
                && run["duration_ms"].is_number()
                && run["surface"] == "cli"
                && run["workflow_id"] == "lightflow.first"
                && run["workflow_ids"] == serde_json::json!(["lightflow.first", "lightflow.second"])
        }),
        "runs:\n{runs}"
    );

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

    let failed = lfw_command(&root)
        .args([
            "run",
            "lightflow.first",
            "-i",
            "text=hello",
            "|",
            "lightflow.broken",
        ])
        .output()?;
    assert!(!failed.status.success());
    let failed_trace = lfw(&root, ["trace", "last"])?;
    assert_eq!(failed_trace["manifest"]["status"], "failed");
    assert_eq!(
        failed_trace["manifest"]["stages"][1]["execution"]["inputs"]["text"],
        "hello"
    );
    assert_eq!(
        failed_trace["execution"]["partial_execution"]["stages"][0]["workflow_id"],
        "lightflow.first"
    );
    assert_eq!(
        failed_trace["execution"]["partial_execution"]["outputs"]["text"],
        "hello"
    );
    assert!(
        failed_trace["events"]
            .as_array()
            .expect("failed events")
            .iter()
            .any(|event| {
                event["event"] == "stage_completed"
                    && event["stage_index"] == 0
                    && event["workflow_id"] == "lightflow.first"
                    && event["outputs"]["text"] == "hello"
            }),
        "failed trace events:\n{failed_trace}"
    );
    assert_eq!(
        failed_trace["events"]
            .as_array()
            .expect("failed events")
            .last()
            .expect("failed event tail")["event"],
        "run_failed"
    );

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

fn complete_generated_workflow_metadata(
    root: &Path,
    category: &str,
    name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = root
        .join("workflows")
        .join(category)
        .join(name)
        .join("src/lib.rs");
    let source = fs::read_to_string(&path)?
        .replace(
            "TODO: describe this workflow.",
            "Publishes a completed test workflow.",
        )
        .replace(
            "TODO: describe the input value.",
            "Input value for the test workflow.",
        )
        .replace(
            "TODO: describe the output value.",
            "Output value from the test workflow.",
        )
        .replace(
            "TODO: describe the runtime input value.",
            "Runtime input value for the test workflow.",
        )
        .replace(
            "TODO: describe the runtime output value.",
            "Runtime output value from the test workflow.",
        );
    fs::write(path, source)?;
    Ok(())
}
