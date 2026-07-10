use super::dsl::read_workflow_source;
use super::workflow_package_identity;
pub(crate) use paths::categorized_workflow_manifest_path;
use paths::workflow_lib_path;
pub(crate) use placeholders::workflow_placeholder_issues;
use std::path::Path;

mod paths;
mod placeholders;

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
    workflow_package_identity(manifest).ok().map(|(id, _)| id)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{LEGACY_LIGHTFLOW_DIR, PROJECT_LIGHTFLOW_DIR, WORKFLOW_DIR};
    use crate::workflow::{PortSpec, WorkflowSpec};
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
        fs::write(
            crate_dir.join("Cargo.toml"),
            "[package]\nname = \"lightflow-metadata\"\nversion = \"0.1.0\"\n",
        )
        .expect("manifest");
        fs::write(
            crate_dir.join("src/lib.rs"),
            r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
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
        fs::write(
            crate_dir.join("Cargo.toml"),
            "[package]\nname = \"lightflow-metadata-id\"\nversion = \"0.1.0\"\n",
        )
        .expect("manifest");
        fs::write(
            crate_dir.join("src/lib.rs"),
            r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
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
            categorized_workflow_manifest_path(root.path(), "lightflow.text_plan").unwrap(),
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
            categorized_workflow_manifest_path(root.path(), "lightflow.text_plan").unwrap(),
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
