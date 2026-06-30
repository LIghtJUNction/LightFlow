use super::dsl::read_workflow_source;
use super::{LEGACY_LIGHTFLOW_DIR, PROJECT_LIGHTFLOW_DIR, WORKFLOW_DIR, util};
use crate::workflow::{PortSpec, WorkflowSpec};
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

pub(crate) fn categorized_workflow_manifest_path(
    root: &Path,
    workflow_id: &str,
) -> io::Result<PathBuf> {
    let project_workflows = root.join(PROJECT_LIGHTFLOW_DIR).join(WORKFLOW_DIR);
    let workflows = root.join(WORKFLOW_DIR);
    let legacy_workflows = root.join(LEGACY_LIGHTFLOW_DIR).join(WORKFLOW_DIR);
    let entries = match fs::read_dir(&project_workflows).or_else(|error| {
        if error.kind() == io::ErrorKind::NotFound {
            fs::read_dir(&workflows).or_else(|error| {
                if error.kind() == io::ErrorKind::NotFound {
                    fs::read_dir(&legacy_workflows)
                } else {
                    Err(error)
                }
            })
        } else {
            Err(error)
        }
    }) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(project_workflows.join(workflow_id).join("Cargo.toml"));
        }
        Err(error) => return Err(error),
    };
    for entry in entries {
        let path = entry?.path();
        if !path.is_dir() || path.join("src").join("lib.rs").exists() {
            continue;
        }
        let category = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("");
        let manifest = path
            .join(util::workflow_crate_dir_name(workflow_id))
            .join("Cargo.toml");
        if manifest.exists() {
            return Ok(manifest);
        }
        if let Some(short_name) = workflow_category_short_name(workflow_id, category) {
            let manifest = path.join(short_name).join("Cargo.toml");
            if manifest.exists() {
                return Ok(manifest);
            }
        }
    }
    Ok(project_workflows.join(workflow_id).join("Cargo.toml"))
}

pub(crate) fn workflow_publish_metadata_issues(manifest: &Path) -> Vec<String> {
    let Some(lib) = workflow_lib_path(manifest) else {
        return Vec::new();
    };
    if !lib.exists() {
        return Vec::new();
    }
    match read_workflow_source(&lib) {
        Ok(workflow) => workflow_placeholder_issues(&workflow),
        Err(error) => vec![format!("workflow source cannot be parsed: {error}")],
    }
}

pub(crate) fn workflow_id_from_manifest(manifest: &Path) -> Option<String> {
    let lib = workflow_lib_path(manifest)?;
    read_workflow_source(&lib).ok().map(|workflow| workflow.id)
}

pub(crate) fn workflow_placeholder_issues(workflow: &WorkflowSpec) -> Vec<String> {
    let mut issues = Vec::new();
    if unresolved_placeholder(workflow.description.as_deref()) {
        issues.push("workflow.description contains unresolved TODO".to_owned());
    }
    collect_port_placeholder_issues("input", &workflow.inputs, &mut issues);
    collect_port_placeholder_issues("output", &workflow.outputs, &mut issues);
    issues
}

fn collect_port_placeholder_issues(kind: &str, ports: &[PortSpec], issues: &mut Vec<String>) {
    for port in ports {
        if unresolved_placeholder(port.description.as_deref()) {
            issues.push(format!(
                "workflow.{kind}.{}.description contains unresolved TODO",
                port.name
            ));
        }
    }
}

fn unresolved_placeholder(value: Option<&str>) -> bool {
    value.is_some_and(|value| value.to_ascii_lowercase().contains("todo"))
}

fn workflow_lib_path(manifest: &Path) -> Option<std::path::PathBuf> {
    Some(manifest.parent()?.join("src").join("lib.rs"))
}

