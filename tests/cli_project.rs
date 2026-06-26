mod support;

use lightflow::api::{ApiService, WorkflowPublishOptions};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;
use support::*;

#[test]
fn project_set_config_matches_git_submodules() -> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let project_config: toml_edit::DocumentMut =
        fs::read_to_string(root.join("projects/lightflow-projects.toml"))?.parse()?;
    let root_manifest: toml_edit::DocumentMut =
        fs::read_to_string(root.join("Cargo.toml"))?.parse()?;
    let gitmodules = fs::read_to_string(root.join(".gitmodules"))?;
    let projects_readme = fs::read_to_string(root.join("projects/README.md"))?;
    let readme = fs::read_to_string(root.join("README.md"))?;
    let check_script = fs::read_to_string(root.join("scripts/check.sh"))?;

    assert!(
        !gitmodules.contains("LIghtJUNction"),
        ".gitmodules should use canonical lightjunction GitHub URLs"
    );

    let expected = toml_string_array(&project_config, &["workspaces", "expected"]);
    let optional = toml_string_array(&project_config, &["workspaces", "optional"]);
    let default_sources = toml_string_array(&project_config, &["workflows", "default_sources"]);

    assert_eq!(
        expected,
        BTreeSet::from([
            "lightflow-flux".to_owned(),
            "lightflow-rig".to_owned(),
            "lightflow-std".to_owned(),
        ])
    );
    assert_eq!(
        optional,
        BTreeSet::from(["lightflow-auto-editing".to_owned()])
    );
    assert_eq!(
        default_sources,
        BTreeSet::from(["lightflow-std".to_owned()])
    );
    assert!(
        default_sources.is_subset(&expected),
        "default workflow sources should also be required workspaces"
    );
    assert!(
        !expected.contains("lightflow-macros")
            && !optional.contains("lightflow-macros")
            && !default_sources.contains("lightflow-macros"),
        "lightflow-macros is a core workspace crate, not a projects/ workflow workspace"
    );

    let root_members = toml_string_array(&root_manifest, &["workspace", "members"]);
    assert_eq!(
        root_members,
        BTreeSet::from([".".to_owned(), "lightflow-macros".to_owned()])
    );

    let catalog = ApiService::new(root).project_workspaces()?;
    assert!(catalog.project_config_present);
    assert_eq!(
        catalog.known_optional_workspace_names,
        vec!["lightflow-auto-editing".to_owned()]
    );
    assert_eq!(
        catalog.project_submodule_update_command,
        vec![
            "git".to_owned(),
            "submodule".to_owned(),
            "update".to_owned(),
            "--init".to_owned(),
            "--recursive".to_owned(),
            "projects/lightflow-auto-editing".to_owned(),
            "projects/lightflow-flux".to_owned(),
            "projects/lightflow-rig".to_owned(),
            "projects/lightflow-std".to_owned(),
        ]
    );
    assert!(
        projects_readme.contains(&catalog.project_submodule_update_command.join(" ")),
        "projects/README.md should show the generated project_submodule_update_command"
    );
    assert!(
        readme.contains("scripts/check.sh")
            && readme.contains("scripts/check.sh --full")
            && readme.contains("scripts/check.sh --list --full --project lightflow-std")
            && readme.contains("scripts/check.sh --full --project lightflow-std"),
        "README.md should document the local check script"
    );
    assert!(
        readme.contains("scripts/check.sh --full --workflow lightflow.text_plan")
            && readme.contains(
                "scripts/check.sh --full --project lightflow-std --workflow lightflow.text_plan"
            ),
        "README.md should document scoped workflow checks"
    );
    assert!(
        check_script.contains(
            "scripts/check.sh [--list] [--full] [--project <name>] [--workflow <workflow_id>]"
        ) && check_script.contains("scripts/check.sh --list --full --project lightflow-std")
            && check_script.contains("scripts/check.sh --full --project lightflow-std"),
        "scripts/check.sh should document scoped project checks"
    );
    assert!(
        check_script.contains("cargo run --bin lfw -- loop projects --dirty [--project <name>]"),
        "scripts/check.sh should document project-scoped dirty workspace checks"
    );
    assert!(
        check_script.contains("scripts/check.sh --full --workflow lightflow.text_plan")
            && check_script.contains(
                "scripts/check.sh --full --project lightflow-std --workflow lightflow.text_plan"
            ),
        "scripts/check.sh should document scoped workflow checks"
    );
    for expected_gate in [
        "cargo test --test standard_nodes repository_workflow_crates_have_agent_skills",
        "cargo clippy --all-targets -- -D warnings",
        "cargo run --bin lfw -- publish --workflows --require-publishable",
        "cargo run --bin lfw -- loop projects --dirty",
        "cargo run --bin lfw -- dev check",
        "cargo run --bin lfw -- release check",
        "cargo test lfw_help_advertises_project_scoped_developer_release_and_publish_selectors",
        "cargo test publish_endpoint_can_filter_project_workspaces",
        "cargo test mcp_exposes_backend_tools",
    ] {
        assert!(
            check_script.contains(expected_gate),
            "scripts/check.sh should keep the local gate: {expected_gate}"
        );
    }
    assert!(
        check_script.contains("run()")
            && check_script.contains("if [ \"$list_only\" = false ]")
            && check_script.contains("set -- \"$@\" --project \"$project\"")
            && check_script.contains("set -- \"$@\" --workflow \"$workflow\""),
        "scripts/check.sh should preview commands and pass scoped arguments through"
    );

    let list_output = Command::new(root.join("scripts/check.sh"))
        .args([
            "--list",
            "--full",
            "--project",
            "lightflow-std",
            "--workflow",
            "lightflow.text_plan",
        ])
        .current_dir(root)
        .output()?;
    assert!(
        list_output.status.success(),
        "scripts/check.sh --list failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&list_output.stdout),
        String::from_utf8_lossy(&list_output.stderr)
    );
    let list_stdout = String::from_utf8_lossy(&list_output.stdout);
    assert!(
        list_stdout
            .contains("+ cargo run --bin lfw -- loop projects --dirty --project lightflow-std"),
        "list output:\n{list_stdout}"
    );
    assert!(
        list_stdout.contains(
            "+ cargo run --bin lfw -- dev check --project lightflow-std --workflow lightflow.text_plan"
        ),
        "list output:\n{list_stdout}"
    );
    assert!(
        list_stdout.contains(
            "+ cargo run --bin lfw -- release check --project lightflow-std --workflow lightflow.text_plan"
        ),
        "list output:\n{list_stdout}"
    );
    assert!(
        !list_stdout.contains("Finished `") && !list_stdout.contains("Compiling "),
        "scripts/check.sh --list should not execute cargo:\n{list_stdout}"
    );
    let publish_index = list_stdout
        .find(
            "+ cargo run --bin lfw -- publish --workflows --require-publishable --project lightflow-std",
        )
        .expect("listed scoped publish command");
    let loop_projects_index = list_stdout
        .find("+ cargo run --bin lfw -- loop projects --dirty --project lightflow-std")
        .expect("listed loop projects command");
    let release_check_index = list_stdout
        .find(
            "+ cargo run --bin lfw -- release check --project lightflow-std --workflow lightflow.text_plan",
        )
        .expect("listed release check command");
    let clippy_index = list_stdout
        .find("+ cargo clippy --all-targets -- -D warnings")
        .expect("listed clippy command");
    assert!(
        publish_index < clippy_index
            && loop_projects_index < clippy_index
            && release_check_index < clippy_index,
        "scripts/check.sh --full should run project/release review before expensive gates:\n{list_stdout}"
    );

    let help_output = Command::new(root.join("scripts/check.sh"))
        .arg("--help")
        .current_dir(root)
        .output()?;
    assert!(
        help_output.status.success(),
        "scripts/check.sh --help failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&help_output.stdout),
        String::from_utf8_lossy(&help_output.stderr)
    );
    let help_stdout = String::from_utf8_lossy(&help_output.stdout);
    assert!(
        help_stdout.contains("cargo run --bin lfw -- loop projects --dirty [--project <name>]")
            && help_stdout.contains(
                "scripts/check.sh --full --project lightflow-std --workflow lightflow.text_plan"
            ),
        "scripts/check.sh --help should document scoped full checks:\n{help_stdout}"
    );

    let bad_filter = Command::new(root.join("scripts/check.sh"))
        .args(["--workflow", "lightflow.text_plan"])
        .current_dir(root)
        .output()?;
    assert!(!bad_filter.status.success());
    assert_eq!(bad_filter.status.code(), Some(2));
    let bad_stderr = String::from_utf8_lossy(&bad_filter.stderr);
    assert!(
        bad_stderr.contains("--workflow requires --full"),
        "stderr:\n{bad_stderr}"
    );

    for expected_text in [
        "git submodule add https://github.com/lightjunction/lightflow-example.git projects/lightflow-example",
        "lfw loop projects --project lightflow-example",
        "lfw dev check --project lightflow-example",
        "lightflow-macros",
    ] {
        assert!(
            projects_readme.contains(expected_text),
            "projects/README.md should document {expected_text}"
        );
    }

    for name in expected.iter().chain(optional.iter()) {
        assert!(
            gitmodules.contains(&format!("[submodule \"projects/{name}\"]")),
            ".gitmodules is missing projects/{name}"
        );
        assert!(
            gitmodules.contains(&format!("\tpath = projects/{name}")),
            ".gitmodules is missing path for projects/{name}"
        );
        assert!(
            gitmodules.contains(&format!(
                "\turl = https://github.com/lightjunction/{name}.git"
            )),
            ".gitmodules is missing canonical URL for projects/{name}"
        );
    }

    Ok(())
}

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
    let manifest = fs::read_to_string(root.join("workflows/examples/extra/Cargo.toml"))?;
    assert!(manifest.contains("name = \"lightflow-extra\""));
    assert!(!manifest.contains("publish = false"));
    let workspace = fs::read_to_string(root.join("Cargo.toml"))?;
    assert!(workspace.contains(&format!("lightflow = {:?}", env!("CARGO_PKG_VERSION"))));
    let gitignore = fs::read_to_string(root.join(".gitignore"))?;
    assert!(gitignore.contains("/target/"));
    assert!(gitignore.contains("/lfw.lock"));
    let rc = fs::read_to_string(root.join(".test-xdg/config/lightflow/.lfwrc"))?;
    assert!(rc.contains("export LFW_PATH="));
    assert!(rc.contains(".test-xdg/data/lightflow"));
    let lfw_path_manifest = root.join(".test-xdg/data/lightflow/Cargo.toml");
    assert!(lfw_path_manifest.exists());
    let lfw_path_workspace = fs::read_to_string(&lfw_path_manifest)?;
    assert!(lfw_path_workspace.contains("members = [\"workflows/*/*\"]"));
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
    let path = root.join("workflows/examples/extra/src/lib.rs");
    let source = fs::read_to_string(path)?;
    assert!(source.contains("workflow(\"lightflow.extra\")"));
    assert!(source.contains(".name(\"Extra Workflow\")"));
    assert!(source.contains(".input_description(\"value\""));
    assert!(source.contains(".input_required(\"value\", true)"));
    assert!(source.contains(".input_widget(\"value\", \"json\")"));
    let skill = fs::read_to_string(
        root.join("workflows/examples/extra/.agent/skills/lightflow-extra/SKILL.md"),
    )?;
    assert!(skill.contains("Workflow id: `lightflow.extra`"));
    assert!(skill.contains("Input `value`: JSON value; required; widget `json`."));
    assert!(skill.contains("## CLI Usage"));
    assert!(skill.contains("## API Usage"));
    assert!(skill.contains("POST http://127.0.0.1:5174/workflows/lightflow.extra/run"));
    assert!(skill.contains("-d '{\"inputs\":{\"value\":{\"hello\":\"world\"}}}'"));
    assert!(!root.join("workflows/examples/extra/src/main.rs").exists());
    complete_generated_workflow_metadata(&root, "examples", "example")?;
    complete_generated_workflow_metadata(&root, "examples", "extra")?;

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
    assert_eq!(
        home["home"],
        root.join(".test-xdg/data/lightflow").to_str().unwrap()
    );
    assert_eq!(
        home["lfw_path"],
        root.join(".test-xdg/data/lightflow").to_str().unwrap()
    );
    assert_eq!(home["manifest"], lfw_path_manifest.to_str().unwrap());
    assert_eq!(
        home["workflows"],
        root.join(".test-xdg/data/lightflow/workflows")
            .to_str()
            .unwrap()
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_new_and_add_support_global_workflow_workspace() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    lfw(&root, ["init"])?;

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
    let global_root = root.join(".test-xdg/data/lightflow/workflows");
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
    let global_manifest = fs::read_to_string(root.join(".test-xdg/data/lightflow/Cargo.toml"))?;
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

    let crate_dir = root.join("workflows/image/my_flux_sampler");
    let source = fs::read_to_string(crate_dir.join("src/lib.rs"))?;
    assert!(source.contains(".runtime(\"image_runtime\", \"lightflow.image.generate\")"));
    assert!(source.contains(".model(\"image_model\", \"text-to-image\")"));
    assert!(source.contains(".input_widget(\"prompt\", \"prompt\")"));
    assert!(source.contains(".input_model_requirement(\"model\", \"image_model\")"));
    assert!(source.contains(".output_artifact_kind(\"image\", \"image\")"));

    let skill =
        fs::read_to_string(crate_dir.join(".agent/skills/lightflow-my-flux-sampler/SKILL.md"))?;
    assert!(skill.contains("Runtime: `lightflow.image.generate`."));
    assert!(skill.contains("Model requirement `image_model`"));
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
    assert_eq!(workflow["models"][0]["id"], "image_model");
    assert_eq!(workflow["inputs"][0]["name"], "prompt");
    assert_eq!(workflow["inputs"][0]["widget"], "prompt");
    assert_eq!(workflow["outputs"][0]["artifact_kind"], "image");

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_node_test_checks_schema_runtime_models_and_skill() -> Result<(), Box<dyn std::error::Error>>
{
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    lfw(&root, ["init"])?;
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

    let crate_dir = root.join("workflows/image/my_flux_sampler");
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

#[test]
fn lfw_loop_check_rejects_unusable_workflow_agent_skills() -> Result<(), Box<dyn std::error::Error>>
{
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    lfw(&root, ["init"])?;
    lfw(&root, ["new", "weak_skill", "--category", "examples"])?;
    complete_generated_workflow_metadata(&root, "examples", "example")?;
    complete_generated_workflow_metadata(&root, "examples", "weak_skill")?;
    fs::write(
        root.join("workflows/examples/weak_skill/.agent/skills/lightflow-weak-skill/SKILL.md"),
        "# Weak skill\n\nThis file exists but does not describe how to run the workflow.\n",
    )?;

    let failed = lfw_command(&root).args(["loop", "check"]).output()?;
    assert!(!failed.status.success());
    let stderr = String::from_utf8_lossy(&failed.stderr);
    assert!(
        stderr.contains("loop.workflow.agent_skills"),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("frontmatter with name, description, and version"),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("workflow id `lightflow.weak_skill`"),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("CLI `lfw run` example"),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("HTTP `/workflows/lightflow.weak_skill/run` example"),
        "stderr:\n{stderr}"
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_dev_skill_template_generates_compliant_skill_source()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    lfw(&root, ["init"])?;

    let template = lfw(&root, ["dev", "skill-template", "lightflow.example"])?;
    assert_eq!(template["workflow_id"], "lightflow.example");
    assert_eq!(
        template["suggested_path"],
        ".agent/skills/lightflow-example/SKILL.md"
    );
    let source = template["source"].as_str().expect("skill template source");
    assert!(source.contains("name: Example Workflow"));
    assert!(source.contains("Workflow id: `lightflow.example`"));
    assert!(source.contains("lfw run lightflow.example"));
    assert!(source.contains("/workflows/lightflow.example/run"));
    assert!(source.contains("Run `lfw help lightflow.example`"));
    assert_eq!(template["written"], false);

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_dev_skill_template_writes_without_accidental_overwrite()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    lfw(&root, ["init"])?;

    let skill_path =
        root.join("workflows/examples/example/.agent/skills/lightflow-example/SKILL.md");
    fs::remove_file(&skill_path)?;
    let written = lfw(
        &root,
        ["dev", "skill-template", "lightflow.example", "--write"],
    )?;
    assert_eq!(written["written"], true);
    assert_eq!(written["overwritten"], false);
    assert_eq!(written["path"], skill_path.to_string_lossy().as_ref());
    let source = fs::read_to_string(&skill_path)?;
    assert!(source.contains("Workflow id: `lightflow.example`"));
    assert!(source.contains("/workflows/lightflow.example/run"));

    let rejected = lfw_command(&root)
        .args(["dev", "skill-template", "lightflow.example", "--write"])
        .output()?;
    assert!(!rejected.status.success());
    let stderr = String::from_utf8_lossy(&rejected.stderr);
    assert!(stderr.contains("already exists; pass --force"));

    fs::write(&skill_path, "stale skill\n")?;
    let forced = lfw(
        &root,
        [
            "dev",
            "skill-template",
            "lightflow.example",
            "--write",
            "--force",
        ],
    )?;
    assert_eq!(forced["written"], true);
    assert_eq!(forced["overwritten"], true);
    let source = fs::read_to_string(&skill_path)?;
    assert!(source.contains("Run `lfw help lightflow.example`"));
    assert!(!source.contains("stale skill"));

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_dev_skill_template_writes_to_explicit_sibling_project()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    lfw(&root, ["init"])?;
    let project = root.join("projects/lightflow-flux");
    write_workflow_crate_in(
        &project.join("workflows"),
        "lightflow.flux.sample",
        r#"
use lightflow::{WorkflowSpec, workflow};

pub fn define() -> WorkflowSpec {
    workflow("lightflow.flux.sample")
        .version("0.1.0")
        .name("Flux Sample")
        .description("Sample sibling project workflow.")
        .input("prompt", "text")
        .input_description("prompt", "Prompt text.")
        .input_required("prompt", true)
        .output("image", "path")
        .output_description("image", "Generated image path.")
        .build()
}
"#,
    )?;

    let lfw_path = project.to_string_lossy().to_string();
    let written = lfw_with_env_values(
        &root,
        ["dev", "skill-template", "lightflow.flux.sample", "--write"],
        [("LFW_PATH", lfw_path.as_str())],
    )?;
    let skill_path =
        project.join("workflows/local/flux_sample/.agent/skills/lightflow-flux-sample/SKILL.md");
    assert_eq!(written["written"], true);
    assert_eq!(written["path"], skill_path.to_string_lossy().as_ref());
    let source = fs::read_to_string(&skill_path)?;
    assert!(source.contains("Workflow id: `lightflow.flux.sample`"));
    assert!(source.contains("/workflows/lightflow.flux.sample/run"));

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_dev_project_config_template_writes_without_accidental_overwrite()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;

    let template = lfw(&root, ["dev", "project-config-template"])?;
    assert_eq!(template["written"], false);
    assert_eq!(template["project_config_present"], false);
    assert_eq!(template["project_config_valid"], true);
    assert_eq!(template["project_config_error"], serde_json::Value::Null);
    assert_eq!(
        template["project_config_template_command"],
        serde_json::json!(["lfw", "dev", "project-config-template"])
    );
    assert_eq!(
        template["project_config_write_command"],
        serde_json::json!(["lfw", "dev", "project-config-template", "--write"])
    );
    assert_eq!(template["optional_workspaces"], serde_json::json!([]));
    assert_eq!(
        template["project_submodule_update_command"],
        serde_json::json!([
            "git",
            "submodule",
            "update",
            "--init",
            "--recursive",
            "projects/lightflow-flux",
            "projects/lightflow-rig",
            "projects/lightflow-std"
        ])
    );
    assert!(
        template["suggested_path"]
            .as_str()
            .expect("suggested path")
            .ends_with("projects/lightflow-projects.toml")
    );
    let source = template["source"].as_str().expect("source");
    assert!(source.contains("[workspaces]"));
    assert!(source.contains("optional = []"));
    assert!(source.contains("\"lightflow-flux\""));
    assert!(source.contains("\"lightflow-std\""));
    assert!(source.contains("\"lightflow-rig\""));
    assert!(source.contains("[workflows]"));
    assert!(source.contains("default_sources"));

    let written = lfw(&root, ["dev", "project-config-template", "--write"])?;
    assert_eq!(written["written"], true);
    let config_path = root.join("projects/lightflow-projects.toml");
    assert_eq!(written["path"], config_path.to_string_lossy().as_ref());
    assert_eq!(fs::read_to_string(&config_path)?, source);
    assert_eq!(written["source"], source);

    let written_config: toml_edit::DocumentMut = fs::read_to_string(&config_path)?.parse()?;
    assert_eq!(
        toml_string_array(&written_config, &["workspaces", "expected"]),
        BTreeSet::from([
            "lightflow-flux".to_owned(),
            "lightflow-rig".to_owned(),
            "lightflow-std".to_owned(),
        ])
    );
    assert_eq!(
        toml_string_array(&written_config, &["workspaces", "optional"]),
        BTreeSet::new()
    );
    assert_eq!(
        toml_string_array(&written_config, &["workflows", "default_sources"]),
        BTreeSet::from(["lightflow-std".to_owned()])
    );

    let catalog_after_write = lfw_command(&root).args(["loop", "projects"]).output()?;
    assert!(!catalog_after_write.status.success());
    let catalog_after_write: serde_json::Value =
        serde_json::from_slice(&catalog_after_write.stderr)?;
    assert_eq!(catalog_after_write["project_config_present"], true);
    assert_eq!(catalog_after_write["project_config_valid"], true);
    assert_eq!(catalog_after_write.get("project_config_error"), None);
    assert_eq!(
        catalog_after_write["known_workspace_names"],
        serde_json::json!(["lightflow-flux", "lightflow-rig", "lightflow-std"])
    );
    assert_eq!(
        catalog_after_write["known_optional_workspace_names"],
        serde_json::json!([])
    );
    assert_eq!(
        catalog_after_write["default_workflow_sources"],
        serde_json::json!(["lightflow-std"])
    );

    let overwrite = lfw_command(&root)
        .args(["dev", "project-config-template", "--write"])
        .output()?;
    assert!(!overwrite.status.success());
    let stderr = String::from_utf8_lossy(&overwrite.stderr);
    assert!(
        stderr.contains("already exists; pass --force to overwrite"),
        "stderr:\n{stderr}"
    );

    fs::write(&config_path, "# stale config\n")?;
    let forced = lfw(
        &root,
        ["dev", "project-config-template", "--write", "--force"],
    )?;
    assert_eq!(forced["written"], true);
    let source = fs::read_to_string(config_path)?;
    assert!(source.contains("[workspaces]"));
    assert!(!source.contains("stale config"));

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_dev_project_config_template_can_repair_invalid_config()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    let projects = root.join("projects");
    fs::create_dir_all(&projects)?;
    let config_path = projects.join("lightflow-projects.toml");
    fs::write(&config_path, "[workspaces]\nexpected = [\"../broken\"]\n")?;

    let template = lfw(&root, ["dev", "project-config-template"])?;
    assert_eq!(template["written"], false);
    assert_eq!(template["project_config_present"], true);
    assert_eq!(template["project_config_valid"], false);
    assert_eq!(
        template["project_config_template_command"],
        serde_json::json!(["lfw", "dev", "project-config-template"])
    );
    assert_eq!(
        template["project_config_write_command"],
        serde_json::json!(["lfw", "dev", "project-config-template", "--write"])
    );
    assert_eq!(template["optional_workspaces"], serde_json::json!([]));
    assert_eq!(
        template["project_submodule_update_command"],
        serde_json::json!([
            "git",
            "submodule",
            "update",
            "--init",
            "--recursive",
            "projects/lightflow-flux",
            "projects/lightflow-rig",
            "projects/lightflow-std"
        ])
    );
    assert!(
        template["project_config_error"]
            .as_str()
            .expect("project config error")
            .contains("entries must be project directory names"),
        "template:\n{template:#?}"
    );
    assert!(
        template["source"]
            .as_str()
            .expect("source")
            .contains("\"lightflow-std\"")
    );

    let dev_check = lfw(&root, ["dev", "check"])?;
    assert_eq!(dev_check["project_config_present"], true);
    assert_eq!(dev_check["project_config_valid"], false);
    assert!(
        dev_check["project_config_error"]
            .as_str()
            .expect("dev check project config error")
            .contains("entries must be project directory names"),
        "dev check:\n{dev_check:#?}"
    );

    let loop_check_output = lfw_command(&root).args(["loop", "check"]).output()?;
    assert!(!loop_check_output.status.success());
    let loop_check: serde_json::Value = serde_json::from_slice(&loop_check_output.stderr)?;
    assert_eq!(loop_check["project_config_present"], true);
    assert_eq!(loop_check["project_config_valid"], false);
    assert!(
        loop_check["project_config_error"]
            .as_str()
            .expect("loop check project config error")
            .contains("entries must be project directory names"),
        "loop check:\n{loop_check:#?}"
    );
    assert_eq!(
        loop_check["project_submodule_update_command"],
        serde_json::json!([
            "git",
            "submodule",
            "update",
            "--init",
            "--recursive",
            "projects/lightflow-flux",
            "projects/lightflow-rig",
            "projects/lightflow-std"
        ])
    );

    let rejected = lfw_command(&root)
        .args(["dev", "project-config-template", "--write"])
        .output()?;
    assert!(!rejected.status.success());
    let stderr = String::from_utf8_lossy(&rejected.stderr);
    assert!(
        stderr.contains("already exists; pass --force to overwrite"),
        "stderr:\n{stderr}"
    );

    let repaired = lfw(
        &root,
        ["dev", "project-config-template", "--write", "--force"],
    )?;
    assert_eq!(repaired["written"], true);
    let source = fs::read_to_string(config_path)?;
    assert!(source.contains("[workspaces]"));
    assert!(source.contains("optional = []"));
    assert!(!source.contains("../broken"));

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn local_loop_agent_skill_failures_are_summarized() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    lfw(&root, ["init"])?;
    complete_generated_workflow_metadata(&root, "examples", "example")?;

    for index in 0..7 {
        let name = format!("weak_skill_{index}");
        lfw(&root, ["new", &name, "--category", "examples"])?;
        complete_generated_workflow_metadata(&root, "examples", &name)?;
        fs::write(
            root.join(format!(
                "workflows/examples/{name}/.agent/skills/lightflow-weak-skill-{index}/SKILL.md"
            )),
            "# Weak skill\n\nThis file exists but does not describe how to run the workflow.\n",
        )?;
    }

    let report = ApiService::new(&root).local_loop_check(None)?;
    let check = report
        .checks
        .iter()
        .find(|check| check.id == "loop.workflow.agent_skills")
        .expect("agent skill check");
    assert_eq!(serde_json::to_value(check.status)?, "failed");
    assert_eq!(check.count, Some(7));
    assert_eq!(check.details.len(), 7);
    assert!(
        check.message.contains("and 2 more"),
        "agent skill message:\n{}",
        check.message
    );
    assert!(
        check
            .details
            .iter()
            .any(|detail| detail.contains("lightflow.weak_skill_6")),
        "agent skill details:\n{:#?}",
        check.details
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_loop_changes_requires_skill_update_with_workflow_edits()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    lfw(&root, ["init"])?;
    lfw(&root, ["new", "reviewed", "--category", "examples"])?;
    complete_generated_workflow_metadata(&root, "examples", "example")?;
    complete_generated_workflow_metadata(&root, "examples", "reviewed")?;
    git_ok(&root, ["init"])?;
    git_ok(&root, ["add", "."])?;
    git_ok(
        &root,
        [
            "-c",
            "user.name=LightFlow Test",
            "-c",
            "user.email=lightflow@example.test",
            "commit",
            "-m",
            "initial workflow",
        ],
    )?;

    let source_path = root.join("workflows/examples/reviewed/src/lib.rs");
    fs::write(
        &source_path,
        fs::read_to_string(&source_path)? + "\n// reviewed behavior change\n",
    )?;
    let missing_skill = lfw_command(&root).args(["loop", "changes"]).output()?;
    assert!(!missing_skill.status.success());
    let stderr = String::from_utf8_lossy(&missing_skill.stderr);
    assert!(stderr.contains("\"valid\":false"), "stderr:\n{stderr}");
    assert!(stderr.contains("\"blockers\":[\"examples/reviewed: workflow files changed without a colocated agent skill update\"]"), "stderr:\n{stderr}");
    assert!(
        stderr.contains("workflow files changed without a colocated agent skill update"),
        "stderr:\n{stderr}"
    );
    let unsafe_loop = lfw_command(&root).args(["loop", "check"]).output()?;
    assert!(!unsafe_loop.status.success());
    let unsafe_loop_stderr = String::from_utf8_lossy(&unsafe_loop.stderr);
    assert!(
        unsafe_loop_stderr.contains("loop.source_changes.safety"),
        "stderr:\n{unsafe_loop_stderr}"
    );
    assert!(
        unsafe_loop_stderr.contains("missing colocated agent skill updates"),
        "stderr:\n{unsafe_loop_stderr}"
    );
    let blocked_publish = lfw_command(&root)
        .args(["publish", "--workflows", "--apply"])
        .output()?;
    assert!(!blocked_publish.status.success());
    let publish_stderr = String::from_utf8_lossy(&blocked_publish.stderr);
    assert!(
        publish_stderr.contains("workflow files changed without a colocated agent skill update"),
        "stderr:\n{publish_stderr}"
    );
    assert!(
        publish_stderr.contains("\"valid\":false"),
        "stderr:\n{publish_stderr}"
    );

    let skill_path =
        root.join("workflows/examples/reviewed/.agent/skills/lightflow-reviewed/SKILL.md");
    fs::write(
        &skill_path,
        fs::read_to_string(&skill_path)? + "\nReview note: behavior changed with source.\n",
    )?;
    let paired = lfw(&root, ["loop", "changes"])?;
    assert_eq!(paired["valid"], true);
    assert_eq!(paired["passed"], 1);
    assert_eq!(paired["warnings"], 0);
    assert_eq!(paired["failed"], 0);
    assert_eq!(paired["blockers"], serde_json::json!([]));
    assert_eq!(
        paired["changed_workflows"][0]["workflow_key"],
        "examples/reviewed"
    );
    assert_eq!(paired["changed_workflows"][0]["workflow_changed"], true);
    assert_eq!(paired["changed_workflows"][0]["skill_changed"], true);
    assert_eq!(paired["changed_workflows"][0]["status"], "passed");

    git_ok(&root, ["add", "."])?;
    git_ok(
        &root,
        [
            "-c",
            "user.name=LightFlow Test",
            "-c",
            "user.email=lightflow@example.test",
            "commit",
            "-m",
            "paired workflow and skill",
        ],
    )?;
    fs::write(
        &skill_path,
        fs::read_to_string(&skill_path)? + "\nReview note: skill docs clarified.\n",
    )?;
    let skill_only_loop = lfw(&root, ["loop", "check"])?;
    assert_eq!(skill_only_loop["valid"], true);
    let skill_only_checks = skill_only_loop["checks"].as_array().expect("loop checks");
    assert!(
        skill_only_checks.iter().any(|check| {
            check["id"] == "loop.source_changes.safety"
                && check["status"] == "passed"
                && check["message"].as_str().unwrap().contains(
                    "workflow source changes are paired with colocated agent skill updates",
                )
        }),
        "loop checks:\n{skill_only_checks:#?}"
    );
    git_ok(&root, ["add", "."])?;
    git_ok(
        &root,
        [
            "-c",
            "user.name=LightFlow Test",
            "-c",
            "user.email=lightflow@example.test",
            "commit",
            "-m",
            "skill docs note",
        ],
    )?;
    lfw(
        &root,
        [
            "patch",
            "save",
            "qa-debug",
            r#"{"nodes":{"identity":{"disable":true}}}"#,
        ],
    )?;
    let patch_change = lfw(&root, ["loop", "changes"])?;
    assert_eq!(patch_change["valid"], true);
    assert_eq!(patch_change["passed"], 0);
    assert_eq!(patch_change["warnings"], 1);
    assert_eq!(patch_change["failed"], 0);
    assert_eq!(patch_change["blockers"], serde_json::json!([]));
    assert_eq!(
        patch_change["changed_workflows"][0]["workflow_key"],
        "patch:qa-debug"
    );
    assert_eq!(patch_change["changed_workflows"][0]["patch_changed"], true);
    assert_eq!(
        patch_change["changed_workflows"][0]["workflow_changed"],
        false
    );
    assert_eq!(patch_change["changed_workflows"][0]["skill_changed"], false);
    assert_eq!(patch_change["changed_workflows"][0]["status"], "warning");
    assert_eq!(
        patch_change["changed_workflows"][0]["patch_paths"][0],
        ".lightflow/patches/qa-debug.json"
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_loop_changes_tracks_untracked_workflow_files() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    lfw(&root, ["init"])?;
    lfw(&root, ["new", "untracked", "--category", "examples"])?;
    git_ok(&root, ["init"])?;
    git_ok(&root, ["add", "."])?;
    git_ok(
        &root,
        [
            "-c",
            "user.name=LightFlow Test",
            "-c",
            "user.email=lightflow@example.test",
            "commit",
            "-m",
            "initial workflow",
        ],
    )?;

    let source_path = root.join("workflows/examples/untracked/src/extra.rs");
    fs::write(&source_path, "pub fn extra_behavior() {}\n")?;
    let missing_skill = lfw_command(&root).args(["loop", "changes"]).output()?;
    assert!(!missing_skill.status.success());
    let stderr = String::from_utf8_lossy(&missing_skill.stderr);
    assert!(stderr.contains("\"valid\":false"), "stderr:\n{stderr}");
    assert!(
        stderr.contains("workflow files changed without a colocated agent skill update"),
        "stderr:\n{stderr}"
    );
    assert!(stderr.contains("extra.rs"), "stderr:\n{stderr}");

    let skill_path =
        root.join("workflows/examples/untracked/.agent/skills/lightflow-untracked/SKILL.md");
    fs::write(
        &skill_path,
        fs::read_to_string(&skill_path)? + "\nReview note: extra source file added.\n",
    )?;
    let paired = lfw(&root, ["loop", "changes"])?;
    assert_eq!(paired["valid"], true);
    assert_eq!(
        paired["changed_workflows"][0]["workflow_key"],
        "examples/untracked"
    );
    assert_eq!(paired["changed_workflows"][0]["workflow_changed"], true);
    assert_eq!(paired["changed_workflows"][0]["skill_changed"], true);
    assert_eq!(paired["changed_workflows"][0]["status"], "passed");

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
#[cfg(unix)]
fn lfw_loop_changes_checks_linked_project_workspaces() -> Result<(), Box<dyn std::error::Error>> {
    let base = unique_temp_root();
    let root = base.join("core");
    let sibling = base.join("lightflow-std");
    fs::create_dir_all(&root)?;
    fs::create_dir_all(&sibling)?;

    fs::write(root.join("README.md"), "# Core\n")?;
    git_ok(&root, ["init"])?;
    git_ok(&root, ["add", "."])?;
    git_ok(
        &root,
        [
            "-c",
            "user.name=LightFlow Test",
            "-c",
            "user.email=lightflow@example.test",
            "commit",
            "-m",
            "initial core",
        ],
    )?;

    lfw(&sibling, ["init"])?;
    lfw(&sibling, ["new", "linked", "--category", "examples"])?;
    git_ok(&sibling, ["init"])?;
    git_ok(&sibling, ["add", "."])?;
    git_ok(
        &sibling,
        [
            "-c",
            "user.name=LightFlow Test",
            "-c",
            "user.email=lightflow@example.test",
            "commit",
            "-m",
            "initial sibling workflow",
        ],
    )?;

    let projects = root.join("projects");
    fs::create_dir_all(&projects)?;
    std::os::unix::fs::symlink(&sibling, projects.join("lightflow-std"))?;

    let source_path = sibling.join("workflows/examples/linked/src/lib.rs");
    fs::write(
        &source_path,
        fs::read_to_string(&source_path)? + "\n// linked behavior change\n",
    )?;
    let missing_skill = lfw_command(&root).args(["loop", "changes"]).output()?;
    assert!(!missing_skill.status.success());
    let stderr = String::from_utf8_lossy(&missing_skill.stderr);
    assert!(stderr.contains("\"valid\":false"), "stderr:\n{stderr}");
    assert!(
        stderr.contains("lightflow-std:examples/linked"),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("projects/lightflow-std/workflows/examples/linked/src/lib.rs"),
        "stderr:\n{stderr}"
    );

    let skill_path =
        sibling.join("workflows/examples/linked/.agent/skills/lightflow-linked/SKILL.md");
    fs::write(
        &skill_path,
        fs::read_to_string(&skill_path)? + "\nReview note: linked behavior changed.\n",
    )?;
    let paired = lfw(&root, ["loop", "changes"])?;
    assert_eq!(paired["valid"], true);
    assert_eq!(
        paired["changed_workflows"][0]["workflow_key"],
        "lightflow-std:examples/linked"
    );
    assert_eq!(
        paired["changed_workflows"][0]["workflow_paths"][0],
        "projects/lightflow-std/workflows/examples/linked/src/lib.rs"
    );
    assert_eq!(paired["changed_workflows"][0]["status"], "passed");

    fs::remove_file(&skill_path)?;
    let missing_skill = lfw_command(&root).args(["loop", "check"]).output()?;
    assert!(!missing_skill.status.success());
    let stderr = String::from_utf8_lossy(&missing_skill.stderr);
    assert!(
        stderr.contains("loop.workflow.agent_skills"),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("projects/lightflow-std/workflows/examples/linked"),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("missing") || stderr.contains("no SKILL.md"),
        "stderr:\n{stderr}"
    );

    let _ = fs::remove_dir_all(base);
    Ok(())
}

#[test]
#[cfg(unix)]
fn lfw_loop_changes_checks_extra_linked_project_workspaces()
-> Result<(), Box<dyn std::error::Error>> {
    let base = unique_temp_root();
    let root = base.join("core");
    let sibling = base.join("custom-workflows");
    fs::create_dir_all(&root)?;
    fs::create_dir_all(&sibling)?;

    fs::write(root.join("README.md"), "# Core\n")?;
    git_ok(&root, ["init"])?;
    git_ok(&root, ["add", "."])?;
    git_ok(
        &root,
        [
            "-c",
            "user.name=LightFlow Test",
            "-c",
            "user.email=lightflow@example.test",
            "commit",
            "-m",
            "initial core",
        ],
    )?;

    lfw(&sibling, ["init"])?;
    lfw(&sibling, ["new", "extra", "--category", "examples"])?;
    git_ok(&sibling, ["init"])?;
    git_ok(&sibling, ["add", "."])?;
    git_ok(
        &sibling,
        [
            "-c",
            "user.name=LightFlow Test",
            "-c",
            "user.email=lightflow@example.test",
            "commit",
            "-m",
            "initial extra workflow",
        ],
    )?;

    let projects = root.join("projects");
    fs::create_dir_all(&projects)?;
    std::os::unix::fs::symlink(&sibling, projects.join("custom-workflows"))?;

    let source_path = sibling.join("workflows/examples/extra/src/lib.rs");
    fs::write(
        &source_path,
        fs::read_to_string(&source_path)? + "\n// extra linked behavior change\n",
    )?;
    let missing_skill = lfw_command(&root).args(["loop", "changes"]).output()?;
    assert!(!missing_skill.status.success());
    let stderr = String::from_utf8_lossy(&missing_skill.stderr);
    assert!(
        stderr.contains("custom-workflows:examples/extra"),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("projects/custom-workflows/workflows/examples/extra/src/lib.rs"),
        "stderr:\n{stderr}"
    );
    let publish_catalog = serde_json::to_value(ApiService::new(&root).workflow_publish_checks()?)?;
    let extra_publish_check = publish_catalog["checks"]
        .as_array()
        .expect("publish checks")
        .iter()
        .find(|check| {
            check["manifest"].as_str().is_some_and(|manifest| {
                manifest.contains("projects/custom-workflows/workflows/examples/extra/Cargo.toml")
            })
        })
        .expect("extra linked workflow publish check");
    assert_eq!(
        extra_publish_check["workspace"],
        "projects/custom-workflows"
    );
    assert_eq!(extra_publish_check["publishable"], false);
    let publish_plan = lfw(&root, ["publish", "--workflows"])?;
    assert_eq!(publish_plan["total"], 2);
    assert_eq!(publish_plan["publishable_count"], 0);
    assert_eq!(publish_plan["blocked_count"], 2);
    let extra_publish_plan = publish_plan["crates"]
        .as_array()
        .expect("publish plan crates")
        .iter()
        .find(|plan| {
            plan["manifest"].as_str().is_some_and(|manifest| {
                manifest.contains("projects/custom-workflows/workflows/examples/extra/Cargo.toml")
            })
        })
        .expect("extra linked workflow publish plan");
    assert_eq!(extra_publish_plan["workspace"], "projects/custom-workflows");

    let _ = fs::remove_dir_all(base);
    Ok(())
}

#[test]
fn lfw_loop_projects_reports_project_workspace_directories()
-> Result<(), Box<dyn std::error::Error>> {
    let base = unique_temp_root();
    let root = base.join("core");
    fs::create_dir_all(&root)?;
    let projects = root.join("projects");
    fs::create_dir_all(&projects)?;
    fs::write(projects.join("README.md"), "# Project workspaces\n")?;
    fs::create_dir_all(projects.join("lightflow-flux"))?;
    fs::create_dir_all(projects.join("lightflow-rig"))?;
    fs::create_dir_all(projects.join("lightflow-custom-tools"))?;
    let std = projects.join("lightflow-std");
    fs::create_dir_all(&std)?;

    lfw(&std, ["init"])?;
    lfw(&std, ["new", "linked", "--category", "examples"])?;

    let report = lfw(&root, ["loop", "projects"])?;
    assert_eq!(report["valid"], true);
    assert_eq!(report["project_config_present"], false);
    assert_eq!(report["project_config_valid"], true);
    assert_eq!(report.get("project_config_error"), None);
    assert!(
        report["project_config_path"]
            .as_str()
            .expect("project config path")
            .ends_with("projects/lightflow-projects.toml")
    );
    assert_eq!(
        report["project_config_template_command"],
        serde_json::json!(["lfw", "dev", "project-config-template"])
    );
    assert_eq!(
        report["project_config_write_command"],
        serde_json::json!(["lfw", "dev", "project-config-template", "--write"])
    );
    assert_eq!(
        report["project_submodule_update_command"],
        serde_json::json!([
            "git",
            "submodule",
            "update",
            "--init",
            "--recursive",
            "projects/lightflow-flux",
            "projects/lightflow-rig",
            "projects/lightflow-std"
        ])
    );
    assert_eq!(report["expected_count"], 3);
    assert_eq!(report["linked_count"], 4);
    assert_eq!(report["missing_count"], 0);
    assert_eq!(report["directory_count"], 4);
    assert_eq!(report["symlink_count"], 0);
    assert_eq!(report["submodule_count"], 0);
    assert_eq!(report["not_symlink_count"], 4);
    assert_eq!(report["broken_count"], 0);
    assert_eq!(report["workflow_crate_count"], 2);
    assert_eq!(
        report["workspaces"].as_array().expect("workspaces").len(),
        4
    );
    assert_eq!(
        report["known_workspace_aliases"]["custom-tools"],
        "lightflow-custom-tools"
    );
    let std_workspace = report["workspaces"]
        .as_array()
        .expect("workspaces")
        .iter()
        .find(|workspace| workspace["name"] == "lightflow-std")
        .expect("lightflow-std workspace");
    assert_eq!(std_workspace["path"], "projects/lightflow-std");
    assert_eq!(std_workspace["is_symlink"], false);
    assert_eq!(std_workspace["workflow_crate_count"], 2);
    assert_eq!(std_workspace.get("target"), None);
    assert_eq!(std_workspace.get("git_dirty"), None);
    assert!(
        std_workspace
            .get("git_status_error")
            .and_then(|value| value.as_str())
            .is_some_and(|error| error.contains("not a git repository"))
    );
    let custom_alias_report = lfw(&root, ["loop", "projects", "--project", "custom-tools"])?;
    assert_eq!(custom_alias_report["present_count"], 1);
    assert_eq!(custom_alias_report["project_filter"], "custom-tools");
    assert_eq!(custom_alias_report["project_filter_matched"], true);
    assert_eq!(
        custom_alias_report["matched_project_workspace"],
        "lightflow-custom-tools"
    );
    assert_eq!(
        custom_alias_report["workspaces"][0]["aliases"],
        serde_json::json!(["custom-tools"])
    );

    let _ = fs::remove_dir_all(base);
    Ok(())
}

#[test]
fn lfw_loop_projects_uses_configured_expected_workspaces() -> Result<(), Box<dyn std::error::Error>>
{
    let base = unique_temp_root();
    let root = base.join("core");
    let projects = root.join("projects");
    fs::create_dir_all(&projects)?;
    fs::write(
        projects.join("lightflow-projects.toml"),
        "[workspaces]\nexpected = [\"lightflow-std\", \"lightflow-custom-tools\"]\noptional = [\"lightflow-extra-tools\"]\n",
    )?;
    fs::create_dir_all(projects.join("lightflow-std"))?;

    let output = lfw_command(&root).args(["loop", "projects"]).output()?;
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stderr)?;
    assert_eq!(report["valid"], false);
    assert_eq!(report["project_config_valid"], true);
    assert_eq!(report.get("project_config_error"), None);
    assert_eq!(report["expected_count"], 2);
    assert_eq!(report["optional_count"], 1);
    assert_eq!(report["present_count"], 1);
    assert_eq!(report["missing_count"], 1);
    assert_eq!(
        report["known_workspace_names"],
        serde_json::json!([
            "lightflow-custom-tools",
            "lightflow-extra-tools",
            "lightflow-std"
        ])
    );
    assert_eq!(
        report["optional_workspace_names"],
        serde_json::json!(["lightflow-extra-tools"])
    );
    assert_eq!(
        report["known_optional_workspace_names"],
        serde_json::json!(["lightflow-extra-tools"])
    );
    assert_eq!(report["project_config_present"], true);
    assert!(
        report["project_config_path"]
            .as_str()
            .expect("project config path")
            .ends_with("projects/lightflow-projects.toml")
    );
    assert_eq!(
        report["project_config_template_command"],
        serde_json::json!(["lfw", "dev", "project-config-template"])
    );
    assert_eq!(
        report["default_workflow_sources"],
        serde_json::json!(["lightflow-std"])
    );
    assert_eq!(
        report["known_workspace_aliases"]["custom-tools"],
        "lightflow-custom-tools"
    );
    let missing_custom = report["workspaces"]
        .as_array()
        .expect("workspaces")
        .iter()
        .find(|workspace| workspace["name"] == "lightflow-custom-tools")
        .expect("configured custom workspace");
    assert_eq!(missing_custom["expected"], true);
    assert_eq!(missing_custom["optional"], false);
    assert_eq!(missing_custom["exists"], false);
    let missing_optional = report["workspaces"]
        .as_array()
        .expect("workspaces")
        .iter()
        .find(|workspace| workspace["name"] == "lightflow-extra-tools")
        .expect("configured optional workspace");
    assert_eq!(missing_optional["expected"], false);
    assert_eq!(missing_optional["optional"], true);
    assert_eq!(missing_optional["exists"], false);
    assert_eq!(missing_optional["issues"], serde_json::json!([]));
    assert!(
        missing_custom["issues"]
            .as_array()
            .expect("custom issues")
            .iter()
            .any(|issue| issue == "missing expected project workspace checkout")
    );

    let filtered = lfw(&root, ["loop", "projects", "--project", "std"])?;
    assert_eq!(filtered["project_filter_matched"], true);
    assert_eq!(filtered["matched_project_workspace"], "lightflow-std");
    assert_eq!(filtered["optional_count"], 0);
    assert_eq!(filtered["optional_workspace_names"], serde_json::json!([]));
    assert_eq!(
        filtered["known_optional_workspace_names"],
        serde_json::json!(["lightflow-extra-tools"])
    );

    let _ = fs::remove_dir_all(base);
    Ok(())
}

#[test]
fn lfw_list_uses_configured_default_project_workflow_sources()
-> Result<(), Box<dyn std::error::Error>> {
    let base = unique_temp_root();
    let root = base.join("core");
    let projects = root.join("projects");
    let custom = projects.join("lightflow-custom-tools");
    fs::create_dir_all(&custom)?;
    fs::write(
        projects.join("lightflow-projects.toml"),
        "[workspaces]\nexpected = []\n\n[workflows]\ndefault_sources = [\"lightflow-custom-tools\"]\n",
    )?;

    lfw(&custom, ["init"])?;
    lfw(&custom, ["new", "custom", "--category", "examples"])?;
    let listed = lfw(&root, ["list"])?;
    assert!(
        listed["workflows"]
            .as_array()
            .expect("workflows")
            .iter()
            .any(|workflow| workflow["id"] == "lightflow.custom"),
        "listed workflows:\n{listed:#?}"
    );
    let projects = lfw(&root, ["loop", "projects"])?;
    assert_eq!(projects["project_config_present"], true);
    assert_eq!(
        projects["default_workflow_sources"],
        serde_json::json!(["lightflow-custom-tools"])
    );
    assert_eq!(projects["expected_count"], 1);
    assert_eq!(projects["optional_count"], 0);
    assert_eq!(
        projects["known_optional_workspace_names"],
        serde_json::json!([])
    );
    assert_eq!(projects["optional_workspace_names"], serde_json::json!([]));
    assert_eq!(projects["workspaces"][0]["expected"], true);
    assert_eq!(projects["workspaces"][0]["optional"], false);

    let _ = fs::remove_dir_all(base);
    Ok(())
}

#[test]
fn lfw_loop_projects_treats_default_sources_as_required_even_when_optional()
-> Result<(), Box<dyn std::error::Error>> {
    let base = unique_temp_root();
    let root = base.join("core");
    let projects = root.join("projects");
    let custom = projects.join("lightflow-custom-tools");
    fs::create_dir_all(&custom)?;
    fs::write(
        projects.join("lightflow-projects.toml"),
        "[workspaces]\nexpected = []\noptional = [\"lightflow-custom-tools\"]\n\n[workflows]\ndefault_sources = [\"lightflow-custom-tools\"]\n",
    )?;

    lfw(&custom, ["init"])?;
    let report = lfw(&root, ["loop", "projects"])?;
    assert_eq!(report["valid"], true);
    assert_eq!(report["expected_count"], 1);
    assert_eq!(report["optional_count"], 0);
    assert_eq!(
        report["known_optional_workspace_names"],
        serde_json::json!([])
    );
    assert_eq!(report["optional_workspace_names"], serde_json::json!([]));
    assert_eq!(
        report["default_workflow_sources"],
        serde_json::json!(["lightflow-custom-tools"])
    );
    assert_eq!(report["workspaces"][0]["expected"], true);
    assert_eq!(report["workspaces"][0]["optional"], false);

    let _ = fs::remove_dir_all(base);
    Ok(())
}

#[test]
fn lfw_loop_projects_requires_configured_default_workflow_sources()
-> Result<(), Box<dyn std::error::Error>> {
    let base = unique_temp_root();
    let root = base.join("core");
    let projects = root.join("projects");
    fs::create_dir_all(&projects)?;
    fs::write(
        projects.join("lightflow-projects.toml"),
        "[workspaces]\nexpected = []\n\n[workflows]\ndefault_sources = [\"lightflow-custom-tools\"]\n",
    )?;

    let output = lfw_command(&root).args(["loop", "projects"]).output()?;
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stderr)?;
    assert_eq!(report["valid"], false);
    assert_eq!(report["expected_count"], 1);
    assert_eq!(report["missing_count"], 1);
    assert_eq!(
        report["default_workflow_sources"],
        serde_json::json!(["lightflow-custom-tools"])
    );
    let missing_custom = report["workspaces"]
        .as_array()
        .expect("workspaces")
        .iter()
        .find(|workspace| workspace["name"] == "lightflow-custom-tools")
        .expect("configured default source workspace");
    assert_eq!(missing_custom["expected"], true);
    assert_eq!(missing_custom["exists"], false);
    assert!(
        report["issues"]
            .as_array()
            .expect("issues")
            .iter()
            .any(|issue| issue
                == "projects/lightflow-custom-tools: missing expected project workspace checkout"),
        "report:\n{report:#?}"
    );

    let _ = fs::remove_dir_all(base);
    Ok(())
}

#[test]
fn lfw_loop_projects_rejects_path_like_project_config_entries()
-> Result<(), Box<dyn std::error::Error>> {
    let base = unique_temp_root();
    let root = base.join("core");
    let projects = root.join("projects");
    fs::create_dir_all(&projects)?;
    fs::write(
        projects.join("lightflow-projects.toml"),
        "[workspaces]\nexpected = [\"../lightflow-std\"]\n",
    )?;

    let output = lfw_command(&root).args(["loop", "projects"]).output()?;
    assert!(!output.status.success());
    let report: serde_json::Value = serde_json::from_slice(&output.stderr)?;
    assert_eq!(report["valid"], false);
    assert_eq!(report["project_config_present"], true);
    assert_eq!(report["project_config_valid"], false);
    assert!(
        report["project_config_error"]
            .as_str()
            .expect("project config error")
            .contains("[workspaces].expected entries must be project directory names"),
        "report:\n{report:#?}"
    );
    assert!(
        report["issues"]
            .as_array()
            .expect("issues")
            .iter()
            .any(|issue| issue.as_str().is_some_and(|issue| {
                issue.contains("project config invalid") && issue.contains("../lightflow-std")
            })),
        "report:\n{report:#?}"
    );
    assert_eq!(
        report["project_config_write_command"],
        serde_json::json!(["lfw", "dev", "project-config-template", "--write"])
    );

    let mcp_report = lfw(
        &root,
        [
            "mcp",
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"lightflow.loop.projects","arguments":{}}}"#,
        ],
    )?;
    let mcp_text = mcp_report["result"]["content"][0]["text"]
        .as_str()
        .expect("mcp loop projects text");
    let mcp_catalog: serde_json::Value = serde_json::from_str(mcp_text)?;
    assert_eq!(mcp_catalog["valid"], false);
    assert_eq!(mcp_catalog["project_config_present"], true);
    assert_eq!(mcp_catalog["project_config_valid"], false);
    assert!(
        mcp_catalog["project_config_error"]
            .as_str()
            .expect("mcp project config error")
            .contains("[workspaces].expected entries must be project directory names"),
        "mcp catalog:\n{mcp_catalog:#?}"
    );

    let _ = fs::remove_dir_all(base);
    Ok(())
}

#[test]
fn lfw_loop_check_uses_project_workspaces_for_publish_crate_presence()
-> Result<(), Box<dyn std::error::Error>> {
    let base = unique_temp_root();
    let root = base.join("core");
    let std = root.join("projects/lightflow-std");
    fs::create_dir_all(&root)?;
    fs::create_dir_all(&std)?;

    lfw(&root, ["init"])?;
    fs::remove_dir_all(root.join("workflows"))?;
    git_ok(&root, ["init"])?;
    git_ok(&root, ["add", "."])?;
    git_ok(
        &root,
        [
            "-c",
            "user.name=LightFlow Test",
            "-c",
            "user.email=lightflow@example.test",
            "commit",
            "-m",
            "initial core",
        ],
    )?;
    lfw(&std, ["init"])?;
    complete_generated_workflow_metadata(&std, "examples", "example")?;
    git_ok(&std, ["init"])?;
    git_ok(&std, ["add", "."])?;
    git_ok(
        &std,
        [
            "-c",
            "user.name=LightFlow Test",
            "-c",
            "user.email=lightflow@example.test",
            "commit",
            "-m",
            "initial std",
        ],
    )?;

    let loop_check = lfw(&root, ["loop", "check"])?;
    let checks = loop_check["checks"].as_array().expect("loop checks");
    let publish_crates = checks
        .iter()
        .find(|check| check["id"] == "loop.publish.workflow_crates")
        .expect("publish crate presence check");
    assert_eq!(publish_crates["status"], "passed");
    assert_eq!(publish_crates["count"], 1);
    assert!(
        checks.iter().all(|check| {
            check["message"]
                .as_str()
                .is_none_or(|message| !message.contains("no workflow crates found"))
        }),
        "loop checks:\n{checks:#?}"
    );

    let _ = fs::remove_dir_all(base);
    Ok(())
}

#[test]
fn lfw_loop_projects_reports_dirty_git_workspaces() -> Result<(), Box<dyn std::error::Error>> {
    let base = unique_temp_root();
    let root = base.join("core");
    let projects = root.join("projects");
    let std = projects.join("lightflow-std");
    fs::create_dir_all(projects.join("lightflow-flux"))?;
    fs::create_dir_all(projects.join("lightflow-rig"))?;
    fs::create_dir_all(&std)?;
    fs::write(root.join("README.md"), "# core\n")?;
    git_ok(&root, ["init"])?;
    git_ok(&root, ["add", "."])?;
    git_ok(
        &root,
        [
            "-c",
            "user.name=LightFlow Test",
            "-c",
            "user.email=lightflow@example.test",
            "commit",
            "-m",
            "initial core",
        ],
    )?;
    for name in ["lightflow-flux", "lightflow-rig"] {
        let project = projects.join(name);
        fs::write(project.join("README.md"), format!("# {name}\n"))?;
        git_ok(&project, ["init"])?;
        git_ok(&project, ["add", "."])?;
        git_ok(
            &project,
            [
                "-c",
                "user.name=LightFlow Test",
                "-c",
                "user.email=lightflow@example.test",
                "commit",
                "-m",
                "initial project",
            ],
        )?;
    }
    lfw(&std, ["init"])?;
    complete_generated_workflow_metadata(&std, "examples", "example")?;
    git_ok(&std, ["init"])?;
    git_ok(&std, ["add", "."])?;
    git_ok(
        &std,
        [
            "-c",
            "user.name=LightFlow Test",
            "-c",
            "user.email=lightflow@example.test",
            "commit",
            "-m",
            "initial std",
        ],
    )?;
    git_ok(
        &std,
        [
            "remote",
            "add",
            "origin",
            "https://example.test/lightflow-std.git",
        ],
    )?;
    let branch_name = git_output(&std, ["branch", "--show-current"])?;
    git_ok(
        &std,
        [
            "update-ref",
            &format!("refs/remotes/origin/{branch_name}"),
            "HEAD",
        ],
    )?;
    git_ok(
        &std,
        [
            "branch",
            "--set-upstream-to",
            &format!("origin/{branch_name}"),
        ],
    )?;
    fs::write(std.join("README.md"), "# dirty std workspace\n")?;

    let report = lfw(&root, ["loop", "projects"])?;
    assert_eq!(report["dirty_filter"], false);
    let std_workspace = report["workspaces"]
        .as_array()
        .expect("workspaces")
        .iter()
        .find(|workspace| workspace["name"] == "lightflow-std")
        .expect("lightflow-std workspace");
    assert_eq!(std_workspace["git_dirty"], true);
    assert_eq!(std_workspace["git_changed_count"], 1);
    assert_eq!(
        std_workspace["git_changed_paths"],
        serde_json::json!(["README.md"])
    );
    assert!(
        std_workspace["git_branch"]
            .as_str()
            .is_some_and(|branch| !branch.is_empty())
    );
    assert_eq!(
        std_workspace["git_upstream"],
        format!("origin/{branch_name}")
    );
    assert_eq!(
        std_workspace["git_remote_url"],
        "https://example.test/lightflow-std.git"
    );
    assert!(
        std_workspace["git_head"]
            .as_str()
            .is_some_and(|head| !head.is_empty())
    );
    assert_eq!(
        std_workspace["git_status_command"],
        serde_json::json!(["git", "-C", "projects/lightflow-std", "status", "--short"])
    );
    assert_eq!(
        std_workspace["git_stage_command"],
        serde_json::json!(["git", "-C", "projects/lightflow-std", "add", "."])
    );
    assert_eq!(
        std_workspace["git_commit_command"],
        serde_json::json!([
            "git",
            "-C",
            "projects/lightflow-std",
            "commit",
            "-m",
            "<message>"
        ])
    );
    assert_eq!(
        std_workspace["git_push_command"],
        serde_json::json!(["git", "-C", "projects/lightflow-std", "push"])
    );
    assert_eq!(std_workspace.get("git_status_error"), None);

    let dirty_report = lfw(&root, ["loop", "projects", "--dirty"])?;
    assert_eq!(dirty_report["dirty_filter"], true);
    assert_eq!(
        dirty_report["known_workspace_names"],
        serde_json::json!(["lightflow-flux", "lightflow-rig", "lightflow-std"])
    );
    assert_eq!(
        dirty_report["known_project_workspaces"],
        dirty_report["known_workspace_names"]
    );
    assert_eq!(
        dirty_report["known_workspace_aliases"]["std"],
        "lightflow-std"
    );
    assert_eq!(
        dirty_report["known_project_aliases"],
        dirty_report["known_workspace_aliases"]
    );
    assert_eq!(dirty_report["present_count"], 1);
    assert_eq!(dirty_report["linked_count"], 1);
    assert_eq!(dirty_report["workflow_crate_count"], 1);
    let dirty_workspaces = dirty_report["workspaces"]
        .as_array()
        .expect("dirty workspaces");
    assert_eq!(dirty_workspaces.len(), 1);
    assert_eq!(dirty_workspaces[0]["name"], "lightflow-std");
    assert_eq!(dirty_workspaces[0]["aliases"], serde_json::json!(["std"]));
    assert_eq!(
        dirty_workspaces[0]["git_changed_paths"],
        serde_json::json!(["README.md"])
    );

    let std_report = lfw(&root, ["loop", "projects", "--project", "lightflow-std"])?;
    assert_eq!(std_report["present_count"], 1);
    assert_eq!(std_report["workspaces"][0]["name"], "lightflow-std");
    assert_eq!(
        std_report["workspaces"][0]["git_changed_paths"],
        serde_json::json!(["README.md"])
    );
    let std_alias_report = lfw(&root, ["loop", "projects", "--project", "std"])?;
    assert_eq!(std_alias_report["present_count"], 1);
    assert_eq!(std_alias_report["project_filter"], "std");
    assert_eq!(std_alias_report["project_filter_matched"], true);
    assert_eq!(
        std_alias_report["matched_project_workspace"],
        "lightflow-std"
    );
    assert_eq!(std_alias_report["workspaces"][0]["name"], "lightflow-std");

    let mcp_dirty_projects = lfw(
        &root,
        [
            "mcp",
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"lightflow.loop.projects","arguments":{"dirty":true,"project":"projects/lightflow-std"}}}"#,
        ],
    )?;
    let mcp_dirty_text = mcp_dirty_projects["result"]["content"][0]["text"]
        .as_str()
        .expect("mcp dirty projects text");
    let mcp_dirty_report: serde_json::Value = serde_json::from_str(mcp_dirty_text)?;
    assert_eq!(mcp_dirty_report["present_count"], 1);
    assert_eq!(mcp_dirty_report["workspaces"][0]["name"], "lightflow-std");

    let unknown_project = lfw_command(&root)
        .args(["loop", "projects", "--project", "lightflow-typo"])
        .output()?;
    assert!(!unknown_project.status.success());
    let stderr = String::from_utf8_lossy(&unknown_project.stderr);
    assert!(
        stderr.contains("project workspace filter matched no workspace: lightflow-typo"),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("known workspaces:")
            && stderr.contains("lightflow-flux")
            && stderr.contains("lightflow-rig")
            && stderr.contains("lightflow-std"),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("known aliases:")
            && stderr.contains("flux=lightflow-flux")
            && stderr.contains("rig=lightflow-rig")
            && stderr.contains("std=lightflow-std"),
        "stderr:\n{stderr}"
    );

    let loop_check = lfw(&root, ["loop", "check"])?;
    let checks = loop_check["checks"].as_array().expect("loop checks");
    let git_status = checks
        .iter()
        .find(|check| check["id"] == "loop.projects.git_status")
        .expect("project git status check");
    assert_eq!(git_status["status"], "warning");
    assert_eq!(git_status["count"], 1);
    assert!(
        git_status["details"]
            .as_array()
            .expect("details")
            .iter()
            .any(|detail| {
                detail.as_str().is_some_and(|detail| {
                    detail.contains("projects/lightflow-std has 1 changed path")
                })
            }),
        "git status check:\n{git_status:#?}"
    );

    let dev_check = lfw(&root, ["dev", "check"])?;
    let project_review = dev_check["checks"]
        .as_array()
        .expect("dev checks")
        .iter()
        .find(|check| check["id"] == "release.review.project_workspaces")
        .expect("project workspace review");
    assert_eq!(project_review["status"], "warning");
    assert!(
        project_review["details"]
            .as_array()
            .expect("project review details")
            .iter()
            .any(|detail| {
                detail.as_str().is_some_and(|detail| {
                    detail.contains("git -C projects/lightflow-std status --short")
                })
            }),
        "project review:\n{project_review:#?}"
    );
    assert!(
        project_review["details"]
            .as_array()
            .expect("project review details")
            .iter()
            .any(|detail| {
                detail.as_str().is_some_and(|detail| {
                    detail.contains("git -C projects/lightflow-std commit -m <message>")
                })
            }),
        "project review:\n{project_review:#?}"
    );

    let scoped_dev_check = lfw(&root, ["dev", "check", "--project", "lightflow-std"])?;
    assert_eq!(scoped_dev_check["project"], "lightflow-std");
    assert_eq!(scoped_dev_check["project_filter_matched"], true);
    assert_eq!(scoped_dev_check["project_config_present"], false);
    assert_eq!(scoped_dev_check["project_config_valid"], true);
    assert_eq!(
        scoped_dev_check["project_config_error"],
        serde_json::Value::Null
    );
    assert_eq!(
        scoped_dev_check["project_config_path"],
        root.join("projects/lightflow-projects.toml")
            .to_string_lossy()
            .as_ref()
    );
    assert_eq!(
        scoped_dev_check["project_config_template_command"],
        serde_json::json!(["lfw", "dev", "project-config-template"])
    );
    assert_eq!(
        scoped_dev_check["project_config_write_command"],
        serde_json::json!(["lfw", "dev", "project-config-template", "--write"])
    );
    assert_eq!(
        scoped_dev_check["project_submodule_update_command"],
        serde_json::json!([
            "git",
            "submodule",
            "update",
            "--init",
            "--recursive",
            "projects/lightflow-flux",
            "projects/lightflow-rig",
            "projects/lightflow-std"
        ])
    );
    assert_eq!(
        scoped_dev_check["default_workflow_sources"],
        serde_json::json!(["lightflow-std"])
    );
    assert_eq!(
        scoped_dev_check["known_optional_workspace_names"],
        serde_json::json!([])
    );
    assert_eq!(
        scoped_dev_check["known_project_workspaces"],
        serde_json::json!(["lightflow-flux", "lightflow-rig", "lightflow-std"])
    );
    assert_eq!(
        scoped_dev_check["known_project_aliases"]["std"],
        "lightflow-std"
    );
    let scoped_checks = scoped_dev_check["checks"]
        .as_array()
        .expect("scoped dev checks");
    let scoped_project_review = scoped_checks
        .iter()
        .find(|check| check["id"] == "release.review.project_workspaces")
        .expect("scoped project review");
    assert_eq!(scoped_project_review["count"], 1);
    assert!(
        scoped_project_review["details"]
            .as_array()
            .expect("scoped project review details")
            .iter()
            .any(|detail| detail.as_str() == Some("project filter: lightflow-std")),
        "scoped project review:\n{scoped_project_review:#?}"
    );
    let scoped_publish_review = scoped_checks
        .iter()
        .find(|check| check["id"] == "release.review.workflow_publish_ready")
        .expect("scoped publish review");
    assert_eq!(scoped_publish_review["count"], 1);
    assert!(
        scoped_publish_review["details"]
            .as_array()
            .expect("scoped publish review details")
            .iter()
            .any(|detail| detail.as_str() == Some("workspace: projects/lightflow-std")),
        "scoped publish review:\n{scoped_publish_review:#?}"
    );
    assert!(scoped_checks.iter().any(|check| {
        check["id"] == "release.command.project_workspaces"
            && check["command"]
                == serde_json::json!([
                    "cargo",
                    "run",
                    "--bin",
                    "lfw",
                    "--",
                    "loop",
                    "projects",
                    "--project",
                    "lightflow-std"
                ])
    }));
    assert!(scoped_checks.iter().any(|check| {
        check["id"] == "release.command.dirty_project_workspaces"
            && check["command"]
                == serde_json::json!([
                    "cargo",
                    "run",
                    "--bin",
                    "lfw",
                    "--",
                    "loop",
                    "projects",
                    "--dirty",
                    "--project",
                    "lightflow-std"
                ])
    }));
    assert!(scoped_checks.iter().any(|check| {
        check["id"] == "release.command.workflow_publish_ready"
            && check["command"]
                == serde_json::json!([
                    "cargo",
                    "run",
                    "--bin",
                    "lfw",
                    "--",
                    "publish",
                    "--workflows",
                    "--require-publishable",
                    "--project",
                    "lightflow-std"
                ])
    }));
    let scoped_alias_dev_check = lfw(&root, ["dev", "check", "--project", "std"])?;
    assert_eq!(scoped_alias_dev_check["project"], "std");
    assert_eq!(scoped_alias_dev_check["project_filter_matched"], true);
    assert_eq!(
        scoped_alias_dev_check["matched_project_workspace"],
        "lightflow-std"
    );
    let scoped_alias_review = scoped_alias_dev_check["checks"]
        .as_array()
        .expect("scoped alias dev checks")
        .iter()
        .find(|check| check["id"] == "release.review.project_workspaces")
        .expect("scoped alias project review");
    assert_eq!(scoped_alias_review["count"], 1);
    assert!(
        scoped_alias_review["details"]
            .as_array()
            .expect("scoped alias project review details")
            .iter()
            .any(|detail| detail.as_str() == Some("projects/lightflow-std has 1 changed path(s)")),
        "scoped alias project review:\n{scoped_alias_review:#?}"
    );

    let unknown_scoped_dev_check = lfw(&root, ["dev", "check", "--project", "lightflow-typo"])?;
    assert_eq!(unknown_scoped_dev_check["valid"], false);
    assert_eq!(unknown_scoped_dev_check["project_filter_matched"], false);
    assert_eq!(
        unknown_scoped_dev_check.get("matched_project_workspace"),
        None
    );
    assert_eq!(
        unknown_scoped_dev_check["known_project_workspaces"],
        serde_json::json!(["lightflow-flux", "lightflow-rig", "lightflow-std"])
    );
    let unknown_project_review = unknown_scoped_dev_check["checks"]
        .as_array()
        .expect("unknown project checks")
        .iter()
        .find(|check| check["id"] == "release.review.project_workspaces")
        .expect("unknown project review");
    assert_eq!(unknown_project_review["status"], "failed");
    assert!(
        unknown_scoped_dev_check["issues"]
            .as_array()
            .expect("unknown project issues")
            .iter()
            .any(|issue| {
                issue.as_str().is_some_and(|issue| {
                    issue.contains("project workspace filter matched no workspace: lightflow-typo")
                        && issue.contains("known aliases:")
                        && issue.contains("std=lightflow-std")
                })
            }),
        "unknown project dev check:\n{unknown_scoped_dev_check:#?}"
    );
    let unknown_scoped_apply = lfw_command(&root)
        .args(["dev", "check", "--apply", "--project", "lightflow-typo"])
        .output()?;
    assert!(!unknown_scoped_apply.status.success());
    let apply_stderr = String::from_utf8_lossy(&unknown_scoped_apply.stderr);
    assert!(
        apply_stderr.contains("\"valid\":false")
            && apply_stderr.contains("\"status\":\"skipped\"")
            && apply_stderr.contains("command skipped because an earlier release gate failed"),
        "stderr:\n{apply_stderr}"
    );

    let _ = fs::remove_dir_all(base);
    Ok(())
}

