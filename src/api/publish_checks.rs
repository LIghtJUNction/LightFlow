use std::fs;
use std::path::Path;
use toml_edit::{DocumentMut, Item};

mod dependencies;
pub(crate) use dependencies::{internal_path_dependency_packages, publish_issues};

pub(crate) fn cargo_publish_command(
    manifest_path: &Path,
    dry_run: bool,
    allow_dirty: bool,
) -> Vec<String> {
    let mut command = vec![
        "cargo".to_owned(),
        "publish".to_owned(),
        "--manifest-path".to_owned(),
        display_path(manifest_path),
    ];
    if allow_dirty {
        command.push("--allow-dirty".to_owned());
    }
    if dry_run {
        command.push("--dry-run".to_owned());
    }
    command
}

pub(crate) fn package_field_value(document: &DocumentMut, field: &str) -> Option<String> {
    document
        .get("package")
        .and_then(|package| package.get(field))
        .and_then(Item::as_str)
        .map(ToOwned::to_owned)
}

fn parse_cargo_manifest(source: &str) -> Result<DocumentMut, toml_edit::TomlError> {
    source.parse::<DocumentMut>()
}

pub(crate) fn read_cargo_manifest(path: &Path) -> Result<DocumentMut, CargoManifestReadError> {
    let source = fs::read_to_string(path).map_err(CargoManifestReadError::Io)?;
    parse_cargo_manifest(&source).map_err(CargoManifestReadError::Parse)
}

pub(crate) fn read_workspace_cargo_manifest(
    root: &Path,
) -> Result<Option<DocumentMut>, CargoManifestReadError> {
    let manifest_path = root.join("Cargo.toml");
    if !manifest_path.exists() {
        return Ok(None);
    }
    read_cargo_manifest(&manifest_path).map(Some)
}

#[derive(Debug)]
pub(crate) enum CargoManifestReadError {
    Io(std::io::Error),
    Parse(toml_edit::TomlError),
}

pub(crate) fn cargo_manifest_api_error(error: CargoManifestReadError) -> crate::api::ApiError {
    match error {
        CargoManifestReadError::Io(error) => crate::api::ApiError::Io(error),
        CargoManifestReadError::Parse(error) => {
            crate::api::ApiError::InvalidRequest(format!("invalid Cargo manifest: {error}"))
        }
    }
}

fn display_path(path: &std::path::Path) -> String {
    path.strip_prefix(".").unwrap_or(path).display().to_string()
}

#[cfg(test)]
mod tests;
