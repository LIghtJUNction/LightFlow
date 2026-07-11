#![allow(unused_imports)]

mod support;

use lightflow::api::{ApiService, WorkflowPublishOptions};
use serde_json::Value;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;
use support::*;

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
    lfw(&std, ["new", "linked"])?;

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
    let filtered_by_path = lfw(
        &root,
        [
            "loop",
            "projects",
            "--project",
            root.join("projects/lightflow-std")
                .to_str()
                .expect("project path"),
        ],
    )?;
    assert_eq!(filtered_by_path["project_filter_matched"], true);
    assert_eq!(
        filtered_by_path["matched_project_workspace"],
        "lightflow-std"
    );
    let filtered_by_relative_path = lfw(
        &root,
        ["loop", "projects", "--project", "./projects/lightflow-std"],
    )?;
    assert_eq!(filtered_by_relative_path["project_filter_matched"], true);
    assert_eq!(
        filtered_by_relative_path["matched_project_workspace"],
        "lightflow-std"
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
    lfw(&custom, ["new", "custom"])?;
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