#[test]
fn lfw_loop_projects_reports_stale_parent_gitlinks() -> Result<(), Box<dyn std::error::Error>> {
    let base = unique_temp_root();
    let root = base.join("core");
    let std = root.join("projects/lightflow-std");
    fs::create_dir_all(&std)?;
    fs::write(root.join("README.md"), "# core\n")?;

    lfw(&std, ["init"])?;
    complete_generated_workflow_metadata(&std, "examples", "example")?;
    git_ok(&std, ["init"])?;
    git_ok(&std, ["add", "."])?;
    git_ok(
        &std,
        [
            "-c",
            "user.name=LightFlow Test",
            "-c",
            "user.email=lightflow@example.test",
            "commit",
            "-m",
            "initial std",
        ],
    )?;

    git_ok(&root, ["init"])?;
    git_ok(&root, ["add", "."])?;
    git_ok(
        &root,
        [
            "-c",
            "user.name=LightFlow Test",
            "-c",
            "user.email=lightflow@example.test",
            "commit",
            "-m",
            "initial parent gitlink",
        ],
    )?;

    fs::write(std.join("README.md"), "# updated std\n")?;
    git_ok(&std, ["add", "."])?;
    git_ok(
        &std,
        [
            "-c",
            "user.name=LightFlow Test",
            "-c",
            "user.email=lightflow@example.test",
            "commit",
            "-m",
            "update std",
        ],
    )?;

    let report = lfw(
        &root,
        ["loop", "projects", "--dirty", "--project", "lightflow-std"],
    )?;
    let workspace = &report["workspaces"][0];
    assert_eq!(workspace["name"], "lightflow-std");
    assert_eq!(workspace["git_dirty"], false);
    assert_eq!(workspace["parent_gitlink_changed"], true);
    assert_ne!(workspace["parent_gitlink_head"], workspace["git_head"]);
    assert_eq!(
        workspace["parent_gitlink_stage_command"],
        serde_json::json!(["git", "add", "projects/lightflow-std"])
    );

    let _ = fs::remove_dir_all(base);
    Ok(())
}

