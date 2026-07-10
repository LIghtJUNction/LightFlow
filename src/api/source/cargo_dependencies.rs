use super::normalize_existing_path;
use super::path_dependencies::dependency_category;
use crate::api::dsl::read_optional_workflow_source_from_manifest;
use crate::api::{ApiError, ApiResult};
use crate::workflow::WorkflowSpec;
use serde::Deserialize;
use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::{Path, PathBuf};
use std::process::Command;

pub(super) fn read_cargo_dependency_workflows(
    root_manifest: &Path,
    workflows: &mut Vec<WorkflowSpec>,
    manifests: &mut BTreeSet<PathBuf>,
    visited_libs: &mut BTreeSet<PathBuf>,
) -> ApiResult<()> {
    let metadata = cargo_metadata(root_manifest)?;
    discover_metadata_workflows(&metadata, workflows, manifests, visited_libs)
}

fn discover_metadata_workflows(
    metadata: &CargoMetadata,
    workflows: &mut Vec<WorkflowSpec>,
    manifests: &mut BTreeSet<PathBuf>,
    visited_libs: &mut BTreeSet<PathBuf>,
) -> ApiResult<()> {
    let packages = metadata
        .packages
        .iter()
        .map(|package| (package.id.as_str(), package))
        .collect::<BTreeMap<_, _>>();
    let nodes = metadata
        .resolve
        .as_ref()
        .map(|resolve| {
            resolve
                .nodes
                .iter()
                .map(|node| (node.id.as_str(), node))
                .collect::<BTreeMap<_, _>>()
        })
        .unwrap_or_default();
    let mut queue = VecDeque::new();
    for member in &metadata.workspace_members {
        enqueue_normal_dependencies(member, &nodes, &mut queue);
    }

    let mut inspected = BTreeSet::new();
    while let Some(package_id) = queue.pop_front() {
        if !inspected.insert(package_id.clone()) {
            continue;
        }
        let Some(package) = packages.get(package_id.as_str()) else {
            continue;
        };
        let Some(lib) = package.library_source() else {
            continue;
        };
        let lib = normalize_existing_path(lib)?;
        let Some(mut workflow) =
            read_optional_workflow_source_from_manifest(&lib, &package.manifest_path)?
        else {
            continue;
        };
        workflow.category = Some(
            package
                .manifest_path
                .parent()
                .and_then(dependency_category)
                .unwrap_or_else(|| "extensions".to_owned()),
        );
        if visited_libs.insert(lib) {
            workflows.push(workflow);
        }
        manifests.insert(normalize_existing_path(&package.manifest_path)?);
        enqueue_normal_dependencies(&package.id, &nodes, &mut queue);
    }
    Ok(())
}

fn enqueue_normal_dependencies(
    package_id: &str,
    nodes: &BTreeMap<&str, &MetadataNode>,
    queue: &mut VecDeque<String>,
) {
    let Some(node) = nodes.get(package_id) else {
        return;
    };
    queue.extend(
        node.deps
            .iter()
            .filter(|dependency| dependency.is_normal())
            .map(|dependency| dependency.pkg.clone()),
    );
}

fn cargo_metadata(manifest: &Path) -> ApiResult<CargoMetadata> {
    let output = cargo_metadata_bytes_with(manifest, |offline| {
        run_cargo_metadata_command(manifest, offline)
    })?;
    serde_json::from_slice(&output).map_err(|error| {
        ApiError::InvalidRequest(format!(
            "cargo metadata returned invalid JSON for {}: {error}",
            manifest.display()
        ))
    })
}

fn cargo_metadata_bytes_with(
    manifest: &Path,
    mut run: impl FnMut(bool) -> Result<MetadataCommandOutput, std::io::Error>,
) -> ApiResult<Vec<u8>> {
    let offline = run(true).map_err(|error| metadata_spawn_error(manifest, error))?;
    if offline.success {
        return Ok(offline.stdout);
    }
    let online = run(false).map_err(|error| metadata_spawn_error(manifest, error))?;
    if online.success {
        return Ok(online.stdout);
    }
    Err(ApiError::InvalidRequest(format!(
        "cargo metadata failed for {}; offline attempt: {}; normal Cargo fallback: {}; run cargo fetch and retry",
        manifest.display(),
        metadata_stderr(&offline.stderr),
        metadata_stderr(&online.stderr)
    )))
}

