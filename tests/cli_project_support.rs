#![allow(dead_code)]

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::process::Command;

pub fn git_ok<const N: usize>(
    root: &Path,
    args: [&str; N],
) -> Result<(), Box<dyn std::error::Error>> {
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

pub fn toml_string_array(document: &toml_edit::DocumentMut, path: &[&str]) -> BTreeSet<String> {
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

pub fn git_output<const N: usize>(
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

pub fn complete_generated_workflow_metadata(
    root: &Path,
    name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = root
        .join(".lightflow/workflows")
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

pub fn use_local_lightflow_dependency(root: &Path) -> Result<(), Box<dyn std::error::Error>> {
    let manifest_path = root.join("Cargo.toml");
    let source = fs::read_to_string(&manifest_path)?;
    let version_dependency = format!("lightflow = {:?}", env!("CARGO_PKG_VERSION"));
    let path_dependency = format!(
        "lightflow = {{ version = {:?}, path = {:?} }}",
        env!("CARGO_PKG_VERSION"),
        env!("CARGO_MANIFEST_DIR")
    );
    fs::write(
        manifest_path,
        source.replace(&version_dependency, &path_dependency),
    )?;
    Ok(())
}