#[test]
#[cfg(unix)]
fn lfw_publish_workflows_dedupes_linked_workspace_duplicates()
-> Result<(), Box<dyn std::error::Error>> {
    let base = unique_temp_root();
    let root = base.join("core");
    let sibling = base.join("lightflow-std");
    fs::create_dir_all(&root)?;
    fs::create_dir_all(&sibling)?;

    lfw(&root, ["init"])?;
    lfw(&sibling, ["init"])?;
    complete_generated_workflow_metadata(&root, "examples", "example")?;
    complete_generated_workflow_metadata(&sibling, "examples", "example")?;

    let projects = root.join("projects");
    fs::create_dir_all(&projects)?;
    std::os::unix::fs::symlink(&sibling, projects.join("lightflow-std"))?;

    let api_catalog = serde_json::to_value(ApiService::new(&root).workflow_publish_checks()?)?;
    let cli_plan = lfw(&root, ["publish", "--workflows"])?;
    assert_eq!(api_catalog["total"], 1);
    assert_eq!(cli_plan["total"], api_catalog["total"]);
    assert_eq!(
        cli_plan["publishable_count"],
        api_catalog["publishable_count"]
    );
    assert_eq!(cli_plan["blocked_count"], api_catalog["blocked_count"]);
    assert_eq!(cli_plan["crates"].as_array().expect("crates").len(), 1);
    assert_eq!(cli_plan["crates"][0]["workflow_id"], "lightflow.example");
    assert_eq!(cli_plan["crates"][0]["workspace"], "root");
    assert!(
        !cli_plan["crates"][0]["manifest"]
            .as_str()
            .expect("manifest")
            .contains("projects/lightflow-std"),
        "cli plan:\n{cli_plan:#?}"
    );

    let scoped_plan = lfw(
        &root,
        ["publish", "--workflows", "--project", "lightflow-std"],
    )?;
    assert_eq!(scoped_plan["project"], "lightflow-std");
    assert_eq!(scoped_plan["project_filter_matched"], true);
    assert_eq!(scoped_plan["total"], 1);
    assert_eq!(scoped_plan["crates"][0]["workflow_id"], "lightflow.example");
    assert_eq!(
        scoped_plan["crates"][0]["workspace"],
        "projects/lightflow-std"
    );
    assert!(
        scoped_plan["crates"][0]["manifest"]
            .as_str()
            .expect("manifest")
            .contains("projects/lightflow-std"),
        "scoped plan:\n{scoped_plan:#?}"
    );

    let scoped_api_catalog = serde_json::to_value(
        ApiService::new(&root).workflow_publish_checks_with_options(&WorkflowPublishOptions {
            project: Some("lightflow-std".to_owned()),
        })?,
    )?;
    assert_eq!(scoped_api_catalog["project"], "lightflow-std");
    assert_eq!(scoped_api_catalog["project_filter_matched"], true);
    assert_eq!(
        scoped_api_catalog["matched_project_workspace"],
        "lightflow-std"
    );
    assert_eq!(scoped_api_catalog["total"], 1);
    assert_eq!(
        scoped_api_catalog["checks"][0]["workspace"],
        "projects/lightflow-std"
    );

    let scoped_mcp = lfw(
        &root,
        [
            "mcp",
            r#"{"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":"lightflow.workflow.publish_list","arguments":{"project":"std"}}}"#,
        ],
    )?;
    let scoped_mcp_text = scoped_mcp["result"]["content"][0]["text"]
        .as_str()
        .expect("scoped publish list mcp text");
    let scoped_mcp_catalog: serde_json::Value = serde_json::from_str(scoped_mcp_text)?;
    assert_eq!(scoped_mcp_catalog["project"], "std");
    assert_eq!(
        scoped_mcp_catalog["matched_project_workspace"],
        "lightflow-std"
    );
    assert_eq!(scoped_mcp_catalog["total"], 1);
    assert_eq!(
        scoped_mcp_catalog["checks"][0]["workspace"],
        "projects/lightflow-std"
    );

    let scoped_alias_plan = lfw(&root, ["publish", "--workflows", "--project", "std"])?;
    assert_eq!(scoped_alias_plan["project"], "std");
    assert_eq!(scoped_alias_plan["project_filter_matched"], true);
    assert_eq!(
        scoped_alias_plan["crates"][0]["workspace"],
        "projects/lightflow-std"
    );

    let unknown_project = lfw_command(&root)
        .args(["publish", "--workflows", "--project", "lightflow-typo"])
        .output()?;
    assert!(!unknown_project.status.success());
    let unknown_stderr = String::from_utf8_lossy(&unknown_project.stderr);
    assert!(
        unknown_stderr.contains("project workspace filter matched no workspace: lightflow-typo"),
        "stderr:\n{unknown_stderr}"
    );

    let _ = fs::remove_dir_all(base);
    Ok(())
}

