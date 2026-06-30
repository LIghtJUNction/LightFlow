use crate::api::{CargoManifestReadError, read_cargo_manifest, read_workspace_cargo_manifest};
use crate::cli::{CliError, CliResult, run_status};
use std::path::{Path, PathBuf};
use std::process::Command;
use toml_edit::DocumentMut;

pub(super) fn workspace_document(root: &Path) -> CliResult<Option<DocumentMut>> {
    read_workspace_cargo_manifest(root).map_err(cargo_manifest_error)
}

pub(super) fn workspace_root_for_manifest(repo_root: &Path, manifest: &Path) -> CliResult<PathBuf> {
    let crate_dir = manifest.parent().unwrap_or(repo_root);
    for ancestor in crate_dir.ancestors() {
        let manifest = ancestor.join("Cargo.toml");
        if manifest.exists() {
            let document = match read_cargo_manifest(&manifest) {
                Ok(document) => document,
                Err(CargoManifestReadError::Parse(_)) if ancestor != repo_root => continue,
                Err(error) => return Err(cargo_manifest_error(error)),
            };
            if document.get("workspace").is_some() {
                return Ok(ancestor.to_path_buf());
            }
        }
        if ancestor == repo_root {
            break;
        }
    }
    Ok(crate_dir.to_path_buf())
}

pub(super) fn run_cargo_command(command: &[String]) -> CliResult<()> {
    let mut process = Command::new("cargo");
    for arg in &command[1..] {
        process.arg(arg);
    }
    run_status(&mut process)
}

pub(super) fn cargo_manifest_error(error: CargoManifestReadError) -> CliError {
    match error {
        CargoManifestReadError::Io(error) => CliError::Io(error),
        CargoManifestReadError::Parse(error) => {
            CliError::Usage(format!("invalid Cargo manifest: {error}"))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn workspace_root_for_manifest_finds_parent_workspace_manifest() {
        let root = test_dir("workspace-root-parent");
        let workspace = root.path.join("vendor").join("workspace");
        let crate_dir = workspace.join("crates").join("app");
        fs::create_dir_all(&crate_dir).expect("crate dir");
        fs::write(workspace.join("Cargo.toml"), "[workspace]\nmembers = []\n")
            .expect("workspace manifest");
        fs::write(
            crate_dir.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        )
        .expect("crate manifest");

        assert_eq!(
            workspace_root_for_manifest(&root.path, &crate_dir.join("Cargo.toml"))
                .expect("workspace root"),
            workspace
        );
    }

    #[test]
    fn workspace_root_for_manifest_falls_back_to_crate_dir() {
        let root = test_dir("workspace-root-crate");
        let crate_dir = root.path.join("extensions").join("app");
        fs::create_dir_all(&crate_dir).expect("crate dir");
        fs::write(
            crate_dir.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        )
        .expect("crate manifest");

        assert_eq!(
            workspace_root_for_manifest(&root.path, &crate_dir.join("Cargo.toml"))
                .expect("workspace root"),
            crate_dir
        );
    }

    #[test]
    fn workspace_root_for_manifest_skips_invalid_intermediate_manifest() {
        let root = test_dir("workspace-root-invalid-intermediate");
        let workspace = root.path.join("vendor").join("workspace");
        let crate_dir = workspace.join("crates").join("app");
        fs::create_dir_all(&crate_dir).expect("crate dir");
        fs::write(workspace.join("Cargo.toml"), "[workspace]\nmembers = []\n")
            .expect("workspace manifest");
        fs::write(
            workspace.join("crates").join("Cargo.toml"),
            "this is not toml =",
        )
        .expect("intermediate manifest");
        fs::write(
            crate_dir.join("Cargo.toml"),
            "[package]\nname = \"app\"\nversion = \"0.1.0\"\n",
        )
        .expect("crate manifest");

        assert_eq!(
            workspace_root_for_manifest(&root.path, &crate_dir.join("Cargo.toml"))
                .expect("workspace root"),
            workspace
        );
    }

    struct TestDir {
        path: PathBuf,
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    fn test_dir(name: &str) -> TestDir {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock after unix epoch")
            .as_nanos();
        TestDir {
            path: std::env::temp_dir().join(format!(
                "lightflow-cli-publish-cargo-{name}-{}-{nanos}",
                std::process::id()
            )),
        }
    }
}
