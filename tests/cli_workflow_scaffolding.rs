#![allow(unused_imports)]

mod cli_project_support;
mod support;

use cli_project_support::*;
use lightflow::api::{ApiService, WorkflowPublishOptions};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;
use support::*;

#[test]
fn lfw_init_and_add_create_rust_workflow_files() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;

    let init = lfw(&root, ["init"])?;
    assert!(
        init["created"]
            .as_array()
            .unwrap()
            .iter()
            .any(|path| path.as_str().unwrap().ends_with("Cargo.toml"))
    );
    assert!(init["created"].as_array().unwrap().iter().any(|path| {
        path.as_str()
            .unwrap()
            .ends_with("examples/example/src/lib.rs")
    }));
    assert!(init["created"].as_array().unwrap().iter().any(|path| {
        path.as_str()
            .unwrap()
            .ends_with("examples/example/.agent/skills/lightflow-example/SKILL.md")
    }));

    let missing_category = Command::new(env!("CARGO_BIN_EXE_lfw"))
        .args(["new", "missing_category"])
        .current_dir(&root)
        .output()?;
    assert!(!missing_category.status.success());
    assert!(
        String::from_utf8_lossy(&missing_category.stderr)
            .contains("lfw new requires --category <name>")
    );

    let added = lfw(
        &root,
        [
            "new",
            "extra",
            "--category",
            "examples",
            "--name",
            "Extra Workflow",
        ],
    )?;
    assert_eq!(added["workflow_id"], "lightflow.extra");
    assert_eq!(added["category"], "examples");
    let manifest = fs::read_to_string(root.join(".lightflow/workflows/examples/extra/Cargo.toml"))?;
    assert!(manifest.contains("name = \"lightflow-extra\""));
    assert!(!manifest.contains("publish = false"));
    let workspace = fs::read_to_string(root.join("Cargo.toml"))?;
    assert!(workspace.contains("-lightflow-host\""));
    assert!(workspace.contains("version = \"0.0.0\""));
    assert!(workspace.contains("publish = false"));
    assert!(workspace.contains("path = \".lightflow/workspace.rs\""));
    assert!(root.join(".lightflow/workspace.rs").is_file());
    assert!(workspace.contains(&format!("lightflow = {:?}", env!("CARGO_PKG_VERSION"))));
    let gitignore = fs::read_to_string(root.join(".gitignore"))?;
    assert!(gitignore.contains("/target/"));
    assert!(gitignore.contains("/lfw.lock"));
    let rc = fs::read_to_string(root.join(".test-xdg/config/lightflow/.lfwrc"))?;
    assert!(rc.contains("export LFW_PATH="));
    assert!(rc.contains(".lightflow"));
    let lfw_path_manifest = root.join(".lightflow/Cargo.toml");
    assert!(lfw_path_manifest.exists());
    let lfw_path_workspace = fs::read_to_string(&lfw_path_manifest)?;
    assert!(lfw_path_workspace.contains("-lightflow-host\""));
    assert!(lfw_path_workspace.contains("publish = false"));
    assert!(lfw_path_workspace.contains("members = [\"workflows/*/*\"]"));
    assert!(root.join(".lightflow/.lightflow/workspace.rs").is_file());
    assert!(lfw_path_workspace.contains(&format!("lightflow = {:?}", env!("CARGO_PKG_VERSION"))));
    assert_eq!(
        init["config"]["workflow_workspace_manifest"],
        lfw_path_manifest.to_str().unwrap()
    );
    assert_eq!(init["config"]["workflow_workspace_created"], true);
    let zshrc = fs::read_to_string(root.join(".zshrc"))?;
    assert!(zshrc.contains("source "));
    assert!(zshrc.contains(".test-xdg/config/lightflow/.lfwrc"));
    assert_eq!(init["config"]["shell"], "zsh");
    assert_eq!(init["config"]["source_installed"], true);

    let second_init = lfw(&root, ["init"])?;
    assert_eq!(second_init["created"], serde_json::json!([]));
    assert_eq!(second_init["config"]["rc_created"], false);
    assert_eq!(second_init["config"]["source_installed"], false);
    assert_eq!(second_init["config"]["workflow_workspace_created"], false);
    let path = root.join(".lightflow/workflows/examples/extra/src/lib.rs");
    let source = fs::read_to_string(path)?;
    assert!(source.contains("workflow!()"));
    assert!(!source.contains(".version("));
    assert!(source.contains(".name(\"Extra Workflow\")"));
    assert!(source.contains(".input_description(\"value\""));
    assert!(source.contains(".input_required(\"value\", true)"));
    assert!(source.contains(".input_widget(\"value\", \"json\")"));
    let skill = fs::read_to_string(
        root.join(".lightflow/workflows/examples/extra/.agent/skills/lightflow-extra/SKILL.md"),
    )?;
    assert!(skill.contains("Workflow id: `lightflow.extra`"));
    assert!(skill.contains("Input `value`: JSON value; required; widget `json`."));
    assert!(skill.contains("## CLI Usage"));
    assert!(skill.contains("## API Usage"));
    assert!(skill.contains("POST http://127.0.0.1:5174/workflows/lightflow.extra/run"));
    assert!(skill.contains("-d '{\"inputs\":{\"value\":{\"hello\":\"world\"}}}'"));
    assert!(
        !root
            .join(".lightflow/workflows/examples/extra/src/main.rs")
            .exists()
    );
    complete_generated_workflow_metadata(&root, "examples", "example")?;
    complete_generated_workflow_metadata(&root, "examples", "extra")?;
    use_local_lightflow_dependency(&root)?;

    let workflow = lightflow(&root, ["workflows", "get", "lightflow.extra"])?;
    assert_eq!(workflow["id"], "lightflow.extra");

    let loop_check = lfw(&root, ["loop", "check", "lightflow.extra"])?;
    assert_eq!(loop_check["valid"], true);
    assert_eq!(loop_check["workflow_id"], "lightflow.extra");
    let loop_checks = loop_check["checks"].as_array().expect("loop checks");
    assert!(loop_checks.iter().any(|check| {
        check["id"] == "loop.document.local_workflow_loop" && check["status"] == "warning"
    }));
    for id in [
        "loop.workflow.discovery",
        "loop.workflow.agent_skills",
        "loop.executor.catalog",
        "loop.publish.workflow_crates",
        "loop.publish.readiness",
        "loop.selected.exists",
        "loop.selected.validation",
        "loop.selected.dependencies",
        "loop.selected.plan",
        "loop.selected.executors",
        "loop.selected.models",
        "loop.selected.publish",
        "loop.selected.patches",
        "loop.patches.registry",
    ] {
        assert!(
            loop_checks
                .iter()
                .any(|check| check["id"] == id && check["status"] == "passed"),
            "missing passed loop check {id}"
        );
    }
    for id in ["loop.selected.history", "loop.selected.replay"] {
        assert!(
            loop_checks
                .iter()
                .any(|check| check["id"] == id && check["status"] == "warning"),
            "missing warning loop check {id}"
        );
    }
    assert!(
        loop_check["next_commands"]
            .as_array()
            .expect("next commands")
            .iter()
            .any(|command| command == &serde_json::json!(["lfw", "plan", "lightflow.extra"]))
    );
    assert!(
        loop_check["next_commands"]
            .as_array()
            .expect("next commands")
            .iter()
            .any(|command| command == &serde_json::json!(["lfw", "replay", "<run_id>"]))
    );
    assert!(
        loop_check["next_commands"]
            .as_array()
            .expect("next commands")
            .iter()
            .any(|command| command == &serde_json::json!(["lfw", "loop", "changes"]))
    );
    assert!(
        loop_check["next_commands"]
            .as_array()
            .expect("next commands")
            .iter()
            .any(|command| command == &serde_json::json!(["lfw", "publish", "lightflow.extra"]))
    );

    let home = lfw(&root, ["home"])?;
    assert_eq!(home["home"], root.join(".lightflow").to_str().unwrap());
    assert_eq!(home["lfw_path"], root.join(".lightflow").to_str().unwrap());
    assert_eq!(home["manifest"], lfw_path_manifest.to_str().unwrap());
    assert_eq!(
        home["workflows"],
        root.join(".lightflow/workflows").to_str().unwrap()
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_new_and_add_support_global_workflow_workspace() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    lfw(&root, ["init"])?;
    use_local_lightflow_dependency(&root)?;

    let global = lfw(
        &root,
        [
            "new",
            "-g",
            "global_tool",
            "--category",
            "tools",
            "--name",
            "Global Tool",
        ],
    )?;
    assert_eq!(global["workflow_id"], "lightflow.global_tool");
    assert_eq!(global["global"], true);
    let global_root = root.join(".lightflow/workflows");
    assert!(global_root.join("tools/global_tool/src/lib.rs").exists());
    assert!(!root.join("workflows/tools/global_tool/src/lib.rs").exists());

    let listed = lfw(&root, ["list"])?;
    assert!(
        listed["workflows"]
            .as_array()
            .unwrap()
            .iter()
            .any(|workflow| workflow["id"] == "lightflow.global_tool")
    );

    let project_manifest = fs::read_to_string(root.join("Cargo.toml"))?;
    assert!(!project_manifest.contains("lightflow-std"));
    let added = lfw(
        &root,
        [
            "add",
            "-g",
            "lightflow-std",
            "--path",
            "vendor/lightflow-std",
        ],
    )?;
    assert_eq!(added["global"], true);
    let global_manifest = fs::read_to_string(root.join(".lightflow/Cargo.toml"))?;
    assert!(global_manifest.contains("members = [\"workflows/*/*\"]"));
    assert!(global_manifest.contains("lightflow-std"));
    let project_manifest = fs::read_to_string(root.join("Cargo.toml"))?;
    assert!(!project_manifest.contains("lightflow-std"));

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_new_runtime_template_creates_node_contract_files() -> Result<(), Box<dyn std::error::Error>>
{
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    lfw(&root, ["init"])?;
    use_local_lightflow_dependency(&root)?;

    let created = lfw(
        &root,
        [
            "new",
            "my_flux_sampler",
            "--category",
            "image",
            "--name",
            "My Flux Sampler",
            "--runtime",
            "lightflow.image.generate",
        ],
    )?;
    assert_eq!(created["workflow_id"], "lightflow.my_flux_sampler");
    assert_eq!(created["runtime"], "lightflow.image.generate");
    assert_eq!(
        created["example"],
        serde_json::json!([
            "lfw",
            "run",
            "lightflow.my_flux_sampler",
            "--prompt",
            "\"a quiet lake\"",
            "-i",
            "width=512",
            "-i",
            "height=512"
        ])
    );

    let crate_dir = root.join(".lightflow/workflows/image/my_flux_sampler");
    let source = fs::read_to_string(crate_dir.join("src/lib.rs"))?;
    assert!(source.contains(
        ".builtin_runtime(\"image_runtime\", \"lightflow.image.generate\", \"builtin.preview.v1\")"
    ));
    assert!(!source.contains(".model(\"image_model\", \"text-to-image\")"));
    assert!(source.contains(".input_widget(\"prompt\", \"prompt\")"));
    assert!(!source.contains(".input_model_requirement("));
    assert!(!source.contains(".output_model_requirement("));
    assert!(source.contains(".output_artifact_kind(\"image\", \"image\")"));

    let skill =
        fs::read_to_string(crate_dir.join(".agent/skills/lightflow-my-flux-sampler/SKILL.md"))?;
    assert!(skill.contains("Runtime: `lightflow.image.generate`."));
    assert!(skill.contains("deterministic preview"));
    assert!(skill.contains("does not represent production model quality"));
    assert!(skill.contains("declare its concrete model requirements"));
    assert!(skill.contains("lfw run lightflow.my_flux_sampler"));
    assert!(skill.contains("POST http://127.0.0.1:5174/workflows/lightflow.my_flux_sampler/run"));
    assert!(
        skill.contains(
            "-d '{\"inputs\":{\"prompt\":\"a quiet lake\",\"width\":512,\"height\":512}}'"
        )
    );

    let contract = fs::read_to_string(crate_dir.join("tests/contract.rs"))?;
    assert!(contract.contains("lightflow_my_flux_sampler::define()"));
    assert!(contract.contains("lightflow.image.generate"));

    let workflow = lfw(&root, ["workflows", "get", "lightflow.my_flux_sampler"])?;
    assert_eq!(
        workflow["runtimes"][0]["capability"],
        "lightflow.image.generate"
    );
    assert_eq!(workflow["runtimes"][0]["engine"], "builtin.preview.v1");
    assert_eq!(workflow["models"], serde_json::json!([]));
    assert_eq!(workflow["inputs"][0]["name"], "prompt");
    assert_eq!(workflow["inputs"][0]["widget"], "prompt");
    assert_eq!(workflow["outputs"][0]["artifact_kind"], "image");

    let output_path = root.join("generated-preview.png");
    let output_path_text = output_path.display().to_string();
    let execution = lfw(
        &root,
        [
            "run",
            "lightflow.my_flux_sampler",
            "--prompt",
            "a quiet lake",
            "--output",
            output_path_text.as_str(),
        ],
    )?;
    assert_eq!(execution["runtime"]["executor_id"], "builtin.preview.v1");
    let image = fs::read(&output_path)?;
    assert!(image.starts_with(b"\x89PNG\r\n\x1a\n"));

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_node_test_checks_schema_runtime_models_and_skill() -> Result<(), Box<dyn std::error::Error>>
{
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    lfw(&root, ["init"])?;
    use_local_lightflow_dependency(&root)?;
    lfw(
        &root,
        [
            "new",
            "my_flux_sampler",
            "--category",
            "image",
            "--runtime",
            "lightflow.image.generate",
        ],
    )?;

    let report = lfw(&root, ["node", "test", "lightflow.my_flux_sampler"])?;
    assert_eq!(report["workflow_id"], "lightflow.my_flux_sampler");
    assert_eq!(report["valid"], true);
    assert!(
        report["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| { check["id"] == "node.schema" && check["status"] == "passed" })
    );
    assert!(
        report["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| { check["id"] == "node.placeholders" && check["status"] == "warning" })
    );
    assert!(
        report["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| { check["id"] == "node.runtime" && check["status"] == "passed" })
    );
    assert!(
        report["checks"]
            .as_array()
            .unwrap()
            .iter()
            .any(|check| { check["id"] == "node.skill" && check["status"] == "passed" })
    );

    let crate_dir = root.join(".lightflow/workflows/image/my_flux_sampler");
    fs::remove_dir_all(crate_dir.join(".agent/skills/lightflow-my-flux-sampler"))?;
    let failed = Command::new(env!("CARGO_BIN_EXE_lfw"))
        .args(["node", "test", "lightflow.my_flux_sampler"])
        .current_dir(&root)
        .env("HOME", &root)
        .env("SHELL", "/bin/zsh")
        .env("XDG_CONFIG_HOME", root.join(".test-xdg/config"))
        .env("XDG_DATA_HOME", root.join(".test-xdg/data"))
        .env_remove("LFW_PATH")
        .output()?;
    assert!(!failed.status.success());
    let stderr = String::from_utf8_lossy(&failed.stderr);
    assert!(stderr.contains("\"valid\":false"), "stderr:\n{stderr}");
    assert!(stderr.contains("node.skill"), "stderr:\n{stderr}");

    let _ = fs::remove_dir_all(root);
    Ok(())
}