#[test]
#[cfg(unix)]
fn lfw_loop_check_fails_when_linked_project_cannot_be_inspected()
-> Result<(), Box<dyn std::error::Error>> {
    let base = unique_temp_root();
    let root = base.join("core");
    let sibling = base.join("lightflow-std");
    fs::create_dir_all(&root)?;
    fs::create_dir_all(&sibling)?;

    fs::write(root.join("README.md"), "# Core\n")?;
    git_ok(&root, ["init"])?;
    git_ok(&root, ["add", "."])?;
    git_ok(
        &root,
        [
            "-c",
            "user.name=LightFlow Test",
            "-c",
            "user.email=lightflow@example.test",
            "commit",
            "-m",
            "initial core",
        ],
    )?;

    lfw(&sibling, ["init"])?;
    let projects = root.join("projects");
    fs::create_dir_all(&projects)?;
    std::os::unix::fs::symlink(&sibling, projects.join("lightflow-std"))?;

    let output = lfw_command(&root).args(["loop", "check"]).output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("\"id\":\"loop.source_changes.safety\""),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("\"status\":\"failed\""),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("projects/lightflow-std: git status failed"),
        "stderr:\n{stderr}"
    );

    let _ = fs::remove_dir_all(base);
    Ok(())
}

#[test]
fn lfw_init_plugin_creates_standard_cargo_plugin_crate() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;

    let init = lfw(&root, ["init", "--plugin"])?;
    assert_eq!(init["kind"], "plugin");
    assert!(root.join(".gitignore").exists());
    assert!(root.join("Cargo.toml").exists());
    assert!(root.join("src/lib.rs").exists());
    assert!(root.join("tests/contract.rs").exists());
    let plugin_skill_root = root.join(".agent/skills");
    assert!(
        fs::read_dir(&plugin_skill_root)?
            .filter_map(Result::ok)
            .any(|entry| entry.path().join("SKILL.md").exists())
    );
    assert!(!root.join("workflows").exists());
    assert!(!root.join(".test-xdg/config/lightflow/.lfwrc").exists());

    let manifest = fs::read_to_string(root.join("Cargo.toml"))?;
    assert!(manifest.contains("name = \"lightflow-cli-test"));
    assert!(manifest.contains(&format!("lightflow = {:?}", env!("CARGO_PKG_VERSION"))));
    let source = fs::read_to_string(root.join("src/lib.rs"))?;
    assert!(source.contains("pub fn define() -> WorkflowSpec"));
    let skill_path = fs::read_dir(&plugin_skill_root)?
        .filter_map(Result::ok)
        .map(|entry| entry.path().join("SKILL.md"))
        .find(|path| path.exists())
        .expect("plugin skill");
    let skill = fs::read_to_string(skill_path)?;
    assert!(skill.contains("## CLI Usage"));
    assert!(skill.contains("## API Usage"));
    assert!(skill.contains("/workflows/lightflow."));
    let contract = fs::read_to_string(root.join("tests/contract.rs"))?;
    let package_ident = root
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap()
        .replace('-', "_");
    assert!(contract.contains(&format!("{package_ident}::define()")));
    assert!(!contract.contains("lightflow_lightflow"));

    fs::create_dir_all(root.join(".cargo"))?;
    fs::write(
        root.join(".cargo/config.toml"),
        format!(
            "[patch.crates-io]\nlightflow = {{ path = {:?} }}\n",
            env!("CARGO_MANIFEST_DIR")
        ),
    )?;
    let test_status = Command::new("cargo")
        .arg("test")
        .current_dir(&root)
        .status()?;
    assert!(test_status.success());

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_update_and_upgrade_delegate_to_cargo() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    fs::write(
        root.join("Cargo.toml"),
        r#"[package]
name = "lfw-update-test"
version = "0.1.0"
edition = "2024"
"#,
    )?;
    fs::create_dir_all(root.join("src"))?;
    fs::write(root.join("src/lib.rs"), "")?;

    let update = lfw(&root, ["update"])?;
    assert_eq!(update["command"], serde_json::json!(["cargo", "fetch"]));
    assert_eq!(update["executed"], true);

    let upgrade = lfw(&root, ["upgrade"])?;
    assert_eq!(upgrade["command"], serde_json::json!(["cargo", "update"]));
    assert_eq!(upgrade["executed"], true);

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_uses_xdg_default_and_lfw_path_environment() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    let xdg_data_workflows = root.join(".test-xdg/data/lightflow/workflows");
    write_workflow_crate_in(
        &xdg_data_workflows,
        "lightflow.xdg_default",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.xdg_default")
        .version("0.1.0")
        .name("XDG Default")
        .input("value", "json")
        .output("value", "json")
        .build()
}
"#,
    )?;

    let default_list = lfw(&root, ["list"])?;
    assert_eq!(default_list["workflows"][0]["id"], "lightflow.xdg_default");

    let legacy_default_home = lfw_with_env(
        &root,
        ["home"],
        [("LFW_PATH", xdg_data_workflows.as_path())],
    )?;
    assert_eq!(
        legacy_default_home["lfw_path"],
        root.join(".test-xdg/data/lightflow").to_str().unwrap()
    );
    let legacy_default_list = lfw_with_env(
        &root,
        ["list"],
        [("LFW_PATH", xdg_data_workflows.as_path())],
    )?;
    assert_eq!(
        legacy_default_list["workflows"][0]["id"],
        "lightflow.xdg_default"
    );

    let custom_workflows = root.join("custom-workflows");
    write_workflow_crate_in(
        &custom_workflows,
        "lightflow.rc",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.rc")
        .version("0.1.0")
        .name("RC Workflow")
        .input("value", "json")
        .output("value", "json")
        .build()
}
"#,
    )?;
    let rc_dir = root.join(".test-xdg/config/lightflow");
    fs::create_dir_all(&rc_dir)?;
    fs::write(
        rc_dir.join(".lfwrc"),
        format!("export LFW_PATH='{}'\n", custom_workflows.display()),
    )?;

    let still_default = lfw(&root, ["list"])?;
    assert_eq!(still_default["workflows"][0]["id"], "lightflow.xdg_default");

    let env_list = lfw_with_env(&root, ["list"], [("LFW_PATH", custom_workflows.as_path())])?;
    assert_eq!(env_list["workflows"][0]["id"], "lightflow.rc");
    let env_loop = lfw_with_env(
        &root,
        ["loop", "check", "lightflow.rc"],
        [("LFW_PATH", custom_workflows.as_path())],
    )?;
    let env_loop_checks = env_loop["checks"].as_array().expect("loop checks");
    assert!(
        env_loop_checks.iter().any(|check| {
            check["id"] == "loop.selected.publish"
                && check["status"] == "warning"
                && check["message"]
                    .as_str()
                    .unwrap()
                    .contains("package.publish is false")
        }),
        "loop checks:\n{env_loop_checks:#?}"
    );
    assert!(
        env_loop_checks
            .iter()
            .any(|check| { check["id"] == "loop.selected.exists" && check["status"] == "passed" }),
        "loop checks:\n{env_loop_checks:#?}"
    );

    write_workflow_crate(
        &root,
        "lightflow.rc",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.rc")
        .version("0.1.0")
        .name("Project Override")
        .input("value", "json")
        .output("value", "json")
        .build()
}
"#,
    )?;
    let project_wins = lfw_with_env(&root, ["list"], [("LFW_PATH", custom_workflows.as_path())])?;
    assert_eq!(project_wins["workflows"][0]["name"], "Project Override");

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_init_installs_fish_source_when_shell_is_fish() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    let output = Command::new(env!("CARGO_BIN_EXE_lfw"))
        .arg("init")
        .current_dir(&root)
        .env("HOME", &root)
        .env("SHELL", "/usr/bin/fish")
        .env("XDG_CONFIG_HOME", root.join(".test-xdg/config"))
        .env("XDG_DATA_HOME", root.join(".test-xdg/data"))
        .env_remove("LFW_PATH")
        .output()?;
    assert!(
        output.status.success(),
        "lfw init failed\nstdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    let init: Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(init["config"]["shell"], "fish");
    assert_eq!(init["config"]["source_installed"], true);

    let rc = fs::read_to_string(root.join(".test-xdg/config/lightflow/.lfwrc"))?;
    assert!(rc.contains("set -gx LFW_PATH "));
    assert!(root.join(".test-xdg/data/lightflow/Cargo.toml").exists());
    let fish_config = fs::read_to_string(root.join(".test-xdg/config/fish/config.fish"))?;
    assert!(fish_config.contains("source "));
    assert!(fish_config.contains(".test-xdg/config/lightflow/.lfwrc"));

    let _ = fs::remove_dir_all(root);
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

    let workflows_plan = lfw(&root, ["publish", "--workflows"])?;
    assert_eq!(workflows_plan["dry_run"], true);
    assert_eq!(workflows_plan["target"]["kind"], "workflows");
    assert_eq!(workflows_plan["publishable"], true);
    assert_eq!(workflows_plan["total"], 1);
    assert_eq!(workflows_plan["publishable_count"], 1);
    assert_eq!(workflows_plan["blocked_count"], 0);
    assert_eq!(workflows_plan["crates"].as_array().unwrap().len(), 1);
    assert_eq!(workflows_plan["crates"][0]["package"], "lightflow-example");
    assert_eq!(
        workflows_plan["commands"][0],
        serde_json::json!([
            "cargo",
            "publish",
            "--manifest-path",
            "workflows/examples/example/Cargo.toml",
            "--dry-run"
        ])
    );
    let dirty_workflows_plan = lfw(&root, ["publish", "--workflows", "--allow-dirty"])?;
    assert_eq!(
        dirty_workflows_plan["commands"][0],
        serde_json::json!([
            "cargo",
            "publish",
            "--manifest-path",
            "workflows/examples/example/Cargo.toml",
            "--allow-dirty",
            "--dry-run"
        ])
    );
    let strict_workflows_plan = lfw(&root, ["publish", "--workflows", "--require-publishable"])?;
    assert_eq!(strict_workflows_plan["publishable"], true);
    assert_eq!(strict_workflows_plan["total"], 1);
    assert_eq!(strict_workflows_plan["publishable_count"], 1);
    assert_eq!(strict_workflows_plan["blocked_count"], 0);

    lfw(&root, ["new", "lightflow.base", "--category", "examples"])?;
    lfw(&root, ["new", "lightflow.top", "--category", "examples"])?;
    complete_generated_workflow_metadata(&root, "examples", "base")?;
    complete_generated_workflow_metadata(&root, "examples", "top")?;
    let top_manifest_path = root.join("workflows/examples/top/Cargo.toml");
    let mut top_manifest = fs::read_to_string(&top_manifest_path)?;
    top_manifest.push_str("lightflow-base = { path = \"../base\", version = \"0.1.0\" }\n");
    fs::write(&top_manifest_path, top_manifest)?;
    let ordered_plan = lfw(&root, ["publish", "--workflows"])?;
    let packages = ordered_plan["crates"]
        .as_array()
        .unwrap()
        .iter()
        .map(|crate_plan| crate_plan["package"].as_str().unwrap())
        .collect::<Vec<_>>();
    let base_index = packages
        .iter()
        .position(|package| *package == "lightflow-base")
        .unwrap();
    let top_index = packages
        .iter()
        .position(|package| *package == "lightflow-top")
        .unwrap();
    assert!(base_index < top_index);
    assert_eq!(
        ordered_plan["crates"][top_index]["internal_dependencies"],
        serde_json::json!(["lightflow-base"])
    );
    let publish_catalog = serde_json::to_value(ApiService::new(&root).workflow_publish_checks()?)?;
    let api_packages = publish_catalog["checks"]
        .as_array()
        .unwrap()
        .iter()
        .map(|check| check["package"].as_str().unwrap())
        .collect::<Vec<_>>();
    let api_base_index = api_packages
        .iter()
        .position(|package| *package == "lightflow-base")
        .unwrap();
    let api_top_index = api_packages
        .iter()
        .position(|package| *package == "lightflow-top")
        .unwrap();
    assert!(api_base_index < api_top_index);
    assert_eq!(
        publish_catalog["checks"][api_top_index]["internal_dependencies"],
        serde_json::json!(["lightflow-base"])
    );
    assert_eq!(
        publish_catalog["checks"][api_top_index]["version"],
        serde_json::json!("0.1.0")
    );

    let root_manifest_path = root.join("Cargo.toml");
    let mut root_manifest = fs::read_to_string(&root_manifest_path)?;
    root_manifest.push_str("bad-workspace = { path = \"../bad-workspace\" }\n");
    fs::write(&root_manifest_path, root_manifest)?;
    let example_manifest_path = root.join("workflows/examples/example/Cargo.toml");
    let mut example_manifest = fs::read_to_string(&example_manifest_path)?;
    example_manifest.push_str("bad-workspace = { workspace = true }\n");
    fs::write(&example_manifest_path, example_manifest)?;
    let blocked_plan = lfw(&root, ["publish", "--workflows"])?;
    assert_eq!(blocked_plan["publishable"], false);
    assert_eq!(blocked_plan["total"], 3);
    assert_eq!(blocked_plan["publishable_count"], 2);
    assert_eq!(blocked_plan["blocked_count"], 1);
    assert!(
        blocked_plan["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|issue| issue
                .as_str()
                .unwrap()
                .contains("dependency bad-workspace uses path without a crates.io version"))
    );
    let strict_blocked = lfw_command(&root)
        .args(["publish", "--workflows", "--require-publishable"])
        .output()?;
    assert!(!strict_blocked.status.success());
    let strict_stderr = String::from_utf8_lossy(&strict_blocked.stderr);
    assert!(strict_stderr.contains("\"publishable\":false"));
    assert!(
        strict_stderr.contains("dependency bad-workspace uses path without a crates.io version")
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}
#[test]
fn add_writes_git_workflow_dependency() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    lfw(&root, ["init"])?;

    let output = lfw(
        &root,
        [
            "add",
            "lightflow-std",
            "--git",
            "https://github.com/lightjunction/lightflow-std",
            "--package",
            "lightflow-std",
        ],
    )?;
    assert_eq!(output["dependency"], "lightflow-std");
    assert_eq!(
        output["source"]["git"],
        "https://github.com/lightjunction/lightflow-std"
    );
    assert_eq!(output["package"], "lightflow-std");

    let manifest = fs::read_to_string(root.join("Cargo.toml"))?;
    assert!(manifest.contains(
        "lightflow-std = { git = \"https://github.com/lightjunction/lightflow-std\", package = \"lightflow-std\" }"
    ));

    let _ = fs::remove_dir_all(root);
    Ok(())
}

fn git_ok<const N: usize>(root: &Path, args: [&str; N]) -> Result<(), Box<dyn std::error::Error>> {
    let output = Command::new("git").args(args).current_dir(root).output()?;
    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "git failed with status {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into())
    }
}

fn toml_string_array(document: &toml_edit::DocumentMut, path: &[&str]) -> BTreeSet<String> {
    let mut item = document.as_item();
    for segment in path {
        item = item
            .get(segment)
            .unwrap_or_else(|| panic!("missing TOML key {}", path.join(".")));
    }
    item.as_array()
        .unwrap_or_else(|| panic!("TOML key {} is not an array", path.join(".")))
        .iter()
        .map(|value| {
            value
                .as_str()
                .unwrap_or_else(|| panic!("TOML key {} contains a non-string", path.join(".")))
                .to_owned()
        })
        .collect()
}

fn git_output<const N: usize>(
    root: &Path,
    args: [&str; N],
) -> Result<String, Box<dyn std::error::Error>> {
    let output = Command::new("git").args(args).current_dir(root).output()?;
    if output.status.success() {
        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    } else {
        Err(format!(
            "git failed with status {}\nstdout:\n{}\nstderr:\n{}",
            output.status,
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        )
        .into())
    }
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
