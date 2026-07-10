use super::{ApiError, ApiResult};
use crate::workflow::workflow_id_from_package_name;
use std::fs;
use std::path::Path;
use toml_edit::{DocumentMut, Item};

pub(crate) fn workflow_package_identity_from_source(path: &Path) -> ApiResult<(String, String)> {
    let crate_dir = path
        .parent()
        .and_then(Path::parent)
        .ok_or_else(|| invalid_manifest(path, "source is not under <crate>/src"))?;
    workflow_package_identity(&crate_dir.join("Cargo.toml"))
}

pub(crate) fn workflow_package_identity(manifest: &Path) -> ApiResult<(String, String)> {
    let document = read_manifest(manifest)?;
    let package = document
        .get("package")
        .and_then(Item::as_table)
        .ok_or_else(|| invalid_manifest(manifest, "missing [package] table"))?;
    let package_name = package
        .get("name")
        .and_then(Item::as_str)
        .ok_or_else(|| invalid_manifest(manifest, "missing package.name"))?;
    let version = match package.get("version") {
        Some(value) if value.as_str().is_some() => value.as_str().unwrap_or_default().to_owned(),
        Some(value) if inherits_workspace_value(value) => workspace_package_version(manifest)?,
        _ => return Err(invalid_manifest(manifest, "missing package.version")),
    };
    Ok((workflow_id_from_package_name(package_name), version))
}

fn inherits_workspace_value(item: &Item) -> bool {
    item.as_inline_table()
        .and_then(|table| table.get("workspace"))
        .and_then(|value| value.as_bool())
        .or_else(|| {
            item.as_table()
                .and_then(|table| table.get("workspace"))
                .and_then(Item::as_bool)
        })
        == Some(true)
}

fn workspace_package_version(manifest: &Path) -> ApiResult<String> {
    for ancestor in manifest.parent().into_iter().flat_map(Path::ancestors) {
        let candidate = ancestor.join("Cargo.toml");
        let Ok(document) = read_manifest(&candidate) else {
            continue;
        };
        if let Some(version) = document
            .get("workspace")
            .and_then(Item::as_table)
            .and_then(|workspace| workspace.get("package"))
            .and_then(Item::as_table)
            .and_then(|package| package.get("version"))
            .and_then(Item::as_str)
        {
            return Ok(version.to_owned());
        }
    }
    Err(invalid_manifest(
        manifest,
        "package.version inherits from workspace but workspace.package.version is missing",
    ))
}

fn read_manifest(path: &Path) -> ApiResult<DocumentMut> {
    fs::read_to_string(path)
        .map_err(ApiError::from)?
        .parse::<DocumentMut>()
        .map_err(|error| invalid_manifest(path, &error.to_string()))
}

fn invalid_manifest(path: &Path, issue: &str) -> ApiError {
    ApiError::InvalidRequest(format!(
        "invalid workflow manifest {}: {issue}",
        path.display()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn package_identity_maps_name_and_explicit_version() {
        let root = tempfile::tempdir().expect("tempdir");
        let manifest = root.path().join("Cargo.toml");
        fs::write(
            &manifest,
            "[package]\nname = \"lightflow-image-upscale\"\nversion = \"1.2.3\"\n",
        )
        .expect("manifest");

        assert_eq!(
            workflow_package_identity(&manifest).expect("identity"),
            ("lightflow.image_upscale".to_owned(), "1.2.3".to_owned())
        );
    }

    #[test]
    fn package_identity_reads_workspace_version() {
        let root = tempfile::tempdir().expect("tempdir");
        fs::write(
            root.path().join("Cargo.toml"),
            "[workspace]\nmembers = [\"flow\"]\n[workspace.package]\nversion = \"2.0.0\"\n",
        )
        .expect("workspace manifest");
        let crate_dir = root.path().join("flow");
        fs::create_dir(&crate_dir).expect("crate dir");
        let manifest = crate_dir.join("Cargo.toml");
        fs::write(
            &manifest,
            "[package]\nname = \"my-workflow\"\nversion.workspace = true\n",
        )
        .expect("crate manifest");

        assert_eq!(
            workflow_package_identity(&manifest).expect("identity"),
            ("lightflow.my_workflow".to_owned(), "2.0.0".to_owned())
        );
    }
}
