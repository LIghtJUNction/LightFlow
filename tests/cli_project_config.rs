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
        "scripts/check-source-shape.sh",
        "cargo test --test standard_workflow_skills repository_workflow_crates_have_agent_skills",
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
