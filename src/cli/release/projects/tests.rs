use super::*;
use std::fs;
use std::time::{SystemTime, UNIX_EPOCH};

#[test]
fn parses_projects_release_options_and_rejects_invalid_semver() {
    let options = parse_options(&strings(["1.2.3", "--apply", "--publish", "--allow-dirty"]))
        .expect("valid options");
    assert_eq!(options.version, Version::parse("1.2.3").unwrap());
    assert!(options.apply);
    assert!(options.publish);
    assert!(options.allow_dirty);

    let error = parse_options(&strings(["not-semver"])).expect_err("invalid SemVer");
    assert!(error.to_string().contains("invalid release SemVer"));
    assert!(parse_options(&strings(["1.2.3", "--allow-dirty"])).is_err());
}

#[test]
fn dry_run_is_read_only_and_publish_is_only_planned() {
    let fixture = Fixture::new("dry-run");
    fixture.write_release_train(true);
    let root_before = fs::read_to_string(fixture.path.join("Cargo.toml")).unwrap();
    let workflow_before =
        fs::read_to_string(fixture.workflow_manifest("lightflow-std", "example")).unwrap();

    let report = release_projects(
        &ApiService::new(&fixture.path),
        &strings(["2.0.0", "--publish"]),
    )
    .expect("release plan");

    assert_eq!(report["dry_run"], true);
    assert_eq!(report["apply"], false);
    assert_eq!(report["publish"], true);
    assert_eq!(report["executed"], serde_json::json!([]));
    let order = report["publish_order"].as_array().unwrap();
    assert_eq!(order[0], "lightflow");
    assert_eq!(order[1], "lightflow-support");
    assert_eq!(order[2], "lightflow-example");
    assert_eq!(
        fs::read_to_string(fixture.path.join("Cargo.toml")).unwrap(),
        root_before
    );
    assert_eq!(
        fs::read_to_string(fixture.workflow_manifest("lightflow-std", "example")).unwrap(),
        workflow_before
    );
}

#[test]
fn apply_updates_package_and_release_train_dependencies_only() {
    let fixture = Fixture::new("apply");
    fixture.write_release_train(true);

    let report = release_projects(
        &ApiService::new(&fixture.path),
        &strings(["2.1.0", "--apply"]),
    )
    .expect("apply release");

    assert_eq!(report["dry_run"], false);
    assert_eq!(report["publish"], false);
    assert_eq!(report["executed"], serde_json::json!([]));
    let root = read_manifest(&fixture.path.join("Cargo.toml")).unwrap();
    assert_eq!(root["package"]["version"].as_str(), Some("2.1.0"));
    assert_eq!(root["dependencies"]["serde"]["version"].as_str(), Some("1"));
    let workspace = read_manifest(&fixture.path.join("projects/lightflow-std/Cargo.toml")).unwrap();
    assert_eq!(
        workspace["workspace"]["dependencies"]["lightflow"].as_str(),
        Some("2.1.0")
    );
    assert_eq!(
        workspace["workspace"]["dependencies"]["lightflow-support"]["version"].as_str(),
        Some("2.1.0")
    );
    let workflow = read_manifest(&fixture.workflow_manifest("lightflow-std", "example")).unwrap();
    let support = read_manifest(
        &fixture
            .path
            .join("projects/lightflow-std/runtime/Cargo.toml"),
    )
    .unwrap();
    assert_eq!(support["package"]["version"].as_str(), Some("2.1.0"));
    assert_eq!(workflow["package"]["version"].as_str(), Some("2.1.0"));
    assert_eq!(
        workflow["dev-dependencies"]["lightflow"]["version"].as_str(),
        Some("2.1.0")
    );
    assert_eq!(
        workflow["build-dependencies"]["release-core"]["version"].as_str(),
        Some("2.1.0")
    );
    assert_eq!(
        workflow["target"]["cfg(unix)"]["dependencies"]["lightflow"]["version"].as_str(),
        Some("2.1.0")
    );
    assert_eq!(
        workflow["package"]["metadata"]["tool"]["dependencies"]["lightflow"]["version"].as_str(),
        Some("0.1.0")
    );
    assert_eq!(
        workflow["dependencies"]["serde"]["version"].as_str(),
        Some("1")
    );
}

#[test]
fn missing_optional_workspace_is_reported_and_skipped() {
    let fixture = Fixture::new("optional");
    fixture.write_release_train(false);
    let report =
        release_projects(&ApiService::new(&fixture.path), &strings(["1.5.0"])).expect("plan");
    assert_eq!(
        report["selected_projects"],
        serde_json::json!(["lightflow-std"])
    );
    assert_eq!(
        report["skipped_projects"][0]["name"],
        serde_json::json!("lightflow-extra")
    );
}