fn workflow_category_short_name(workflow_id: &str, category: &str) -> Option<String> {
    let prefixed = workflow_id
        .strip_prefix("lightflow.")
        .unwrap_or(workflow_id);
    let short = prefixed.strip_prefix(category)?.strip_prefix('.')?;
    Some(short.replace('.', "_"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn workflow_placeholder_issues_report_workflow_and_port_descriptions() {
        let workflow = WorkflowSpec {
            id: "lightflow.placeholder".to_owned(),
            version: "0.1.0".to_owned(),
            name: "Placeholder".to_owned(),
            category: None,
            description: Some("TODO: describe workflow".to_owned()),
            inputs: vec![port("value", "todo: describe input")],
            outputs: vec![port("result", "TODO: describe output")],
            config_schema: serde_json::Value::Null,
            dependencies: Vec::new(),
            models: Vec::new(),
            runtimes: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
        };

        assert_eq!(
            workflow_placeholder_issues(&workflow),
            vec![
                "workflow.description contains unresolved TODO",
                "workflow.input.value.description contains unresolved TODO",
                "workflow.output.result.description contains unresolved TODO",
            ]
        );
    }

    #[test]
    fn workflow_publish_metadata_issues_reads_manifest_workflow_source() {
        let root = test_dir("metadata-valid");
        let crate_dir = root.path().join("workflow");
        fs::create_dir_all(crate_dir.join("src")).expect("workflow source dir");
        fs::write(crate_dir.join("Cargo.toml"), "[package]\nname = \"demo\"\n").expect("manifest");
        fs::write(
            crate_dir.join("src/lib.rs"),
            r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.metadata")
        .version("0.1.0")
        .name("Metadata")
        .description("TODO: describe this workflow.")
        .input("value", "text")
        .input_description("value", "Input text.")
        .output("result", "text")
        .output_description("result", "Result text.")
        .build()
}
"#,
        )
        .expect("workflow source");

        assert_eq!(
            workflow_publish_metadata_issues(&crate_dir.join("Cargo.toml")),
            vec!["workflow.description contains unresolved TODO"]
        );
    }

    #[test]
    fn workflow_id_from_manifest_reads_workflow_source() {
        let root = test_dir("metadata-id");
        let crate_dir = root.path().join("workflow");
        fs::create_dir_all(crate_dir.join("src")).expect("workflow source dir");
        fs::write(crate_dir.join("Cargo.toml"), "[package]\nname = \"demo\"\n").expect("manifest");
        fs::write(
            crate_dir.join("src/lib.rs"),
            r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.metadata_id")
        .version("0.1.0")
        .name("Metadata ID")
        .build()
}
"#,
        )
        .expect("workflow source");

        assert_eq!(
            workflow_id_from_manifest(&crate_dir.join("Cargo.toml")).as_deref(),
            Some("lightflow.metadata_id")
        );
    }

    #[test]
    fn workflow_publish_metadata_issues_ignores_manifests_without_workflow_source() {
        let root = test_dir("metadata-no-lib");
        let crate_dir = root.path().join("workflow");
        fs::create_dir_all(&crate_dir).expect("workflow dir");
        fs::write(crate_dir.join("Cargo.toml"), "[package]\nname = \"demo\"\n").expect("manifest");

        assert!(workflow_publish_metadata_issues(&crate_dir.join("Cargo.toml")).is_empty());
    }

    #[test]
    fn workflow_publish_metadata_issues_reports_unparseable_workflow_source() {
        let root = test_dir("metadata-invalid");
        let crate_dir = root.path().join("workflow");
        fs::create_dir_all(crate_dir.join("src")).expect("workflow source dir");
        fs::write(crate_dir.join("Cargo.toml"), "[package]\nname = \"demo\"\n").expect("manifest");
        fs::write(crate_dir.join("src/lib.rs"), "pub fn define(").expect("workflow source");

        let issues = workflow_publish_metadata_issues(&crate_dir.join("Cargo.toml"));
        assert_eq!(issues.len(), 1);
        assert!(issues[0].starts_with("workflow source cannot be parsed:"));
    }

    #[test]
    fn categorized_workflow_manifest_path_finds_category_short_name() {
        let root = test_dir("manifest-category-short-name");
        let crate_dir = root
            .path()
            .join(PROJECT_LIGHTFLOW_DIR)
            .join(WORKFLOW_DIR)
            .join("text")
            .join("plan");
        fs::create_dir_all(&crate_dir).expect("workflow crate dir");
        fs::write(crate_dir.join("Cargo.toml"), "[package]\nname = \"demo\"\n").expect("manifest");

        assert_eq!(
            categorized_workflow_manifest_path(root.path(), "lightflow.text.plan").unwrap(),
            crate_dir.join("Cargo.toml")
        );
    }

    #[test]
    fn categorized_workflow_manifest_path_falls_back_to_project_workflows() {
        let root = test_dir("manifest-fallback");

        assert_eq!(
            categorized_workflow_manifest_path(root.path(), "lightflow.missing").unwrap(),
            root.path()
                .join(PROJECT_LIGHTFLOW_DIR)
                .join(WORKFLOW_DIR)
                .join("lightflow.missing")
                .join("Cargo.toml")
        );
    }

    #[test]
    fn categorized_workflow_manifest_path_falls_back_to_legacy_directory() {
        let root = test_dir("manifest-legacy-directory");
        let crate_dir = root
            .path()
            .join(LEGACY_LIGHTFLOW_DIR)
            .join(WORKFLOW_DIR)
            .join("text")
            .join("plan");
        fs::create_dir_all(&crate_dir).expect("workflow crate dir");
        fs::write(crate_dir.join("Cargo.toml"), "[package]\nname = \"demo\"\n").expect("manifest");

        assert_eq!(
            categorized_workflow_manifest_path(root.path(), "lightflow.text.plan").unwrap(),
            crate_dir.join("Cargo.toml")
        );
    }

    fn port(name: &str, description: &str) -> PortSpec {
        let mut port = PortSpec::new(name, "text");
        port.description = Some(description.to_owned());
        port
    }

    struct TestDir {
        path: std::path::PathBuf,
    }

    impl TestDir {
        fn path(&self) -> &std::path::Path {
            &self.path
        }
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
                "lightflow-workflow-metadata-{name}-{}-{nanos}",
                std::process::id()
            )),
        }
    }
}