fn run_cargo_metadata_command(
    manifest: &Path,
    offline: bool,
) -> Result<MetadataCommandOutput, std::io::Error> {
    let mut command = Command::new("cargo");
    command
        .arg("metadata")
        .args(["--format-version", "1"])
        .arg("--manifest-path")
        .arg(manifest);
    if offline {
        command.arg("--offline");
    }
    command.output().map(|output| MetadataCommandOutput {
        success: output.status.success(),
        stdout: output.stdout,
        stderr: output.stderr,
    })
}

fn metadata_spawn_error(manifest: &Path, error: std::io::Error) -> ApiError {
    ApiError::InvalidRequest(format!(
        "failed to run cargo metadata for {}: {error}",
        manifest.display()
    ))
}

fn metadata_stderr(stderr: &[u8]) -> String {
    let stderr = String::from_utf8_lossy(stderr);
    let stderr = stderr.trim();
    if stderr.is_empty() {
        "no stderr output".to_owned()
    } else {
        stderr.to_owned()
    }
}

struct MetadataCommandOutput {
    success: bool,
    stdout: Vec<u8>,
    stderr: Vec<u8>,
}

#[derive(Debug, Deserialize)]
struct CargoMetadata {
    packages: Vec<MetadataPackage>,
    workspace_members: Vec<String>,
    resolve: Option<MetadataResolve>,
}

#[derive(Debug, Deserialize)]
struct MetadataPackage {
    id: String,
    manifest_path: PathBuf,
    targets: Vec<MetadataTarget>,
}

impl MetadataPackage {
    fn library_source(&self) -> Option<&Path> {
        self.targets
            .iter()
            .find(|target| target.kind.iter().any(|kind| kind == "lib"))
            .map(|target| target.src_path.as_path())
    }
}

#[derive(Debug, Deserialize)]
struct MetadataTarget {
    kind: Vec<String>,
    src_path: PathBuf,
}

#[derive(Debug, Deserialize)]
struct MetadataResolve {
    nodes: Vec<MetadataNode>,
}

#[derive(Debug, Deserialize)]
struct MetadataNode {
    id: String,
    deps: Vec<MetadataDependency>,
}

#[derive(Debug, Deserialize)]
struct MetadataDependency {
    pkg: String,
    dep_kinds: Vec<MetadataDependencyKind>,
}

impl MetadataDependency {
    fn is_normal(&self) -> bool {
        self.dep_kinds
            .iter()
            .any(|kind| kind.kind.as_deref().is_none_or(|kind| kind == "normal"))
    }
}

#[derive(Debug, Deserialize)]
struct MetadataDependencyKind {
    kind: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn discovers_direct_workflow_dependency() {
        let fixture = Fixture::new();
        fixture.root("direct = { path = \"direct\", package = \"lightflow-direct\" }");
        fixture.workflow("direct", "lightflow-direct", "");
        assert_eq!(fixture.discover(), vec!["lightflow.direct"]);
    }

    #[test]
    fn does_not_scan_dependencies_of_non_workflows() {
        let fixture = Fixture::new();
        fixture.root("ordinary = { path = \"ordinary\" }");
        fixture.library(
            "ordinary",
            "ordinary",
            "hidden = { path = \"../hidden\", package = \"lightflow-hidden\" }",
        );
        fixture.workflow("hidden", "lightflow-hidden", "");
        assert!(fixture.discover().is_empty());
    }

    #[test]
    fn recurses_through_workflow_dependencies() {
        let fixture = Fixture::new();
        fixture.root("parent = { path = \"parent\", package = \"lightflow-parent\" }");
        fixture.workflow(
            "parent",
            "lightflow-parent",
            "child = { path = \"../child\", package = \"lightflow-child\" }",
        );
        fixture.workflow("child", "lightflow-child", "");
        assert_eq!(
            fixture.discover(),
            vec!["lightflow.child", "lightflow.parent"]
        );
    }