#[test]
fn blocked_child_stops_release_before_manifest_writes_or_publish_commands() {
    let fixture = Fixture::new("blocked-child");
    fixture.write_release_train(false);
    fixture.add_blocked_workflow("lightflow-std");
    let root_manifest = fixture.path.join("Cargo.toml");
    let root_before = fs::read_to_string(&root_manifest).unwrap();

    let error = release_projects(
        &ApiService::new(&fixture.path),
        &strings(["2.0.0", "--apply", "--publish"]),
    )
    .expect_err("blocked child must stop the release");
    let message = error.to_string();

    assert!(message.contains("\"blocked_count\":1"), "{message}");
    assert!(message.contains("\"executed\":[]"), "{message}");
    assert_eq!(fs::read_to_string(root_manifest).unwrap(), root_before);
}

#[test]
fn publish_plan_interleaves_preflight_and_upload_in_dependency_order() {
    let dry_runs = vec![
        publish_command("lightflow", true),
        publish_command("lightflow-b", true),
        publish_command("lightflow-a", true),
    ];
    let steps = interleaved_publish_steps(&dry_runs);
    let commands = steps
        .iter()
        .map(|step| step.command.clone())
        .collect::<Vec<_>>();

    assert_eq!(
        commands,
        vec![
            publish_command("lightflow", true),
            publish_command("lightflow", false),
            publish_command("lightflow-b", true),
            publish_command("lightflow-b", false),
            publish_command("lightflow-a", true),
            publish_command("lightflow-a", false),
        ]
    );
    assert_eq!(steps[0].phase, PublishPhase::Preflight);
    assert_eq!(steps[1].phase, PublishPhase::Upload);
}

#[test]
fn generated_plan_publishes_internal_dependency_before_dependent() {
    let fixture = Fixture::new("dependency-order");
    fixture.write_release_train(false);
    fixture.add_base_workflow_dependency("lightflow-std");

    let report = release_projects(
        &ApiService::new(&fixture.path),
        &strings(["3.0.0", "--publish"]),
    )
    .expect("release plan");
    let commands = report["publish_commands"].as_array().unwrap();
    let root_preflight = command_position(commands, "/Cargo.toml", true);
    let root_upload = root_preflight + 1;
    let support_preflight = command_position(commands, "/runtime/Cargo.toml", true);
    let support_upload = command_position(commands, "/runtime/Cargo.toml", false);
    let base_preflight = command_position(commands, "/workflows/base/Cargo.toml", true);
    let base_upload = command_position(commands, "/workflows/base/Cargo.toml", false);
    let example_preflight = command_position(commands, "/workflows/example/Cargo.toml", true);
    let example_upload = command_position(commands, "/workflows/example/Cargo.toml", false);

    assert!(
        !commands[root_upload]
            .as_array()
            .unwrap()
            .iter()
            .any(|argument| argument == "--dry-run")
    );
    assert!(root_preflight < root_upload);
    assert!(root_upload < support_preflight);
    assert!(support_preflight < support_upload);
    assert!(support_upload < base_preflight);
    assert!(base_preflight < base_upload);
    assert!(base_upload < example_preflight);
    assert!(example_preflight < example_upload);
    assert_eq!(report["executed"], serde_json::json!([]));
}

fn command_position(commands: &[serde_json::Value], suffix: &str, dry_run: bool) -> usize {
    commands
        .iter()
        .position(|command| {
            let command = command.as_array().unwrap();
            let manifest_matches = command.iter().any(|argument| {
                argument
                    .as_str()
                    .is_some_and(|argument| argument.ends_with(suffix))
            });
            let is_dry_run = command.iter().any(|argument| argument == "--dry-run");
            manifest_matches && is_dry_run == dry_run
        })
        .unwrap_or_else(|| panic!("missing command for {suffix}, dry_run={dry_run}"))
}

fn publish_command(package: &str, dry_run: bool) -> Vec<String> {
    let mut command = vec![
        "cargo".to_owned(),
        "publish".to_owned(),
        "--manifest-path".to_owned(),
        format!("{package}/Cargo.toml"),
    ];
    if dry_run {
        command.push("--dry-run".to_owned());
    }
    command
}

fn strings<const N: usize>(values: [&str; N]) -> Vec<String> {
    values.into_iter().map(ToOwned::to_owned).collect()
}

pub(super) struct Fixture {
    pub(super) path: PathBuf,
}

