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
fn lfw_loop_check_rejects_unusable_workflow_agent_skills() -> Result<(), Box<dyn std::error::Error>>
{
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    lfw(&root, ["init"])?;
    lfw(&root, ["new", "weak_skill", "--category", "examples"])?;
    complete_generated_workflow_metadata(&root, "examples", "example")?;
    complete_generated_workflow_metadata(&root, "examples", "weak_skill")?;
    fs::write(
        root.join(
            ".lightflow/workflows/examples/weak_skill/.agent/skills/lightflow-weak-skill/SKILL.md",
        ),
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
        root.join(".lightflow/workflows/examples/example/.agent/skills/lightflow-example/SKILL.md");
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
