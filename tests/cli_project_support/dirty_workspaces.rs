use crate::cli_project_support::{complete_generated_workflow_metadata, git_ok, git_output};
use crate::support::{lfw, unique_temp_root};
use std::fs;
use std::path::PathBuf;

pub struct DirtyProjectWorkspaceFixture {
    pub base: PathBuf,
    pub root: PathBuf,
    pub branch_name: String,
}

pub fn dirty_project_workspace_fixture()
-> Result<DirtyProjectWorkspaceFixture, Box<dyn std::error::Error>> {
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
    Ok(DirtyProjectWorkspaceFixture {
        base,
        root,
        branch_name,
    })
}