impl Fixture {
    pub(super) fn new(name: &str) -> Self {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!(
            "lightflow-release-projects-{name}-{}-{nanos}",
            std::process::id()
        ));
        fs::create_dir_all(&path).unwrap();
        Self { path }
    }

    pub(super) fn write_release_train(&self, create_optional: bool) {
        fs::write(
            self.path.join("Cargo.toml"),
            r#"[package]
name = "lightflow"
version = "0.1.0"
edition = "2024"
description = "Test release root."
license = "MIT"

[workspace]
members = ["."]

[dependencies]
serde = { version = "1" }
"#,
        )
        .unwrap();
        fs::create_dir_all(self.path.join("projects")).unwrap();
        fs::write(
            self.path.join("projects/lightflow-projects.toml"),
            r#"[workspaces]
expected = ["lightflow-std"]
optional = ["lightflow-extra"]

[workflows]
default_sources = []
"#,
        )
        .unwrap();
        self.write_project("lightflow-std");
        if create_optional {
            self.write_project("lightflow-extra");
        }
    }

    fn write_project(&self, project: &str) {
        let root = self.path.join("projects").join(project);
        let workflow = root.join("workflows/example");
        let runtime = root.join("runtime");
        fs::create_dir_all(workflow.join("src")).unwrap();
        fs::create_dir_all(workflow.join(".agent/skills/example")).unwrap();
        fs::create_dir_all(runtime.join("src")).unwrap();
        fs::write(
            root.join("Cargo.toml"),
            r#"[workspace]
members = ["runtime", "workflows/*"]

[workspace.dependencies]
lightflow = "0.1.0"
lightflow-support = { path = "runtime", version = "0.1.0" }
"#,
        )
        .unwrap();
        fs::write(
            runtime.join("Cargo.toml"),
            r#"[package]
name = "lightflow-support"
version = "0.1.0"
edition = "2024"
description = "Test support crate."
license = "MIT"

[dependencies]
lightflow = { workspace = true }
"#,
        )
        .unwrap();
        fs::write(runtime.join("src/lib.rs"), "pub fn support() {}\n").unwrap();
        fs::write(
            workflow.join("Cargo.toml"),
            r#"[package]
name = "lightflow-example"
version = "0.1.0"
edition = "2024"
description = "Test workflow."
license = "MIT"

[dependencies]
lightflow = { workspace = true }
lightflow-support = { workspace = true }
serde = { version = "1" }

[dev-dependencies]
lightflow = { version = "0.1.0" }

[build-dependencies]
release-core = { package = "lightflow", version = "0.1.0" }

[target.'cfg(unix)'.dependencies]
lightflow = { version = "0.1.0" }

[package.metadata.tool.dependencies]
lightflow = { version = "0.1.0" }
"#,
        )
        .unwrap();
        fs::write(
            workflow.join("src/lib.rs"),
            r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow! {
        name: "Example",
        description: "Test workflow.",
        input "value": "json" {
            description: "Input value.",
            required: true,
        }
        output "value": "json" {
            description: "Output value.",
        }
    }

    .build()
}
"#,
        )
        .unwrap();
        fs::write(
            workflow.join(".agent/skills/example/SKILL.md"),
            r#"---
name: example
description: Use the example workflow.
version: 1
---

Run `lfw run lightflow.example --input value=null`.
"#,
        )
        .unwrap();
    }

    fn add_base_workflow_dependency(&self, project: &str) {
        let project_root = self.path.join("projects").join(project);
        let base = project_root.join("workflows/base");
        fs::create_dir_all(base.join("src")).unwrap();
        fs::create_dir_all(base.join(".agent/skills/base")).unwrap();
        fs::write(
            base.join("Cargo.toml"),
            r#"[package]
name = "lightflow-base"
version = "0.1.0"
edition = "2024"
description = "Base test workflow."
license = "MIT"

[dependencies]
lightflow = { workspace = true }
"#,
        )
        .unwrap();
        fs::write(
            base.join("src/lib.rs"),
            r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow! {
        name: "Base",
        description: "Base workflow.",
        input "value": "json" { description: "Input value.", required: true }
        output "value": "json" { description: "Output value." }
    }

    .build()
}
"#,
        )
        .unwrap();
        fs::write(
            base.join(".agent/skills/base/SKILL.md"),
            "---\nname: base\ndescription: Use base.\nversion: 1\n---\n\nRun base.\n",
        )
        .unwrap();
        let example_manifest = self.workflow_manifest(project, "example");
        let source = fs::read_to_string(&example_manifest).unwrap().replace(
            "[dependencies]\nlightflow = { workspace = true }",
            "[dependencies]\nlightflow = { workspace = true }\nlightflow-base = { path = \"../base\", version = \"0.1.0\" }",
        );
        fs::write(example_manifest, source).unwrap();
    }

    fn add_blocked_workflow(&self, project: &str) {
        let root = self.path.join("projects").join(project);
        let workflow = root.join("workflows/blocked");
        fs::create_dir_all(workflow.join("src")).unwrap();
        fs::create_dir_all(workflow.join(".agent/skills/blocked")).unwrap();
        fs::write(
            workflow.join("Cargo.toml"),
            r#"[package]
name = "lightflow-blocked"
version = "0.1.0"
edition = "2024"
description = "Blocked test workflow."
license = "MIT"
publish = false

[dependencies]
lightflow = { workspace = true }
"#,
        )
        .unwrap();
        fs::write(
            workflow.join("src/lib.rs"),
            r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow! {
        name: "Blocked",
        description: "Blocked workflow.",
        input "value": "json" { description: "Input value.", required: true }
        output "value": "json" { description: "Output value." }
    }
    .build()
}
"#,
        )
        .unwrap();
        fs::write(
            workflow.join(".agent/skills/blocked/SKILL.md"),
            "---\nname: blocked\ndescription: Use blocked.\nversion: 1\n---\n\nRun blocked.\n",
        )
        .unwrap();
    }

    fn workflow_manifest(&self, project: &str, workflow: &str) -> PathBuf {
        self.path
            .join("projects")
            .join(project)
            .join("workflows")
            .join(workflow)
            .join("Cargo.toml")
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}