    #[test]
    fn discovers_custom_library_target_path() {
        let fixture = Fixture::new();
        fixture.root("custom = { path = \"custom\", package = \"lightflow-custom\" }");
        fixture.custom_lib_workflow("custom", "lightflow-custom");
        assert_eq!(fixture.discover(), vec!["lightflow.custom"]);
    }

    #[test]
    fn existing_stale_lock_is_updated_by_metadata() {
        let fixture = Fixture::new();
        fixture.root("first = { path = \"first\", package = \"lightflow-first\" }");
        fixture.workflow("first", "lightflow-first", "");
        assert_eq!(fixture.discover(), vec!["lightflow.first"]);
        assert!(fixture.0.path().join("Cargo.lock").is_file());

        fixture.root(
            "first = { path = \"first\", package = \"lightflow-first\" }\nsecond = { path = \"second\", package = \"lightflow-second\" }",
        );
        fixture.workflow("second", "lightflow-second", "");

        assert_eq!(
            fixture.discover(),
            vec!["lightflow.first", "lightflow.second"]
        );
    }

    #[test]
    fn offline_failure_falls_back_to_normal_cargo_resolution() {
        let mut attempts = Vec::new();
        let output = cargo_metadata_bytes_with(Path::new("Cargo.toml"), |offline| {
            attempts.push(offline);
            Ok(MetadataCommandOutput {
                success: !offline,
                stdout: if offline { Vec::new() } else { b"{}".to_vec() },
                stderr: if offline {
                    b"not cached".to_vec()
                } else {
                    Vec::new()
                },
            })
        })
        .expect("fallback output");
        assert_eq!(attempts, vec![true, false]);
        assert_eq!(output, b"{}");
    }

    struct Fixture(tempfile::TempDir);

    impl Fixture {
        fn new() -> Self {
            Self(tempfile::tempdir().expect("tempdir"))
        }

        fn root(&self, dependencies: &str) {
            self.library(".", "fixture-app", dependencies);
        }

        fn workflow(&self, relative: &str, package: &str, dependencies: &str) {
            self.package(
                relative,
                package,
                dependencies,
                r#"use lightflow::preload::*;
pub fn define() -> WorkflowSpec {
    workflow!().name("Fixture").build()
}
"#,
            );
        }

        fn custom_lib_workflow(&self, relative: &str, package: &str) {
            let dir = self.0.path().join(relative);
            fs::create_dir_all(&dir).expect("crate dir");
            fs::write(
                dir.join("Cargo.toml"),
                format!(
                    "[package]\nname = {package:?}\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[lib]\npath = \"workflow.rs\"\n"
                ),
            )
            .expect("manifest");
            fs::write(
                dir.join("workflow.rs"),
                "use lightflow::preload::*;\npub fn define() -> WorkflowSpec { workflow!().name(\"Custom\").build() }\n",
            )
            .expect("custom lib source");
        }

        fn library(&self, relative: &str, package: &str, dependencies: &str) {
            self.package(relative, package, dependencies, "pub fn library() {}\n");
        }

        fn package(&self, relative: &str, package: &str, dependencies: &str, source: &str) {
            let dir = self.0.path().join(relative);
            fs::create_dir_all(dir.join("src")).expect("source dir");
            fs::write(
                dir.join("Cargo.toml"),
                format!(
                    "[package]\nname = {package:?}\nversion = \"0.1.0\"\nedition = \"2024\"\n\n[dependencies]\n{dependencies}\n"
                ),
            )
            .expect("manifest");
            fs::write(dir.join("src/lib.rs"), source).expect("source");
        }

        fn discover(&self) -> Vec<String> {
            let mut workflows = Vec::new();
            read_cargo_dependency_workflows(
                &self.0.path().join("Cargo.toml"),
                &mut workflows,
                &mut BTreeSet::new(),
                &mut BTreeSet::new(),
            )
            .expect("metadata discovery");
            let mut ids = workflows
                .into_iter()
                .map(|workflow| workflow.id)
                .collect::<Vec<_>>();
            ids.sort();
            ids
        }
    }
}
