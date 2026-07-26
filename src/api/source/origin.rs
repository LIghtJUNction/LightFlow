use super::WorkflowOrigin;
use crate::api::{ApiError, ApiResult};
use crate::workflow::WorkflowSpec;
use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use toml_edit::{DocumentMut, Item, TableLike};

pub(in crate::api) fn read_workflow_origin(manifest: &Path) -> ApiResult<WorkflowOrigin> {
    let source = fs::read_to_string(manifest).map_err(ApiError::from)?;
    let document = source.parse::<DocumentMut>().map_err(|error| {
        ApiError::InvalidRequest(format!(
            "invalid Cargo manifest {}: {error}",
            manifest.display()
        ))
    })?;
    let package = document
        .get("package")
        .and_then(Item::as_table_like)
        .ok_or_else(|| {
            ApiError::InvalidRequest(format!(
                "workflow Cargo manifest {} has no [package] table",
                manifest.display()
            ))
        })?;
    let package_name = package
        .get("name")
        .and_then(Item::as_str)
        .filter(|name| !name.trim().is_empty())
        .ok_or_else(|| {
            ApiError::InvalidRequest(format!(
                "workflow Cargo manifest {} has no package name",
                manifest.display()
            ))
        })?
        .to_owned();
    let runner_bin = package
        .get("metadata")
        .and_then(|metadata| metadata.get("lightflow"))
        .and_then(|lightflow| lightflow.get("runner"))
        .map(|runner| {
            runner
                .as_str()
                .filter(|value| !value.trim().is_empty())
                .map(str::to_owned)
                .ok_or_else(|| {
                    ApiError::InvalidRequest(format!(
                        "workflow package {package_name} metadata lightflow.runner must be a non-empty string"
                    ))
                })
        })
        .transpose()?;
    let runner_features = read_runner_features(package, &package_name)?;
    if runner_bin.is_none() && !runner_features.is_empty() {
        return Err(ApiError::InvalidRequest(format!(
            "workflow package {package_name} declares runner-features without a runner"
        )));
    }
    if let Some(runner) = runner_bin.as_deref() {
        validate_runner_bin(manifest, &document, &package_name, runner)?;
    }
    Ok(WorkflowOrigin {
        manifest_path: manifest.to_path_buf(),
        package_name,
        runner_bin,
        runner_features,
    })
}

fn read_runner_features(package: &dyn TableLike, package_name: &str) -> ApiResult<Vec<String>> {
    package
        .get("metadata")
        .and_then(|metadata| metadata.get("lightflow"))
        .and_then(|lightflow| lightflow.get("runner-features"))
        .map(|features| {
            let features = features.as_array().ok_or_else(|| {
                ApiError::InvalidRequest(format!(
                    "workflow package {package_name} metadata lightflow.runner-features must be an array"
                ))
            })?;
            let mut validated = BTreeSet::new();
            for feature in features {
                let feature = feature
                    .as_str()
                    .filter(|value| {
                        !value.is_empty()
                            && value.len() <= 64
                            && value.bytes().all(|byte| {
                                byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_')
                            })
                    })
                    .ok_or_else(|| {
                        ApiError::InvalidRequest(format!(
                            "workflow package {package_name} metadata lightflow.runner-features contains an invalid Cargo feature"
                        ))
                    })?;
                validated.insert(feature.to_owned());
            }
            Ok(validated.into_iter().collect())
        })
        .transpose()
        .map(Option::unwrap_or_default)
}

fn validate_runner_bin(
    manifest: &Path,
    document: &DocumentMut,
    package_name: &str,
    runner: &str,
) -> ApiResult<()> {
    let declared = document
        .get("bin")
        .and_then(Item::as_array_of_tables)
        .is_some_and(|bins| {
            bins.iter()
                .any(|bin| bin.get("name").and_then(Item::as_str) == Some(runner))
        });
    let package_root = manifest.parent().ok_or_else(|| {
        ApiError::InvalidRequest(format!(
            "Cargo manifest {} has no parent",
            manifest.display()
        ))
    })?;
    let implicit = package_root
        .join("src/bin")
        .join(format!("{runner}.rs"))
        .is_file()
        || package_root
            .join("src/bin")
            .join(runner)
            .join("main.rs")
            .is_file()
        || (runner == package_name && package_root.join("src/main.rs").is_file());
    if declared || implicit {
        Ok(())
    } else {
        Err(ApiError::InvalidRequest(format!(
            "workflow package {package_name} declares runner {runner}, but no matching Cargo bin target exists"
        )))
    }
}

pub(in crate::api) fn apply_runner_runtime(workflow: &mut WorkflowSpec, origin: &WorkflowOrigin) {
    if origin.runner_bin.is_none()
        || workflow
            .runtimes
            .iter()
            .any(|runtime| runtime.engine.as_deref() == Some(crate::api::plan::RUNNER_ENGINE))
    {
        return;
    }
    workflow.runtimes.push(crate::workflow::RuntimeRequirement {
        id: "runner".to_owned(),
        capability: crate::api::plan::RUNNER_CAPABILITY.to_owned(),
        engine: Some(crate::api::plan::RUNNER_ENGINE.to_owned()),
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn manifest(source: &str, has_runner: bool) -> tempfile::TempDir {
        let root = tempfile::tempdir().expect("tempdir");
        if has_runner {
            fs::create_dir_all(root.path().join("src/bin")).expect("bin dir");
            fs::write(root.path().join("src/bin/runner.rs"), "fn main() {}\n")
                .expect("runner source");
        }
        fs::write(root.path().join("Cargo.toml"), source).expect("manifest");
        root
    }

    #[test]
    fn reads_explicit_runner_and_features() {
        let root = manifest(
            "[package]\nname = \"lightflow-fixture\"\nversion = \"0.1.0\"\n\
             [package.metadata.lightflow]\nrunner = \"runner\"\n\
             runner-features = [\"native\", \"gpu_2\"]\n",
            true,
        );
        let origin =
            read_workflow_origin(&root.path().join("Cargo.toml")).expect("workflow origin");
        assert_eq!(origin.package_name, "lightflow-fixture");
        assert_eq!(origin.runner_bin.as_deref(), Some("runner"));
        assert_eq!(origin.runner_features, ["gpu_2", "native"]);
    }

    #[test]
    fn rejects_unsafe_features_and_missing_target() {
        let unsafe_root = manifest(
            "[package]\nname = \"lightflow-fixture\"\nversion = \"0.1.0\"\n\
             [package.metadata.lightflow]\nrunner = \"runner\"\n\
             runner-features = [\"native,evil\"]\n",
            true,
        );
        let error = read_workflow_origin(&unsafe_root.path().join("Cargo.toml"))
            .expect_err("unsafe feature");
        assert!(error.to_string().contains("invalid Cargo feature"));

        let missing_root = manifest(
            "[package]\nname = \"lightflow-fixture\"\nversion = \"0.1.0\"\n\
             [package.metadata.lightflow]\nrunner = \"missing\"\n",
            false,
        );
        let error = read_workflow_origin(&missing_root.path().join("Cargo.toml"))
            .expect_err("missing runner target");
        assert!(
            error
                .to_string()
                .contains("no matching Cargo bin target exists")
        );
    }
}
