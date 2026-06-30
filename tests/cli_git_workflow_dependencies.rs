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
