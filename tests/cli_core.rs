mod support;

use lightflow::api::{ApiService, CheckProfile, ReleaseCheckOptions};
use serde_json::Value;
use std::fs;
use std::path::Path;
use std::process::Command;
use support::*;

#[test]
fn cli_reads_rust_workflows_and_resolves_dependencies() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    write_project_specs(&root)?;

    let list = lightflow(&root, ["workflows", "list"])?;
    let ids = list["workflows"]
        .as_array()
        .expect("workflows list returns an array")
        .iter()
        .map(|workflow| workflow["id"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(
        ids,
        vec!["lightflow.child", "lightflow.parent", "lightflow.sink"]
    );

    let child = lightflow(&root, ["workflows", "get", "lightflow.child"])?;
    assert_eq!(child["id"], Value::String("lightflow.child".to_owned()));
    assert_eq!(child["version"], Value::String("0.1.0".to_owned()));

    let deps = lfw(&root, ["deps", "lightflow.parent"])?;
    assert_eq!(
        deps["workflow_id"],
        Value::String("lightflow.parent".to_owned())
    );
    assert_eq!(deps["complete"], Value::Bool(true));
    assert_eq!(
        deps["workflows"],
        serde_json::json!(["lightflow.child", "lightflow.parent", "lightflow.sink"])
    );
    assert_eq!(
        deps["workflow_order"],
        serde_json::json!(["lightflow.child", "lightflow.sink", "lightflow.parent"])
    );

    let plan = lfw(&root, ["plan", "lightflow.parent"])?;
    assert_eq!(plan["workflow_id"], "lightflow.parent");
    assert_eq!(plan["kind"], "composite");
    assert_eq!(plan["nodes"][0]["node_id"], "nested");
    assert_eq!(plan["nodes"][0]["selected_workflow_id"], "lightflow.child");
    assert_eq!(plan["nodes"][0]["runtime"]["executor_id"], "passthrough");
    assert_eq!(plan["nodes"][0]["runtime"]["data_policy"], "json_values");

    let namespaced_plan = lfw(&root, ["workflows", "plan", "lightflow.child"])?;
    assert_eq!(namespaced_plan["kind"], "leaf");
    assert_eq!(namespaced_plan["runtime"]["executor_id"], "passthrough");

    let brief = lfw(&root, ["list"])?;
    assert_eq!(brief["workflows"][0]["id"], "lightflow.child");
    assert_eq!(brief["workflows"][0]["category"], "tests");
    assert!(brief["workflows"][0].get("nodes").is_none());
    assert!(brief["workflows"][0].get("inputs").is_none());
    assert!(brief["workflows"][0].get("description").is_none());

    let categories = lfw(&root, ["list", "--categories"])?;
    assert_eq!(
        categories["categories"],
        serde_json::json!([{ "category": "tests", "workflows": 3 }])
    );
    let filtered = lfw(&root, ["list", "--category", "tests"])?;
    assert_eq!(filtered["workflows"].as_array().unwrap().len(), 3);

    let detail = lfw(&root, ["ls", "--detail"])?;
    assert_eq!(detail["workflows"][1]["id"], "lightflow.parent");
    assert_eq!(detail["workflows"][1]["category"], "tests");
    assert_eq!(detail["workflows"][1]["nodes"][0]["id"], "nested");
    assert_eq!(detail["workflows"][1]["edges"][0]["from"]["node"], "nested");

    let info = lfw(&root, ["info"])?;
    assert_eq!(info["package"]["name"], "lightflow");
    assert_eq!(info["workflows"]["total"], 3);
    assert_eq!(info["workflows"]["leaf"], 2);
    assert_eq!(info["workflows"]["composite"], 1);
    assert_eq!(
        info["workflows"]["categories"],
        serde_json::json!([{ "category": "tests", "workflows": 3 }])
    );
    assert!(
        info["executors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|executor| {
                executor["id"] == "passthrough"
                    && executor["status"] == "builtin"
                    && executor["available"] == true
                    && executor["data_policy"] == "json_values"
                    && executor["plans_models"] == false
                    && executor["status_reason"] == "available in this build"
            })
    );
    assert!(
        info["executors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|executor| {
                executor["id"] == "lightflow.command.executor.v1"
                    && executor["kind"] == "reserved"
                    && executor["status"] == "reserved"
                    && executor["available"] == false
                    && executor["status_reason"]
                        == "reserved executor contract; not runnable in this build"
            })
    );
    assert!(
        info["executors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|executor| {
                executor["id"] == "diffusion-rs.native.v1"
                    && executor["status"] == "native"
                    && executor["data_policy"] == "device_resident_preferred"
                    && executor["plans_models"] == true
            })
    );
    assert!(
        info["executors"]
            .as_array()
            .unwrap()
            .iter()
            .any(|executor| {
                executor["id"] == "lightflow.python.node.executor.v1"
                    && executor["capabilities"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|capability| capability == "lightflow.python.node")
            })
    );
    let arch = lfw(&root, ["arch"])?;
    assert_eq!(arch["workflows"]["total"], 3);

    let workflow_help = lfw(&root, ["help", "lightflow.parent"])?;
    assert_eq!(workflow_help["workflow"]["id"], "lightflow.parent");
    assert_eq!(workflow_help["workflow"]["kind"], "composite");
    assert_eq!(
        workflow_help["ports"]["inputs"],
        serde_json::json!([
            {
                "name": "in",
                "type": "json",
                "cli_flag": "-i in={}",
                "value_hint": "{}"
            }
        ])
    );
    assert_eq!(
        workflow_help["ports"]["outputs"][0],
        serde_json::json!({
            "name": "out",
            "type": "json",
            "value_hint": "{}"
        })
    );
    assert_eq!(workflow_help["dependencies"]["complete"], true);
    assert_eq!(
        workflow_help["usage"]["command"],
        serde_json::json!(["lfw", "run", "lightflow.parent", "-i in={}"])
    );
    assert_eq!(
        workflow_help["usage"]["inputs_json_shape"],
        serde_json::json!({ "in": {} })
    );

    let workflows_help = lfw(&root, ["workflows", "help", "lightflow.child"])?;
    assert_eq!(workflows_help["workflow"]["id"], "lightflow.child");
    assert_eq!(workflows_help["workflow"]["kind"], "leaf");

    let validation = lightflow(
        &root,
        [
            "workflows",
            "validate",
            r#"{
              "id": "lightflow.invalid",
              "version": "0.1.0",
              "name": "Invalid",
              "nodes": [{ "id": "missing", "workflow_id": "lightflow.missing" }],
              "edges": []
            }"#,
        ],
    )?;
    assert_eq!(validation["valid"], Value::Bool(false));
    assert!(
        validation["issues"][0]
            .as_str()
            .unwrap()
            .contains("missing workflow")
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_release_check_reports_release_gates_without_running_them()
-> Result<(), Box<dyn std::error::Error>> {
    let report = lfw(Path::new(env!("CARGO_MANIFEST_DIR")), ["release", "check"])?;
    assert_eq!(report["dry_run"], true);
    assert_eq!(report["valid"], true);
    assert_eq!(report["issues"], serde_json::json!([]));
    assert_eq!(report["workflow_id"], "lightflow.text_plan");
    assert_eq!(
        report["project_root"],
        Path::new(env!("CARGO_MANIFEST_DIR")).display().to_string()
    );

    let checks = report["checks"].as_array().expect("release checks");
    let passed = checks
        .iter()
        .filter(|check| check["status"] == "passed")
        .count();
    let warning_count = checks
        .iter()
        .filter(|check| check["status"] == "warning")
        .count();
    let failed = checks
        .iter()
        .filter(|check| check["status"] == "failed")
        .count();
    let planned = checks
        .iter()
        .filter(|check| check["status"] == "planned")
        .count();
    let skipped = checks
        .iter()
        .filter(|check| check["status"] == "skipped")
        .count();
    assert_eq!(report["passed"], passed);
    assert_eq!(report["warning_count"], warning_count);
    assert_eq!(report["failed"], failed);
    assert_eq!(report["planned"], planned);
    assert_eq!(report["skipped"], skipped);
    assert!(
        checks
            .iter()
            .any(|check| check["id"] == "release.artifact.changelog"
                && check["status"] == "passed"
                && check["path"] == "CHANGELOG.md")
    );
    assert!(checks.iter().any(
        |check| check["id"] == "release.artifact.v0_2_checklist" && check["status"] == "passed"
    ));
    assert!(checks.iter().any(
        |check| check["id"] == "release.artifact.runtime_verification"
            && check["status"] == "passed"
    ));
    assert!(checks.iter().any(
        |check| check["id"] == "release.artifact.local_workflow_loop"
            && check["status"] == "passed"
    ));
    for id in [
        "release.document.changelog_cli",
        "release.document.changelog_api",
        "release.document.changelog_workflows",
        "release.document.changelog_runtime",
        "release.document.changelog_known_limitations",
        "release.document.changelog_migration_notes",
        "release.document.local_workflow_loop",
    ] {
        assert!(
            checks
                .iter()
                .any(|check| check["id"] == id && check["status"] == "passed"),
            "missing passed release document check {id}"
        );
    }
    assert!(checks.iter().any(|check| {
        check["id"] == "release.review.workflow_change_skills"
            && check["kind"] == "review"
            && (check["status"] == "passed" || check["status"] == "warning")
    }));
    assert!(checks.iter().any(|check| {
        check["id"] == "release.review.local_workflow_loop"
            && check["kind"] == "review"
            && (check["status"] == "passed" || check["status"] == "warning")
            && check["count"].as_u64().is_some()
    }));
    assert!(checks.iter().any(|check| {
        check["id"] == "release.review.local_workflow_loop"
            && check["details"].as_array().is_some_and(|details| {
                details.iter().any(|detail| {
                    detail
                        .as_str()
                        .unwrap_or_default()
                        .contains("loop.models.readiness")
                })
            })
    }));
    assert!(checks.iter().any(|check| {
        check["id"] == "release.review.selected_workflow_loop"
            && check["kind"] == "review"
            && check["status"] == "passed"
            && check["message"]
                .as_str()
                .is_some_and(|message| message.contains("lightflow.text_plan"))
    }));
    assert!(checks.iter().any(|check| {
        check["id"] == "release.review.project_workspaces"
            && check["kind"] == "review"
            && (check["status"] == "passed" || check["status"] == "warning")
            && check["path"] == "projects"
    }));
    assert!(checks.iter().any(|check| {
        check["id"] == "release.review.workflow_publish_ready"
            && check["kind"] == "review"
            && check["status"] == "passed"
            && check["count"].as_u64().is_some_and(|count| count > 0)
            && check["message"]
                .as_str()
                .is_some_and(|message| message.contains("workflow crate"))
    }));

    let planned_commands = checks
        .iter()
        .filter(|check| check["kind"] == "command")
        .map(|check| check["command"].clone())
        .collect::<Vec<_>>();
    assert_eq!(
        planned_commands,
        vec![
            serde_json::json!(["cargo", "fmt", "--check"]),
            serde_json::json!(["cargo", "run", "--bin", "lfw", "--", "loop", "check"]),
            serde_json::json!([
                "cargo",
                "run",
                "--bin",
                "lfw",
                "--",
                "loop",
                "check",
                "lightflow.text_plan",
                "--require-replay"
            ]),
            serde_json::json!(["cargo", "run", "--bin", "lfw", "--", "loop", "changes"]),
            serde_json::json!(["cargo", "run", "--bin", "lfw", "--", "loop", "projects"]),
            serde_json::json!([
                "cargo", "run", "--bin", "lfw", "--", "loop", "projects", "--dirty"
            ]),
            serde_json::json!([
                "cargo",
                "run",
                "--bin",
                "lfw",
                "--",
                "publish",
                "--workflows",
                "--require-publishable"
            ]),
            serde_json::json!(["cargo", "clippy", "--all-targets", "--", "-D", "warnings"]),
            serde_json::json!(["cargo", "test"]),
            serde_json::json!([
                "cargo",
                "test",
                "--test",
                "standard_nodes",
                "repository_workflow_crates_have_agent_skills"
            ]),
            serde_json::json!(["cargo", "test", "--features", "rig", "--test", "llm_rig"]),
            serde_json::json!(["cargo", "check", "--features", "flux-native"]),
        ]
    );
    assert!(checks.iter().any(|check| check["status"] == "planned"));
    Ok(())
}

#[test]
fn lfw_dev_check_reports_developer_gate_plan_without_running_commands()
-> Result<(), Box<dyn std::error::Error>> {
    let report = lfw(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        ["dev", "check", "--workflow", "lightflow.text_to_image"],
    )?;
    assert_eq!(report["profile"], "development");
    assert_eq!(report["dry_run"], true);
    assert_eq!(report["workflow_id"], "lightflow.text_to_image");

    let checks = report["checks"].as_array().expect("dev checks");
    assert!(checks.iter().all(|check| check["kind"] != "artifact"));
    assert!(checks.iter().all(|check| check["kind"] != "document"));
    let local_loop_review = checks
        .iter()
        .find(|check| check["id"] == "release.review.local_workflow_loop")
        .expect("local loop review");
    let local_loop_details = local_loop_review["details"]
        .as_array()
        .expect("local loop review details");
    if local_loop_review["status"] == "failed" {
        assert!(
            local_loop_details.len() > 1,
            "local loop review details:\n{local_loop_details:#?}"
        );
        assert!(
            local_loop_details.iter().any(|detail| detail
                .as_str()
                .unwrap_or_default()
                .contains("HTTP `/workflows/")),
            "local loop review details:\n{local_loop_details:#?}"
        );
    }
    assert!(
        checks
            .iter()
            .any(|check| check["id"] == "release.command.fmt" && check["status"] == "planned")
    );
    assert!(
        checks
            .iter()
            .any(|check| check["id"] == "release.command.clippy" && check["status"] == "planned")
    );
    assert!(
        checks
            .iter()
            .any(|check| check["id"] == "release.command.test" && check["status"] == "planned")
    );
    Ok(())
}

#[test]
fn lfw_release_check_dry_run_reports_source_change_blockers()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(root.join("docs"))?;
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\n### CLI\n\n### API\n\n### Workflows\n\n### Runtime\n\n### Known Limitations\n\n### Migration Notes\n",
    )?;
    fs::write(root.join("docs/v0.2-checklist.md"), "# Checklist\n")?;
    fs::write(root.join("docs/runtime-verification.md"), "# Runtime\n")?;
    fs::write(
        root.join("docs/local-workflow-loop.md"),
        "# Local Workflow Loop\n\n## Verification Gates\n",
    )?;
    lfw(&root, ["init"])?;
    lfw(&root, ["new", "reviewed", "--category", "examples"])?;
    complete_generated_workflow_metadata(&root, "examples", "example")?;
    complete_generated_workflow_metadata(&root, "examples", "reviewed")?;
    git_ok(&root, ["init"])?;
    git_ok(&root, ["add", "."])?;
    git_ok(
        &root,
        [
            "-c",
            "user.name=LightFlow Test",
            "-c",
            "user.email=lightflow@example.test",
            "commit",
            "-m",
            "initial workflow",
        ],
    )?;

    let source_path = root.join(".lightflow/workflows/examples/reviewed/src/lib.rs");
    fs::write(
        &source_path,
        fs::read_to_string(&source_path)? + "\n// release-blocking behavior change\n",
    )?;

    lfw(&root, ["run", "lightflow.example", "-i", "value={}"])?;
    let report = lfw(
        &root,
        ["release", "check", "--workflow", "lightflow.example"],
    )?;
    assert_eq!(report["valid"], false);
    assert!(
        report["issues"][0]
            .as_str()
            .expect("release issue")
            .contains("release.review.workflow_change_skills")
    );
    let checks = report["checks"].as_array().expect("release checks");
    let review_check = checks
        .iter()
        .find(|check| check["id"] == "release.review.workflow_change_skills")
        .expect("source change review check");
    assert_eq!(review_check["kind"], "review");
    assert_eq!(review_check["status"], "failed");
    assert!(
        review_check["message"]
            .as_str()
            .expect("review message")
            .contains("workflow source changes need colocated agent skill updates"),
        "review check:\n{review_check:#?}"
    );
    assert!(
        review_check["message"]
            .as_str()
            .expect("review message")
            .contains(
                "examples/reviewed: workflow files changed without a colocated agent skill update"
            ),
        "review check:\n{review_check:#?}"
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_release_check_dry_run_accepts_skill_only_source_changes()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(root.join("docs"))?;
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\n### CLI\n\n### API\n\n### Workflows\n\n### Runtime\n\n### Known Limitations\n\n### Migration Notes\n",
    )?;
    fs::write(root.join("docs/v0.2-checklist.md"), "# Checklist\n")?;
    fs::write(root.join("docs/runtime-verification.md"), "# Runtime\n")?;
    fs::write(
        root.join("docs/local-workflow-loop.md"),
        "# Local Workflow Loop\n\n## Verification Gates\n",
    )?;
    lfw(&root, ["init"])?;
    lfw(&root, ["new", "reviewed", "--category", "examples"])?;
    complete_generated_workflow_metadata(&root, "examples", "example")?;
    complete_generated_workflow_metadata(&root, "examples", "reviewed")?;
    git_ok(&root, ["init"])?;
    git_ok(&root, ["add", "."])?;
    git_ok(
        &root,
        [
            "-c",
            "user.name=LightFlow Test",
            "-c",
            "user.email=lightflow@example.test",
            "commit",
            "-m",
            "initial workflow",
        ],
    )?;

    let skill_path = root
        .join(".lightflow/workflows/examples/reviewed/.agent/skills/lightflow-reviewed/SKILL.md");
    fs::write(
        &skill_path,
        fs::read_to_string(&skill_path)? + "\nReview note: skill docs clarified.\n",
    )?;

    lfw(&root, ["run", "lightflow.example", "-i", "value={}"])?;
    let report = lfw(
        &root,
        ["release", "check", "--workflow", "lightflow.example"],
    )?;
    assert_eq!(report["valid"], true, "release report:\n{report:#?}");
    assert_eq!(
        report["issues"],
        serde_json::json!([]),
        "release report:\n{report:#?}"
    );
    assert!(
        !report["warnings"]
            .as_array()
            .expect("release warnings")
            .iter()
            .any(|warning| {
                warning
                    .as_str()
                    .unwrap_or_default()
                    .contains("release.review.workflow_change_skills")
            }),
        "release report:\n{report:#?}"
    );
    let checks = report["checks"].as_array().expect("release checks");
    let review_check = checks
        .iter()
        .find(|check| check["id"] == "release.review.workflow_change_skills")
        .expect("source change review check");
    assert_eq!(review_check["kind"], "review");
    assert_eq!(review_check["status"], "passed");
    assert_eq!(review_check["count"], 1);
    assert!(
        review_check["message"]
            .as_str()
            .expect("review message")
            .contains("source-change safety passed"),
        "review check:\n{review_check:#?}"
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_release_check_dry_run_reports_local_loop_warnings() -> Result<(), Box<dyn std::error::Error>>
{
    let root = unique_temp_root();
    fs::create_dir_all(root.join("docs"))?;
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\n### CLI\n\n### API\n\n### Workflows\n\n### Runtime\n\n### Known Limitations\n\n### Migration Notes\n",
    )?;
    fs::write(root.join("docs/v0.2-checklist.md"), "# Checklist\n")?;
    fs::write(root.join("docs/runtime-verification.md"), "# Runtime\n")?;
    fs::write(
        root.join("docs/local-workflow-loop.md"),
        "# Local Workflow Loop\n\n## Verification Gates\n",
    )?;
    lfw(&root, ["init"])?;
    complete_generated_workflow_metadata(&root, "examples", "example")?;
    git_ok(&root, ["init"])?;
    git_ok(&root, ["add", "."])?;
    git_ok(
        &root,
        [
            "-c",
            "user.name=LightFlow Test",
            "-c",
            "user.email=lightflow@example.test",
            "commit",
            "-m",
            "initial workflow",
        ],
    )?;

    let run_dir = root.join(".lightflow/runs/run-unknown");
    fs::create_dir_all(&run_dir)?;
    fs::write(root.join(".lightflow/runs/last"), "run-unknown")?;
    fs::write(
        run_dir.join("manifest.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "kind": "workflow_run",
            "run_id": "run-unknown",
            "started_at_ms": 10,
            "completed_at_ms": 12,
            "stages": [{
                "workflow_id": "lightflow.example",
                "execution": {
                    "inputs": {},
                    "disabled_nodes": [],
                    "enabled_nodes": []
                }
            }]
        }))?,
    )?;
    fs::write(
        run_dir.join("events.jsonl"),
        r#"{"event":"run_started","run_id":"run-unknown","surface":"cli"}"#,
    )?;
    let second_run_dir = root.join(".lightflow/runs/run-unknown-2");
    fs::create_dir_all(&second_run_dir)?;
    fs::write(
        second_run_dir.join("manifest.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "kind": "workflow_run",
            "run_id": "run-unknown-2",
            "started_at_ms": 20,
            "completed_at_ms": 22,
            "stages": [{
                "workflow_id": "lightflow.example",
                "execution": {
                    "inputs": {},
                    "disabled_nodes": [],
                    "enabled_nodes": []
                }
            }]
        }))?,
    )?;
    fs::write(
        second_run_dir.join("events.jsonl"),
        r#"{"event":"run_started","run_id":"run-unknown-2","surface":"cli"}"#,
    )?;
    let completed_run_dir = root.join(".lightflow/runs/run-completed");
    fs::create_dir_all(&completed_run_dir)?;
    fs::write(
        completed_run_dir.join("manifest.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "kind": "workflow_run",
            "run_id": "run-completed",
            "status": "completed",
            "started_at_ms": 30,
            "completed_at_ms": 32,
            "stages": [{
                "workflow_id": "lightflow.example",
                "execution": {
                    "inputs": {},
                    "disabled_nodes": [],
                    "enabled_nodes": []
                }
            }]
        }))?,
    )?;
    fs::write(
        completed_run_dir.join("events.jsonl"),
        r#"{"event":"run_finished","run_id":"run-completed","surface":"cli"}"#,
    )?;

    let report = lfw(
        &root,
        ["release", "check", "--workflow", "lightflow.example"],
    )?;
    assert_eq!(report["valid"], true, "release report:\n{report:#?}");
    assert!(
        report["warnings"]
            .as_array()
            .expect("release warnings")
            .iter()
            .any(|warning| warning
                .as_str()
                .unwrap_or_default()
                .contains("loop.history.runs: run history has 2 unknown-status run")),
        "release report:\n{report:#?}"
    );
    let local_loop_review = report["checks"]
        .as_array()
        .expect("release checks")
        .iter()
        .find(|check| check["id"] == "release.review.local_workflow_loop")
        .expect("local workflow loop review");
    assert_eq!(local_loop_review["status"], "warning");
    assert_eq!(local_loop_review["count"], 2);

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn release_check_reviews_configured_workflow_paths() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    let external = unique_temp_root();
    fs::create_dir_all(&root)?;
    fs::create_dir_all(&external)?;
    write_workflow_crate_in(
        &external,
        "lightflow.external_model",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.external_model")
        .version("0.1.0")
        .name("External Model")
        .input("model", "text")
        .output("value", "json")
        .model("external_weights", "text-to-image")
        .input_model_requirement("model", "external_weights")
        .build()
}
"#,
    )?;
    let run_dir = root.join(".lightflow/runs/run-external");
    fs::create_dir_all(&run_dir)?;
    fs::write(root.join(".lightflow/runs/last"), "run-external")?;
    fs::write(
        run_dir.join("manifest.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "kind": "workflow_run",
            "run_id": "run-external",
            "status": "completed",
            "started_at_ms": 10,
            "completed_at_ms": 12,
            "stages": [{
                "workflow_id": "lightflow.external_model",
                "execution": {
                    "inputs": {},
                    "disabled_nodes": [],
                    "enabled_nodes": []
                }
            }]
        }))?,
    )?;
    fs::write(
        run_dir.join("events.jsonl"),
        r#"{"event":"run_finished","run_id":"run-external","surface":"cli"}"#,
    )?;

    let service = ApiService::new(&root).with_workflow_paths(vec![external.clone()]);
    let report = service.release_check(&ReleaseCheckOptions {
        apply: false,
        workflow_id: "lightflow.external_model".to_owned(),
        project: None,
        profile: CheckProfile::Release,
    })?;
    let local_loop_review = report
        .checks
        .iter()
        .find(|check| check.id == "release.review.local_workflow_loop")
        .expect("local loop review");
    assert_eq!(serde_json::to_value(local_loop_review.status)?, "warning");
    assert!(
        local_loop_review
            .message
            .contains("lightflow.external_model::external_weights"),
        "release review message:\n{}",
        local_loop_review.message
    );
    assert!(
        local_loop_review
            .details
            .iter()
            .any(|detail| detail.contains("lightflow.external_model::external_weights")),
        "release review details:\n{:#?}",
        local_loop_review.details
    );
    let selected_review = report
        .checks
        .iter()
        .find(|check| check.id == "release.review.selected_workflow_loop")
        .expect("selected loop review");
    assert_eq!(serde_json::to_value(selected_review.status)?, "warning");
    assert!(
        selected_review.message.contains("loop.selected.models"),
        "selected review message:\n{}",
        selected_review.message
    );
    assert!(
        selected_review
            .details
            .iter()
            .any(|detail| detail.contains("loop.selected.models")),
        "selected review details:\n{:#?}",
        selected_review.details
    );

    let _ = fs::remove_dir_all(root);
    let _ = fs::remove_dir_all(external);
    Ok(())
}

#[test]
fn lfw_release_check_apply_skips_commands_after_source_change_blockers()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(root.join("docs"))?;
    fs::write(
        root.join("CHANGELOG.md"),
        "# Changelog\n\n### CLI\n\n### API\n\n### Workflows\n\n### Runtime\n\n### Known Limitations\n\n### Migration Notes\n",
    )?;
    fs::write(root.join("docs/v0.2-checklist.md"), "# Checklist\n")?;
    fs::write(root.join("docs/runtime-verification.md"), "# Runtime\n")?;
    fs::write(
        root.join("docs/local-workflow-loop.md"),
        "# Local Workflow Loop\n\n## Verification Gates\n",
    )?;
    lfw(&root, ["init"])?;
    lfw(&root, ["new", "reviewed", "--category", "examples"])?;
    git_ok(&root, ["init"])?;
    git_ok(&root, ["add", "."])?;
    git_ok(
        &root,
        [
            "-c",
            "user.name=LightFlow Test",
            "-c",
            "user.email=lightflow@example.test",
            "commit",
            "-m",
            "initial workflow",
        ],
    )?;

    let source_path = root.join(".lightflow/workflows/examples/reviewed/src/lib.rs");
    fs::write(
        &source_path,
        fs::read_to_string(&source_path)? + "\n// apply-blocking behavior change\n",
    )?;

    let output = lfw_command(&root)
        .args(["release", "check", "--apply"])
        .output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("\"id\":\"release.review.workflow_change_skills\""),
        "stderr:\n{stderr}"
    );
    assert!(stderr.contains("\"kind\":\"review\""), "stderr:\n{stderr}");
    assert!(
        stderr.contains("\"status\":\"failed\""),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("\"id\":\"release.command.fmt\""),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("\"status\":\"skipped\""),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("command skipped because an earlier release gate failed"),
        "stderr:\n{stderr}"
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

fn git_ok<const N: usize>(root: &Path, args: [&str; N]) -> Result<(), Box<dyn std::error::Error>> {
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

#[test]
fn lfw_help_advertises_project_scoped_developer_release_and_publish_selectors()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    let output = lfw_command(&root).arg("--help").output()?;
    let output_text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        output_text
            .contains("lfw release check [--apply] [--workflow <workflow_id>] [--project <name>]"),
        "output:\n{output_text}"
    );
    assert!(
        output_text
            .contains("lfw dev check [--apply] [--workflow <workflow_id>] [--project <name>]"),
        "output:\n{output_text}"
    );
    assert!(
        output_text
            .contains("lfw publish [workflow_id|--crate <path>|--workflows] [--project <name>]"),
        "output:\n{output_text}"
    );
    assert!(
        output_text.contains("lfw dev skill-template <workflow_id> [--write] [--force]"),
        "output:\n{output_text}"
    );
    assert!(
        output_text.contains("lfw dev project-config-template [--write] [--force]"),
        "output:\n{output_text}"
    );
    let dev_help = lfw_command(&root).args(["dev", "--help"]).output()?;
    let dev_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&dev_help.stdout),
        String::from_utf8_lossy(&dev_help.stderr)
    );
    assert!(
        dev_help_text.contains("lightflow-* short aliases such as std, flux, rig, or custom-tools"),
        "output:\n{dev_help_text}"
    );
    assert!(
        dev_help_text.contains("project_submodule_update_command"),
        "output:\n{dev_help_text}"
    );
    for args in [
        vec!["dev", "check", "--workflow"],
        vec!["dev", "check", "--workflow", "--apply"],
        vec!["dev", "check", "--project"],
        vec!["dev", "check", "--bad"],
        vec!["dev", "skill-template", "--bad"],
        vec!["dev", "skill-template", "lightflow.text_plan", "extra"],
        vec!["dev", "project-config-template", "--bad"],
    ] {
        let output = lfw_command(&root).args(args).output()?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            text.contains("lfw dev check [--apply] [--workflow <workflow_id>] [--project <name>]")
                && text.contains("lfw dev skill-template <workflow_id> [--write] [--force]")
                && text.contains("lfw dev project-config-template [--write] [--force]")
                && text.contains("The selected workflow gate defaults to lightflow.text_plan")
                && !text.contains("requires a workflow id")
                && !text.contains("requires a project workspace name")
                && !text.trim().starts_with("unexpected argument"),
            "output:\n{text}"
        );
    }
    let release_help = lfw_command(&root).args(["release", "--help"]).output()?;
    let release_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&release_help.stdout),
        String::from_utf8_lossy(&release_help.stderr)
    );
    assert!(
        release_help_text
            .contains("lightflow-* short aliases such as std, flux, rig, or custom-tools"),
        "output:\n{release_help_text}"
    );
    for args in [
        vec!["release", "check", "--workflow"],
        vec!["release", "check", "--workflow", "--apply"],
        vec!["release", "check", "--project"],
        vec!["release", "check", "--bad"],
    ] {
        let output = lfw_command(&root).args(args).output()?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            text.contains(
                "lfw release check [--apply] [--workflow <workflow_id>] [--project <name>]"
            ) && text.contains("The selected workflow gate defaults to lightflow.text_plan")
                && !text.contains("requires a workflow id")
                && !text.contains("requires a project workspace name")
                && !text.trim().starts_with("unexpected argument"),
            "output:\n{text}"
        );
    }
    let publish_help = lfw_command(&root).args(["publish", "--help"]).output()?;
    let publish_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&publish_help.stdout),
        String::from_utf8_lossy(&publish_help.stderr)
    );
    assert!(
        publish_help_text
            .contains("lfw publish [workflow_id|--crate <path>|--workflows] [--project <name>]")
            && publish_help_text.contains("--workflows checks workflow crates in dependency order")
            && publish_help_text
                .contains("--project filters --workflows to one linked project workspace"),
        "output:\n{publish_help_text}"
    );
    for args in [
        vec!["publish", "--project"],
        vec!["publish", "--project", "--workflows"],
        vec!["publish", "--project", "std"],
        vec!["publish", "--crate"],
        vec!["publish", "--bad"],
    ] {
        let output = lfw_command(&root).args(args).output()?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            text.contains(
                "lfw publish [workflow_id|--crate <path>|--workflows] [--project <name>]"
            ) && text.contains("--workflows checks workflow crates in dependency order")
                && !text.contains("missing value for")
                && !text.trim().starts_with("unexpected argument"),
            "output:\n{text}"
        );
    }
    let loop_help = lfw_command(&root).args(["loop", "--help"]).output()?;
    let loop_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&loop_help.stdout),
        String::from_utf8_lossy(&loop_help.stderr)
    );
    assert!(
        loop_help_text.contains("lfw loop projects [--dirty] [--project <name>]")
            && loop_help_text
                .contains("lightflow-* short aliases such as std, flux, rig, or custom-tools"),
        "output:\n{loop_help_text}"
    );
    let loop_projects_help = lfw_command(&root)
        .args(["loop", "projects", "--help"])
        .output()?;
    let loop_projects_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&loop_projects_help.stdout),
        String::from_utf8_lossy(&loop_projects_help.stderr)
    );
    assert!(
        loop_projects_help_text.contains("lfw loop projects [--dirty] [--project <name>]")
            && loop_projects_help_text.contains("--dirty narrows project workspace output"),
        "output:\n{loop_projects_help_text}"
    );
    for args in [
        vec!["loop", "projects", "--project"],
        vec!["loop", "projects", "--project", "--dirty"],
        vec!["loop", "projects", "--bad"],
    ] {
        let output = lfw_command(&root).args(args).output()?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            text.contains("lfw loop projects [--dirty] [--project <name>]")
                && text.contains("--dirty narrows project workspace output")
                && !text.contains("missing value for")
                && !text.trim().starts_with("unexpected argument"),
            "output:\n{text}"
        );
    }
    let loop_require_replay_missing = lfw_command(&root)
        .args(["loop", "check", "--require-replay"])
        .output()?;
    let loop_require_replay_missing_text = format!(
        "{}{}",
        String::from_utf8_lossy(&loop_require_replay_missing.stdout),
        String::from_utf8_lossy(&loop_require_replay_missing.stderr)
    );
    assert!(
        loop_require_replay_missing_text
            .contains("lfw loop check [workflow_id] [--require-replay]")
            && loop_require_replay_missing_text.contains(
                "--require-replay requires a selected workflow id and completed-run replay evidence"
            )
            && !loop_require_replay_missing_text.contains("requires a workflow id"),
        "output:\n{loop_require_replay_missing_text}"
    );
    let loop_check_bad = lfw_command(&root)
        .args(["loop", "check", "--bad"])
        .output()?;
    let loop_check_bad_text = format!(
        "{}{}",
        String::from_utf8_lossy(&loop_check_bad.stdout),
        String::from_utf8_lossy(&loop_check_bad.stderr)
    );
    assert!(
        loop_check_bad_text.contains("lfw loop check [workflow_id] [--require-replay]")
            && loop_check_bad_text.contains("lfw loop projects [--dirty] [--project <name>]")
            && !loop_check_bad_text
                .trim()
                .starts_with("unexpected argument"),
        "output:\n{loop_check_bad_text}"
    );
    let loop_changes_help = lfw_command(&root)
        .args(["loop", "changes", "--help"])
        .output()?;
    let loop_changes_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&loop_changes_help.stdout),
        String::from_utf8_lossy(&loop_changes_help.stderr)
    );
    assert!(
        loop_changes_help_text.contains("lfw loop changes")
            && loop_changes_help_text.contains("workflow-loop readiness")
            && loop_changes_help_text.contains("without mutating project files"),
        "output:\n{loop_changes_help_text}"
    );
    let node_help = lfw_command(&root).args(["node", "--help"]).output()?;
    let node_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&node_help.stdout),
        String::from_utf8_lossy(&node_help.stderr)
    );
    assert!(
        node_help_text.contains("lfw node test <workflow_id>")
            && node_help_text.contains("workflow node conformance checks")
            && node_help_text.contains("colocated agent skill coverage"),
        "output:\n{node_help_text}"
    );
    let node_test_help = lfw_command(&root)
        .args(["node", "test", "--help"])
        .output()?;
    let node_test_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&node_test_help.stdout),
        String::from_utf8_lossy(&node_test_help.stderr)
    );
    assert!(
        node_test_help_text.contains("lfw node test <workflow_id>")
            && node_test_help_text.contains("port schema metadata"),
        "output:\n{node_test_help_text}"
    );
    for args in [vec!["node", "test"], vec!["node", "test", "--bad"]] {
        let output = lfw_command(&root).args(args).output()?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            text.contains("lfw node test <workflow_id>")
                && text.contains("workflow node conformance checks")
                && !text.contains("missing workflow id")
                && !text.contains("not found: workflow --bad"),
            "output:\n{text}"
        );
    }
    let workflows_help = lfw_command(&root).args(["workflows", "--help"]).output()?;
    let workflows_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&workflows_help.stdout),
        String::from_utf8_lossy(&workflows_help.stderr)
    );
    assert!(
        workflows_help_text.contains("lfw workflows list")
            && workflows_help_text.contains("lfw workflows plan <workflow_id>")
            && workflows_help_text.contains("lfw workflows validate <json|-|@file>")
            && workflows_help_text.contains("active workflow catalog"),
        "output:\n{workflows_help_text}"
    );
    let workflows_plan_help = lfw_command(&root)
        .args(["workflows", "plan", "--help"])
        .output()?;
    let workflows_plan_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&workflows_plan_help.stdout),
        String::from_utf8_lossy(&workflows_plan_help.stderr)
    );
    assert!(
        workflows_plan_help_text.contains("lfw workflows plan <workflow_id>")
            && workflows_plan_help_text.contains("JSON arguments can be inline JSON"),
        "output:\n{workflows_plan_help_text}"
    );
    for args in [
        ["workflows", "get"],
        ["workflows", "deps"],
        ["workflows", "plan"],
    ] {
        let output = lfw_command(&root).args(args).output()?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            text.contains("lfw workflows get <workflow_id>")
                && text.contains("lfw workflows deps <workflow_id>")
                && text.contains("lfw workflows plan <workflow_id>")
                && !text.contains("missing workflow id"),
            "output:\n{text}"
        );
    }
    let workflows_validate_bad_json = lfw_command(&root)
        .args(["workflows", "validate", "{bad-json"])
        .output()?;
    assert!(!workflows_validate_bad_json.status.success());
    let workflows_validate_bad_json_text = format!(
        "{}{}",
        String::from_utf8_lossy(&workflows_validate_bad_json.stdout),
        String::from_utf8_lossy(&workflows_validate_bad_json.stderr)
    );
    assert!(
        workflows_validate_bad_json_text.contains("invalid workflow JSON for workflows validate")
            && workflows_validate_bad_json_text.contains("lfw workflows validate <json|-|@file>")
            && workflows_validate_bad_json_text.contains("JSON arguments can be inline JSON"),
        "output:\n{workflows_validate_bad_json_text}"
    );
    for args in [
        vec!["workflows", "validate"],
        vec!["workflows", "save"],
        vec!["workflows", "validate", "--bad"],
        vec!["workflows", "save", "--bad"],
        vec!["workflows", "help", "--bad"],
    ] {
        let output = lfw_command(&root).args(args).output()?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            text.contains("lfw workflows validate <json|-|@file>")
                && text.contains("lfw workflows save <json|-|@file>")
                && text.contains("JSON arguments can be inline JSON")
                && !text.contains("missing workflow json")
                && !text.contains("not found: workflow --bad"),
            "output:\n{text}"
        );
    }
    let workflow_shortcut_help = lfw_command(&root).args(["help", "--help"]).output()?;
    let workflow_shortcut_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&workflow_shortcut_help.stdout),
        String::from_utf8_lossy(&workflow_shortcut_help.stderr)
    );
    assert!(
        workflow_shortcut_help_text.contains("lfw help <workflow_id>")
            && workflow_shortcut_help_text.contains("lfw deps <workflow_id>")
            && workflow_shortcut_help_text.contains("lfw plan <workflow_id>")
            && workflow_shortcut_help_text.contains("active workflow catalog")
            && workflow_shortcut_help_text.contains("Equivalent namespaced commands"),
        "output:\n{workflow_shortcut_help_text}"
    );
    for args in [vec!["deps"], vec!["plan"], vec!["help", "--bad"]] {
        let output = lfw_command(&root).args(args).output()?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            text.contains("lfw deps <workflow_id>")
                && text.contains("lfw plan <workflow_id>")
                && text.contains("lfw help <workflow_id>")
                && !text.contains("missing workflow id"),
            "output:\n{text}"
        );
        assert!(
            !text.contains("not found: workflow --bad"),
            "output:\n{text}"
        );
    }
    let workflow_deps_help = lfw_command(&root).args(["deps", "--help"]).output()?;
    let workflow_deps_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&workflow_deps_help.stdout),
        String::from_utf8_lossy(&workflow_deps_help.stderr)
    );
    assert!(
        workflow_deps_help_text.contains("lfw dependencies <workflow_id>")
            && workflow_deps_help_text.contains("version mismatches"),
        "output:\n{workflow_deps_help_text}"
    );
    let workflow_plan_help = lfw_command(&root).args(["plan", "--help"]).output()?;
    let workflow_plan_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&workflow_plan_help.stdout),
        String::from_utf8_lossy(&workflow_plan_help.stderr)
    );
    assert!(
        workflow_plan_help_text.contains("lfw workflows plan <workflow_id>")
            && workflow_plan_help_text.contains("executor/runtime plan"),
        "output:\n{workflow_plan_help_text}"
    );
    let run_help = lfw_command(&root).args(["run", "--help"]).output()?;
    let run_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&run_help.stdout),
        String::from_utf8_lossy(&run_help.stderr)
    );
    assert!(
        run_help_text.contains("lfw run <workflow_id>")
            && run_help_text.contains("records execution under .lightflow/runs/")
            && run_help_text.contains("Use '|' between workflow ids")
            && run_help_text.contains("--patch"),
        "output:\n{run_help_text}"
    );
    let run_missing_workflow_help = lfw_command(&root).arg("run").output()?;
    let run_missing_workflow_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&run_missing_workflow_help.stdout),
        String::from_utf8_lossy(&run_missing_workflow_help.stderr)
    );
    assert!(
        run_missing_workflow_help_text.contains("lfw run <workflow_id>")
            && run_missing_workflow_help_text.contains("records execution under .lightflow/runs/")
            && !run_missing_workflow_help_text.contains("missing workflow id"),
        "output:\n{run_missing_workflow_help_text}"
    );
    for args in [
        vec!["run", "--bad"],
        vec!["run", "lightflow.demo", "--bad"],
        vec!["run", "lightflow.demo", "--input"],
        vec!["run", "lightflow.demo", "--inputs"],
        vec!["run", "lightflow.demo", "--patch", "--enable", "node"],
        vec!["run", "lightflow.demo", "--text", "|", "lightflow.next"],
    ] {
        let output = lfw_command(&root).args(args).output()?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            text.contains("lfw run <workflow_id>")
                && text.contains("Use '|' between workflow ids")
                && text.contains("--patch")
                && !text.contains("missing value for"),
            "output:\n{text}"
        );
        assert!(
            !text.contains("not found: workflow --bad"),
            "output:\n{text}"
        );
        assert!(
            !text.trim().starts_with("unexpected argument"),
            "output:\n{text}"
        );
    }
    let lfx_help = Command::new(env!("CARGO_BIN_EXE_lfx"))
        .arg("--help")
        .current_dir(&root)
        .output()?;
    let lfx_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&lfx_help.stdout),
        String::from_utf8_lossy(&lfx_help.stderr)
    );
    assert!(
        lfx_help_text.contains("lfx <workflow_id>")
            && lfx_help_text.contains("records execution under .lightflow/runs/")
            && lfx_help_text.contains("Use '|' between workflow ids")
            && lfx_help_text.contains("--patch")
            && !lfx_help_text.contains("lfwx <workflow_id>"),
        "output:\n{lfx_help_text}"
    );
    let lfx_missing_workflow_help = Command::new(env!("CARGO_BIN_EXE_lfx"))
        .current_dir(&root)
        .output()?;
    let lfx_missing_workflow_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&lfx_missing_workflow_help.stdout),
        String::from_utf8_lossy(&lfx_missing_workflow_help.stderr)
    );
    assert!(
        lfx_missing_workflow_help_text.contains("lfx <workflow_id>")
            && lfx_missing_workflow_help_text.contains("records execution under .lightflow/runs/")
            && !lfx_missing_workflow_help_text.contains("missing workflow id"),
        "output:\n{lfx_missing_workflow_help_text}"
    );
    let lfx_missing_input_help = Command::new(env!("CARGO_BIN_EXE_lfx"))
        .args(["lightflow.demo", "--input"])
        .current_dir(&root)
        .output()?;
    let lfx_missing_input_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&lfx_missing_input_help.stdout),
        String::from_utf8_lossy(&lfx_missing_input_help.stderr)
    );
    assert!(
        lfx_missing_input_help_text.contains("lfx <workflow_id>")
            && lfx_missing_input_help_text.contains("Use '|' between workflow ids")
            && !lfx_missing_input_help_text.contains("lfw run <workflow_id>")
            && !lfx_missing_input_help_text.contains("missing value for"),
        "output:\n{lfx_missing_input_help_text}"
    );
    let lfx_bad_workflow_help = Command::new(env!("CARGO_BIN_EXE_lfx"))
        .arg("--bad")
        .current_dir(&root)
        .output()?;
    let lfx_bad_workflow_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&lfx_bad_workflow_help.stdout),
        String::from_utf8_lossy(&lfx_bad_workflow_help.stderr)
    );
    assert!(
        lfx_bad_workflow_help_text.contains("lfx <workflow_id>")
            && lfx_bad_workflow_help_text.contains("Use '|' between workflow ids")
            && !lfx_bad_workflow_help_text.contains("not found: workflow --bad"),
        "output:\n{lfx_bad_workflow_help_text}"
    );
    let lfx_unknown_flag_help = Command::new(env!("CARGO_BIN_EXE_lfx"))
        .args(["lightflow.demo", "--bad"])
        .current_dir(&root)
        .output()?;
    let lfx_unknown_flag_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&lfx_unknown_flag_help.stdout),
        String::from_utf8_lossy(&lfx_unknown_flag_help.stderr)
    );
    assert!(
        lfx_unknown_flag_help_text.contains("lfx <workflow_id>")
            && lfx_unknown_flag_help_text.contains("Use '|' between workflow ids")
            && !lfx_unknown_flag_help_text
                .trim()
                .starts_with("unexpected argument"),
        "output:\n{lfx_unknown_flag_help_text}"
    );
    let lfwx_help = Command::new(env!("CARGO_BIN_EXE_lfwx"))
        .arg("--help")
        .current_dir(&root)
        .output()?;
    let lfwx_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&lfwx_help.stdout),
        String::from_utf8_lossy(&lfwx_help.stderr)
    );
    assert!(
        lfwx_help_text.contains("lfwx <workflow_id>")
            && lfwx_help_text.contains("records execution under .lightflow/runs/")
            && lfwx_help_text.contains("Use '|' between workflow ids")
            && lfwx_help_text.contains("--patch")
            && !lfwx_help_text.contains("lfx <workflow_id>"),
        "output:\n{lfwx_help_text}"
    );
    let lfwx_missing_workflow_help = Command::new(env!("CARGO_BIN_EXE_lfwx"))
        .current_dir(&root)
        .output()?;
    let lfwx_missing_workflow_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&lfwx_missing_workflow_help.stdout),
        String::from_utf8_lossy(&lfwx_missing_workflow_help.stderr)
    );
    assert!(
        lfwx_missing_workflow_help_text.contains("lfwx <workflow_id>")
            && lfwx_missing_workflow_help_text.contains("records execution under .lightflow/runs/")
            && !lfwx_missing_workflow_help_text.contains("missing workflow id"),
        "output:\n{lfwx_missing_workflow_help_text}"
    );
    let lfwx_missing_patch_help = Command::new(env!("CARGO_BIN_EXE_lfwx"))
        .args(["lightflow.demo", "--patch", "--enable", "node"])
        .current_dir(&root)
        .output()?;
    let lfwx_missing_patch_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&lfwx_missing_patch_help.stdout),
        String::from_utf8_lossy(&lfwx_missing_patch_help.stderr)
    );
    assert!(
        lfwx_missing_patch_help_text.contains("lfwx <workflow_id>")
            && lfwx_missing_patch_help_text.contains("Use '|' between workflow ids")
            && !lfwx_missing_patch_help_text.contains("lfw run <workflow_id>")
            && !lfwx_missing_patch_help_text.contains("missing value for"),
        "output:\n{lfwx_missing_patch_help_text}"
    );
    let lfwx_bad_workflow_help = Command::new(env!("CARGO_BIN_EXE_lfwx"))
        .arg("--bad")
        .current_dir(&root)
        .output()?;
    let lfwx_bad_workflow_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&lfwx_bad_workflow_help.stdout),
        String::from_utf8_lossy(&lfwx_bad_workflow_help.stderr)
    );
    assert!(
        lfwx_bad_workflow_help_text.contains("lfwx <workflow_id>")
            && lfwx_bad_workflow_help_text.contains("Use '|' between workflow ids")
            && !lfwx_bad_workflow_help_text.contains("not found: workflow --bad"),
        "output:\n{lfwx_bad_workflow_help_text}"
    );
    let lfwx_unknown_flag_help = Command::new(env!("CARGO_BIN_EXE_lfwx"))
        .args(["lightflow.demo", "--bad"])
        .current_dir(&root)
        .output()?;
    let lfwx_unknown_flag_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&lfwx_unknown_flag_help.stdout),
        String::from_utf8_lossy(&lfwx_unknown_flag_help.stderr)
    );
    assert!(
        lfwx_unknown_flag_help_text.contains("lfwx <workflow_id>")
            && lfwx_unknown_flag_help_text.contains("Use '|' between workflow ids")
            && !lfwx_unknown_flag_help_text
                .trim()
                .starts_with("unexpected argument"),
        "output:\n{lfwx_unknown_flag_help_text}"
    );
    assert!(
        !root.join(".lightflow/runs").exists(),
        "run help and missing-workflow usage should not create run records"
    );
    let sync_help = lfw_command(&root).args(["sync", "--help"]).output()?;
    let sync_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&sync_help.stdout),
        String::from_utf8_lossy(&sync_help.stderr)
    );
    assert!(
        sync_help_text.contains("lfw sync [workflow_id]")
            && sync_help_text.contains("--model <requirement=variant>")
            && sync_help_text.contains("colocated agent skills")
            && sync_help_text.contains("--locked verifies existing lfw.lock")
            && sync_help_text.contains("--apply writes dependency changes"),
        "output:\n{sync_help_text}"
    );
    for args in [
        vec!["sync", "--model"],
        vec!["sync", "--hf-model"],
        vec!["sync", "--hf-url"],
        vec!["sync", "lightflow.text_plan", "extra"],
        vec!["sync", "--bad"],
    ] {
        let output = lfw_command(&root).args(args).output()?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            text.contains("lfw sync [workflow_id]")
                && text.contains("--model <requirement=variant>")
                && text.contains("--hf-model <requirement=format:repo[:file]>")
                && text.contains("--hf-url <requirement=url>")
                && !text.contains("missing value for")
                && !text.trim().starts_with("unexpected argument"),
            "output:\n{text}"
        );
    }
    let serve_help = lfw_command(&root).args(["serve", "--help"]).output()?;
    let serve_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&serve_help.stdout),
        String::from_utf8_lossy(&serve_help.stderr)
    );
    assert!(
        serve_help_text.contains("lfw serve [--host <host>] [--port <port>]")
            && serve_help_text.contains("Defaults to 127.0.0.1:5174")
            && serve_help_text.contains("/openapi.yaml")
            && serve_help_text.contains("/release"),
        "output:\n{serve_help_text}"
    );
    for args in [
        vec!["serve", "--host"],
        vec!["serve", "--port", "--host", "127.0.0.1"],
        vec!["serve", "--bad"],
    ] {
        let output = lfw_command(&root).args(args).output()?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            text.contains("lfw serve [--host <host>] [--port <port>]")
                && text.contains("Defaults to 127.0.0.1:5174")
                && !text.contains("missing value for")
                && !text.trim().starts_with("unexpected argument"),
            "output:\n{text}"
        );
    }
    let add_help = lfw_command(&root).args(["add", "--help"]).output()?;
    let add_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&add_help.stdout),
        String::from_utf8_lossy(&add_help.stderr)
    );
    assert!(
        add_help_text.contains("lfw add <crate_name>")
            && add_help_text.contains("Registry dependencies require --version")
            && add_help_text.contains("--editable")
            && add_help_text
                .contains("Use lfw import when a repository contains multiple workflow crates"),
        "output:\n{add_help_text}"
    );
    let add_missing = lfw_command(&root).arg("add").output()?;
    let add_missing_text = format!(
        "{}{}",
        String::from_utf8_lossy(&add_missing.stdout),
        String::from_utf8_lossy(&add_missing.stderr)
    );
    assert!(
        add_missing_text.contains("lfw add <crate_name>")
            && add_missing_text.contains("Registry dependencies require --version")
            && !add_missing_text.contains("missing crate name"),
        "output:\n{add_missing_text}"
    );
    for args in [
        vec!["add", "lightflow-demo", "--version"],
        vec!["add", "lightflow-demo", "--path"],
        vec!["add", "lightflow-demo", "--path", "--editable"],
        vec!["add", "lightflow-demo", "--git"],
        vec!["add", "lightflow-demo", "--package"],
        vec!["add", "lightflow-demo"],
        vec!["add", "lightflow-demo", "--editable"],
        vec!["add", "lightflow-demo", "extra"],
        vec!["add", "lightflow-demo", "--version", "1", "--version", "2"],
        vec!["add", "lightflow-demo", "--editable", "--editable"],
        vec!["add", "lightflow-demo", "--package", "a", "--package", "b"],
        vec![
            "add",
            "lightflow-demo",
            "--path",
            "/tmp",
            "--git",
            "https://example.test/repo.git",
        ],
        vec!["add", "--bad"],
    ] {
        let output = lfw_command(&root).args(args).output()?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            text.contains("lfw add <crate_name>")
                && text.contains("Registry dependencies require --version")
                && text
                    .contains("Use lfw import when a repository contains multiple workflow crates")
                && !text.contains("missing value for")
                && !text.contains("registry add requires --version")
                && !text.trim().starts_with("unexpected argument"),
            "output:\n{text}"
        );
    }
    let import_help = lfw_command(&root).args(["import", "--help"]).output()?;
    let import_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&import_help.stdout),
        String::from_utf8_lossy(&import_help.stderr)
    );
    assert!(
        import_help_text.contains("lfw import <path-or-git-url>")
            && import_help_text.contains("workflows/<category>/<crate>")
            && import_help_text.contains("Use lfw add when the target is one known Cargo package")
            && import_help_text.contains("Git imports clone into the LightFlow repo cache")
            && import_help_text
                .contains("--global installs into the default LightFlow home workspace"),
        "output:\n{import_help_text}"
    );
    let import_missing = lfw_command(&root).arg("import").output()?;
    let import_missing_text = format!(
        "{}{}",
        String::from_utf8_lossy(&import_missing.stdout),
        String::from_utf8_lossy(&import_missing.stderr)
    );
    assert!(
        import_missing_text.contains("lfw import <path-or-git-url>")
            && import_missing_text.contains("workflows/<category>/<crate>")
            && !import_missing_text.contains("missing import source"),
        "output:\n{import_missing_text}"
    );
    for args in [
        vec!["import", "https://example.invalid/repo.git", "--name"],
        vec![
            "import",
            "https://example.invalid/repo.git",
            "--name",
            "--global",
        ],
        vec!["import", "--bad"],
    ] {
        let output = lfw_command(&root).args(args).output()?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            text.contains("lfw import <path-or-git-url>")
                && text.contains("Git imports clone into the LightFlow repo cache")
                && !text.contains("missing value for")
                && !text.trim().starts_with("unexpected argument"),
            "output:\n{text}"
        );
    }
    let init_help = lfw_command(&root).args(["init", "--help"]).output()?;
    let init_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&init_help.stdout),
        String::from_utf8_lossy(&init_help.stderr)
    );
    assert!(
        init_help_text.contains("lfw init [--workflow|--plugin] [path]")
            && init_help_text.contains("--workflow creates a workflow collection")
            && init_help_text.contains("--plugin creates a single Cargo crate")
            && init_help_text.contains(".lfwrc")
            && init_help_text.contains("global workflow discovery"),
        "output:\n{init_help_text}"
    );
    let init_bad_flag = lfw_command(&root).args(["init", "--bad"]).output()?;
    let init_bad_flag_text = format!(
        "{}{}",
        String::from_utf8_lossy(&init_bad_flag.stdout),
        String::from_utf8_lossy(&init_bad_flag.stderr)
    );
    assert!(
        init_bad_flag_text.contains("lfw init [--workflow|--plugin] [path]")
            && init_bad_flag_text.contains("global workflow discovery")
            && !init_bad_flag_text.trim().starts_with("unexpected argument"),
        "output:\n{init_bad_flag_text}"
    );
    let new_help = lfw_command(&root).args(["new", "--help"]).output()?;
    let new_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&new_help.stdout),
        String::from_utf8_lossy(&new_help.stderr)
    );
    assert!(
        new_help_text.contains("lfw new <workflow_id> --category <name>")
            && new_help_text.contains("colocated agent skill")
            && new_help_text.contains("--category selects the workflows/<category>/<crate>")
            && new_help_text.contains("--runtime selects a runtime-aware template")
            && new_help_text.contains("--global creates the workflow"),
        "output:\n{new_help_text}"
    );
    let new_missing = lfw_command(&root).arg("new").output()?;
    let new_missing_text = format!(
        "{}{}",
        String::from_utf8_lossy(&new_missing.stdout),
        String::from_utf8_lossy(&new_missing.stderr)
    );
    assert!(
        new_missing_text.contains("lfw new <workflow_id> --category <name>")
            && new_missing_text.contains("colocated agent skill")
            && !new_missing_text.contains("missing workflow id"),
        "output:\n{new_missing_text}"
    );
    for args in [
        vec!["new", "lightflow.demo", "--category"],
        vec!["new", "lightflow.demo", "--category", "--runtime", "image"],
        vec!["new", "lightflow.demo", "--runtime"],
        vec!["new", "lightflow.demo", "--name"],
        vec!["new", "--bad"],
    ] {
        let output = lfw_command(&root).args(args).output()?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            text.contains("lfw new <workflow_id> --category <name>")
                && text.contains("--runtime selects a runtime-aware template")
                && !text.contains("missing value for")
                && !text.trim().starts_with("unexpected argument"),
            "output:\n{text}"
        );
    }
    let update_help = lfw_command(&root).args(["update", "--help"]).output()?;
    let update_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&update_help.stdout),
        String::from_utf8_lossy(&update_help.stderr)
    );
    assert!(
        update_help_text.contains("lfw update [--global|-g]")
            && update_help_text.contains("cargo fetch")
            && update_help_text.contains("default LightFlow home workflow workspace")
            && update_help_text.contains("Use update to fetch dependency indexes"),
        "output:\n{update_help_text}"
    );
    let upgrade_help = lfw_command(&root).args(["upgrade", "--help"]).output()?;
    let upgrade_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&upgrade_help.stdout),
        String::from_utf8_lossy(&upgrade_help.stderr)
    );
    assert!(
        upgrade_help_text.contains("lfw upgrade [--global|-g]")
            && upgrade_help_text.contains("cargo update")
            && upgrade_help_text.contains("update Cargo.lock resolution"),
        "output:\n{upgrade_help_text}"
    );
    for args in [vec!["update", "--bad"], vec!["upgrade", "--bad"]] {
        let output = lfw_command(&root).args(args).output()?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            text.contains("lfw update [--global|-g]") || text.contains("lfw upgrade [--global|-g]"),
            "output:\n{text}"
        );
        assert!(
            !text.trim().starts_with("unexpected argument"),
            "output:\n{text}"
        );
    }
    let list_help = lfw_command(&root).args(["list", "--help"]).output()?;
    let list_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&list_help.stdout),
        String::from_utf8_lossy(&list_help.stderr)
    );
    assert!(
        list_help_text.contains("lfw list [--brief|--detail] [--category <name>]")
            && list_help_text.contains("lfw list --categories")
            && list_help_text.contains("active workflow catalog")
            && list_help_text.contains("--detail includes inputs, outputs, nodes")
            && list_help_text.contains("--categories returns category counts"),
        "output:\n{list_help_text}"
    );
    for args in [
        vec!["list", "--category"],
        vec!["list", "--category", "--detail"],
        vec!["list", "--bad"],
    ] {
        let output = lfw_command(&root).args(args).output()?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            text.contains("lfw list [--brief|--detail] [--category <name>]")
                && text.contains("lfw list --categories")
                && !text.contains("missing value for")
                && !text.trim().starts_with("unexpected argument"),
            "output:\n{text}"
        );
    }
    let home_help = lfw_command(&root).args(["home", "--help"]).output()?;
    let home_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&home_help.stdout),
        String::from_utf8_lossy(&home_help.stderr)
    );
    assert!(
        home_help_text.contains("lfw home")
            && home_help_text.contains("home, manifest, workflows, repos, and lfw_path")
            && home_help_text.contains("global workflow discovery")
            && home_help_text.contains("default home workspace"),
        "output:\n{home_help_text}"
    );
    let info_help = lfw_command(&root).args(["info", "--help"]).output()?;
    let info_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&info_help.stdout),
        String::from_utf8_lossy(&info_help.stderr)
    );
    assert!(
        info_help_text.contains("lfw info")
            && info_help_text.contains("lfw arch")
            && info_help_text.contains("runtime config")
            && info_help_text.contains("active workflow catalog")
            && info_help_text.contains("executor registry"),
        "output:\n{info_help_text}"
    );
    let trace_help = lfw_command(&root).args(["trace", "--help"]).output()?;
    let trace_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&trace_help.stdout),
        String::from_utf8_lossy(&trace_help.stderr)
    );
    assert!(
        trace_help_text.contains("lfw trace [last|run_id]")
            && trace_help_text.contains(".lightflow/runs/")
            && trace_help_text.contains("stored manifest, execution, artifacts, and events"),
        "output:\n{trace_help_text}"
    );
    let replay_help = lfw_command(&root).args(["replay", "--help"]).output()?;
    let replay_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&replay_help.stdout),
        String::from_utf8_lossy(&replay_help.stderr)
    );
    assert!(
        replay_help_text.contains("lfw replay [last|run_id]")
            && replay_help_text.contains("rerun the stored stage definitions"),
        "output:\n{replay_help_text}"
    );
    let runs_help = lfw_command(&root).args(["runs", "--help"]).output()?;
    let runs_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&runs_help.stdout),
        String::from_utf8_lossy(&runs_help.stderr)
    );
    assert!(
        runs_help_text.contains("lfw runs list [--limit <n>]")
            && runs_help_text.contains("lfw runs rm <last|run_id>")
            && runs_help_text.contains("Inspects and replays recorded workflow runs"),
        "output:\n{runs_help_text}"
    );
    for args in [
        vec!["runs", "list", "--limit"],
        vec!["runs", "list", "--limit", "nope"],
        vec!["runs", "list", "--workflow"],
        vec!["runs", "list", "--status", "--limit", "1"],
        vec!["runs", "list", "--bad"],
        vec!["runs", "get", "--bad"],
        vec!["runs", "replay", "--bad"],
        vec!["runs", "rm"],
        vec!["trace", "--bad"],
        vec!["replay", "--bad"],
    ] {
        let output = lfw_command(&root).args(args).output()?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            text.contains("lfw runs list [--limit <n>]")
                && text.contains("Inspects and replays recorded workflow runs")
                && !text.contains("missing value for"),
            "output:\n{text}"
        );
        assert!(
            !text.contains("missing run id") && !text.contains("not found: run --bad"),
            "output:\n{text}"
        );
        assert!(
            !text.trim().starts_with("unexpected argument"),
            "output:\n{text}"
        );
    }
    let runs_replay_help = lfw_command(&root)
        .args(["runs", "replay", "--help"])
        .output()?;
    let runs_replay_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&runs_replay_help.stdout),
        String::from_utf8_lossy(&runs_replay_help.stderr)
    );
    assert!(
        runs_replay_help_text.contains("lfw runs replay [last|run_id]")
            && runs_replay_help_text.contains("rerun the stored stage definitions"),
        "output:\n{runs_replay_help_text}"
    );
    let runs_rm_help = lfw_command(&root).args(["runs", "rm", "--help"]).output()?;
    let runs_rm_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&runs_rm_help.stdout),
        String::from_utf8_lossy(&runs_rm_help.stderr)
    );
    assert!(
        runs_rm_help_text.contains("lfw runs rm <last|run_id>")
            && runs_rm_help_text.contains("Inspects and replays recorded workflow runs")
            && !runs_rm_help_text.contains("\"removed\""),
        "output:\n{runs_rm_help_text}"
    );
    let artifacts_help = lfw_command(&root).args(["artifacts", "--help"]).output()?;
    let artifacts_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&artifacts_help.stdout),
        String::from_utf8_lossy(&artifacts_help.stderr)
    );
    assert!(
        artifacts_help_text.contains("lfw artifacts [--run <last|run_id>]")
            && artifacts_help_text.contains("--workflow <workflow_id>")
            && artifacts_help_text.contains("--kind <kind>")
            && artifacts_help_text.contains(".lightflow/runs/")
            && artifacts_help_text.contains("run, stage, node, workflow, kind, path")
            && artifacts_help_text.contains("lfw artifacts --run last"),
        "output:\n{artifacts_help_text}"
    );
    for args in [
        vec!["artifacts", "--run"],
        vec!["artifacts", "--run", "--kind", "image"],
        vec!["artifacts", "--workflow"],
        vec!["artifacts", "--limit"],
        vec!["artifacts", "--limit", "nope"],
        vec!["artifacts", "--bad"],
    ] {
        let output = lfw_command(&root).args(args).output()?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            text.contains("lfw artifacts [--run <last|run_id>]")
                && text.contains("lfw artifacts --run last")
                && !text.contains("missing value for")
                && !text.trim().starts_with("unexpected argument"),
            "output:\n{text}"
        );
    }
    let models_requirements_help = lfw_command(&root)
        .args(["models", "requirements", "--help"])
        .output()?;
    let models_requirements_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&models_requirements_help.stdout),
        String::from_utf8_lossy(&models_requirements_help.stderr)
    );
    assert!(
        models_requirements_help_text.contains("lfw models requirements")
            && models_requirements_help_text.contains("--status all|available|blocked")
            && models_requirements_help_text.contains("missing locks")
            && models_requirements_help_text.contains("sync/verify commands"),
        "output:\n{models_requirements_help_text}"
    );
    for args in [
        vec!["models", "requirements", "--workflow"],
        vec!["models", "requirements", "--workflow", "--blocked"],
        vec!["models", "requirements", "--status"],
        vec!["models", "requirements", "--bad"],
    ] {
        let output = lfw_command(&root).args(args).output()?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            text.contains("lfw models requirements")
                && text.contains("--status all|available|blocked")
                && !text.contains("missing workflow id")
                && !text.contains("missing model status")
                && !text.contains("not found: workflow --blocked")
                && !text.trim().starts_with("unexpected argument"),
            "output:\n{text}"
        );
    }
    let models_download_missing = lfw_command(&root).args(["models", "download"]).output()?;
    let models_download_missing_text = format!(
        "{}{}",
        String::from_utf8_lossy(&models_download_missing.stdout),
        String::from_utf8_lossy(&models_download_missing.stderr)
    );
    assert!(
        models_download_missing_text.contains("lfw models download <repo> [file]")
            && models_download_missing_text.contains("lfw models download <huggingface-file-url>")
            && !models_download_missing_text.contains("missing repo or Hugging Face URL"),
        "output:\n{models_download_missing_text}"
    );
    let models_rm_missing = lfw_command(&root).args(["models", "rm"]).output()?;
    let models_rm_missing_text = format!(
        "{}{}",
        String::from_utf8_lossy(&models_rm_missing.stdout),
        String::from_utf8_lossy(&models_rm_missing.stderr)
    );
    assert!(
        models_rm_missing_text.contains("lfw models rm <repo|cache_id|path>")
            && models_rm_missing_text.contains("Inspects workflow model requirements")
            && !models_rm_missing_text.contains("missing cache entry"),
        "output:\n{models_rm_missing_text}"
    );
    for args in [
        vec!["models", "list", "extra"],
        vec!["models", "prune", "extra"],
        vec!["models", "rm", "target", "extra"],
        vec!["models", "download", "repo", "file", "extra"],
    ] {
        let output = lfw_command(&root).args(args).output()?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            text.contains("lfw models list")
                && text.contains("lfw models prune")
                && text.contains("Inspects workflow model requirements")
                && !text.trim().starts_with("unexpected argument"),
            "output:\n{text}"
        );
    }
    let patch_validate_help = lfw_command(&root)
        .args(["patch", "validate", "--help"])
        .output()?;
    let patch_validate_help_text = format!(
        "{}{}",
        String::from_utf8_lossy(&patch_validate_help.stdout),
        String::from_utf8_lossy(&patch_validate_help.stderr)
    );
    assert!(
        patch_validate_help_text.contains("lfw patch validate")
            && patch_validate_help_text.contains(".lightflow/patches/")
            && patch_validate_help_text.contains("unknown nodes and port mismatches"),
        "output:\n{patch_validate_help_text}"
    );
    let patch_validate_bad_json = lfw_command(&root)
        .args(["patch", "validate", "{bad-json"])
        .output()?;
    assert!(!patch_validate_bad_json.status.success());
    let patch_validate_bad_json_text = format!(
        "{}{}",
        String::from_utf8_lossy(&patch_validate_bad_json.stdout),
        String::from_utf8_lossy(&patch_validate_bad_json.stderr)
    );
    assert!(
        patch_validate_bad_json_text.contains("invalid patch JSON")
            && patch_validate_bad_json_text
                .contains("lfw patch validate <json|-|@file|registered-name>")
            && patch_validate_bad_json_text.contains(".lightflow/patches/"),
        "output:\n{patch_validate_bad_json_text}"
    );
    let patch_validate_missing_workflow = lfw_command(&root)
        .args(["patch", "validate", "{}", "--workflow"])
        .output()?;
    assert!(!patch_validate_missing_workflow.status.success());
    let patch_validate_missing_workflow_text = format!(
        "{}{}",
        String::from_utf8_lossy(&patch_validate_missing_workflow.stdout),
        String::from_utf8_lossy(&patch_validate_missing_workflow.stderr)
    );
    assert!(
        patch_validate_missing_workflow_text
            .contains("lfw patch validate <json|-|@file|registered-name>")
            && patch_validate_missing_workflow_text.contains(".lightflow/patches/")
            && !patch_validate_missing_workflow_text.contains("missing workflow id"),
        "output:\n{patch_validate_missing_workflow_text}"
    );
    let patch_validate_bad_flag = lfw_command(&root)
        .args(["patch", "validate", "{}", "--bad"])
        .output()?;
    assert!(!patch_validate_bad_flag.status.success());
    let patch_validate_bad_flag_text = format!(
        "{}{}",
        String::from_utf8_lossy(&patch_validate_bad_flag.stdout),
        String::from_utf8_lossy(&patch_validate_bad_flag.stderr)
    );
    assert!(
        patch_validate_bad_flag_text.contains("lfw patch validate <json|-|@file|registered-name>")
            && patch_validate_bad_flag_text.contains(".lightflow/patches/")
            && !patch_validate_bad_flag_text
                .trim()
                .starts_with("unexpected argument"),
        "output:\n{patch_validate_bad_flag_text}"
    );
    for args in [
        ["patch", "get"],
        ["patch", "save"],
        ["patch", "validate"],
        ["patch", "rm"],
    ] {
        let output = lfw_command(&root).args(args).output()?;
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert!(
            text.contains("lfw patch save <name> <json|-|@file|registered-name>")
                && text.contains("lfw patch validate <json|-|@file|registered-name>")
                && text.contains(".lightflow/patches/")
                && !text.contains("missing patch name")
                && !text.contains("missing patch json")
                && !text.contains("missing patch json or name"),
            "output:\n{text}"
        );
    }
    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_release_check_allows_configured_selected_workflow_gate()
-> Result<(), Box<dyn std::error::Error>> {
    let report = lfw(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        ["release", "check", "--workflow", "lightflow.text_to_image"],
    )?;
    assert_eq!(report["dry_run"], true);
    assert_eq!(report["valid"], true);
    assert_eq!(report["workflow_id"], "lightflow.text_to_image");
    assert!(
        report["warnings"]
            .as_array()
            .expect("release warnings")
            .iter()
            .any(|warning| warning
                .as_str()
                .unwrap_or_default()
                .contains("release.review.selected_workflow_loop"))
    );
    let checks = report["checks"].as_array().expect("release checks");
    assert!(checks.iter().any(|check| {
        check["id"] == "release.review.selected_workflow_loop"
            && check["status"] == "warning"
            && check["message"]
                .as_str()
                .is_some_and(|message| message.contains("loop.selected.models"))
    }));
    assert!(checks.iter().any(|check| {
        check["id"] == "release.review.selected_workflow_loop"
            && check["details"].as_array().is_some_and(|details| {
                details.iter().any(|detail| {
                    detail
                        .as_str()
                        .unwrap_or_default()
                        .contains("loop.selected.models")
                })
            })
    }));
    assert!(checks.iter().any(|check| {
        check["id"] == "release.command.selected_workflow_loop"
            && check["status"] == "planned"
            && check["command"]
                == serde_json::json!([
                    "cargo",
                    "run",
                    "--bin",
                    "lfw",
                    "--",
                    "loop",
                    "check",
                    "lightflow.text_to_image",
                    "--require-replay"
                ])
    }));
    assert!(checks.iter().any(|check| {
        check["id"] == "release.command.project_workspaces"
            && check["status"] == "planned"
            && check["command"]
                == serde_json::json!(["cargo", "run", "--bin", "lfw", "--", "loop", "projects"])
    }));
    assert!(checks.iter().any(|check| {
        check["id"] == "release.command.dirty_project_workspaces"
            && check["status"] == "planned"
            && check["command"]
                == serde_json::json!([
                    "cargo", "run", "--bin", "lfw", "--", "loop", "projects", "--dirty"
                ])
    }));
    Ok(())
}

#[test]
fn lfw_loop_check_can_require_selected_replay_readiness() -> Result<(), Box<dyn std::error::Error>>
{
    let output = lfw_command(Path::new(env!("CARGO_MANIFEST_DIR")))
        .args(["loop", "check", "lightflow.std", "--require-replay"])
        .output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("\"id\":\"loop.selected.replay.required\""),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("needs a completed recorded run"),
        "stderr:\n{stderr}"
    );

    let text_plan = lfw(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        ["loop", "check", "lightflow.text_plan", "--require-replay"],
    )?;
    assert_eq!(text_plan["valid"], true);
    assert!(text_plan["checks"].as_array().unwrap().iter().any(|check| {
        check["id"] == "loop.selected.replay.required" && check["status"] == "passed"
    }));
    Ok(())
}

#[test]
fn lfw_release_check_apply_skips_commands_after_failed_artifacts()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    fs::write(
        root.join("Cargo.toml"),
        r#"[package]
name = "release-skip-test"
version = "0.1.0"
edition = "2024"
"#,
    )?;
    fs::create_dir_all(root.join("src"))?;
    fs::write(root.join("src/lib.rs"), "")?;

    let output = lfw_command(&root)
        .args(["release", "check", "--apply"])
        .output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("\"valid\":false"), "stderr:\n{stderr}");
    assert!(
        stderr.contains("\"issues\":[\"release.artifact.changelog: required release artifact is missing: CHANGELOG.md\""),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("\"status\":\"skipped\""),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("command skipped because an earlier release gate failed"),
        "stderr:\n{stderr}"
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_release_check_apply_skips_commands_after_failed_command()
-> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(root.join("docs"))?;
    fs::create_dir_all(root.join("src"))?;
    fs::write(
        root.join("Cargo.toml"),
        r#"[package]
name = "release-command-skip-test"
version = "0.1.0"
edition = "2024"
"#,
    )?;
    fs::write(
        root.join("src/lib.rs"),
        "pub fn badly_formatted( )->i32{1}\n",
    )?;
    fs::write(
        root.join("CHANGELOG.md"),
        r#"### CLI
### API
### Workflows
### Runtime
### Known Limitations
### Migration Notes
"#,
    )?;
    fs::write(root.join("docs/v0.2-checklist.md"), "- release fixture\n")?;
    fs::write(
        root.join("docs/runtime-verification.md"),
        "- release fixture\n",
    )?;
    fs::write(
        root.join("docs/local-workflow-loop.md"),
        "## Verification Gates\n",
    )?;

    let output = lfw_command(&root)
        .args(["release", "check", "--apply"])
        .output()?;
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("\"valid\":false"), "stderr:\n{stderr}");
    assert!(
        stderr.contains("\"id\":\"release.command.fmt\""),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("\"status\":\"failed\""),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("\"id\":\"release.command.local_workflow_loop\""),
        "stderr:\n{stderr}"
    );
    assert!(
        stderr.contains("\"status\":\"skipped\""),
        "stderr:\n{stderr}"
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_loop_check_reports_local_workflow_readiness() -> Result<(), Box<dyn std::error::Error>> {
    let report = lfw(Path::new(env!("CARGO_MANIFEST_DIR")), ["loop", "check"])?;
    assert_eq!(report["valid"], true);
    assert_eq!(report["project_root"], env!("CARGO_MANIFEST_DIR"));

    let checks = report["checks"].as_array().expect("loop checks");
    let passed = checks
        .iter()
        .filter(|check| check["status"] == "passed")
        .count();
    let warnings = checks
        .iter()
        .filter(|check| check["status"] == "warning")
        .count();
    let failed = checks
        .iter()
        .filter(|check| check["status"] == "failed")
        .count();
    assert_eq!(report["passed"], passed);
    assert_eq!(report["warnings"], warnings);
    assert_eq!(report["failed"], failed);
    assert_eq!(
        report["issues"].as_array().expect("loop issues").len(),
        failed
    );
    assert_eq!(
        report["warning_messages"]
            .as_array()
            .expect("loop warning messages")
            .len(),
        warnings
    );
    for id in [
        "loop.document.local_workflow_loop",
        "loop.document.verification_gates",
        "loop.projects.sibling_workspaces",
        "loop.workflow.discovery",
        "loop.workflow.agent_skills",
        "loop.executor.catalog",
        "loop.publish.workflow_crates",
    ] {
        assert!(
            checks
                .iter()
                .any(|check| check["id"] == id && check["status"] == "passed"),
            "missing passed local loop check {id}"
        );
    }
    assert!(
        checks.iter().any(|check| {
            check["id"] == "loop.models.readiness"
                && (check["status"] == "passed" || check["status"] == "warning")
        }),
        "missing non-failed model readiness check"
    );
    assert!(
        checks.iter().any(|check| {
            check["id"] == "loop.source_changes.safety"
                && (check["status"] == "passed" || check["status"] == "warning")
        }),
        "missing non-failed local loop source-change check"
    );
    assert!(
        checks
            .iter()
            .any(|check| check["id"] == "loop.history.runs")
    );
    assert!(
        checks
            .iter()
            .any(|check| check["id"] == "loop.patches.registry")
    );
    assert!(
        checks.iter().any(|check| {
            check["id"] == "loop.publish.readiness" && check["status"] == "passed"
        })
    );
    assert!(
        report["next_commands"]
            .as_array()
            .expect("next commands")
            .iter()
            .any(|command| command == &serde_json::json!(["lfw", "replay", "last"]))
    );
    assert!(
        report["next_commands"]
            .as_array()
            .expect("next commands")
            .iter()
            .any(|command| command == &serde_json::json!(["lfw", "loop", "changes"]))
    );
    assert!(
        report["next_commands"]
            .as_array()
            .expect("next commands")
            .iter()
            .any(|command| command == &serde_json::json!(["lfw", "loop", "projects"]))
    );
    assert!(
        report["next_commands"]
            .as_array()
            .expect("next commands")
            .iter()
            .any(|command| command == &serde_json::json!(["lfw", "publish", "--workflows"]))
    );

    let targeted = lfw(
        Path::new(env!("CARGO_MANIFEST_DIR")),
        ["loop", "check", "lightflow.text_plan"],
    )?;
    assert_eq!(targeted["valid"], true);
    assert!(
        targeted["replay_run_id"]
            .as_str()
            .is_some_and(|run_id| run_id.starts_with("run-")),
        "targeted loop report:\n{targeted:#?}"
    );
    assert!(
        targeted["next_commands"]
            .as_array()
            .expect("targeted next commands")
            .iter()
            .any(|command| {
                command
                    == &serde_json::json!([
                        "lfw",
                        "models",
                        "requirements",
                        "lightflow.text_plan",
                        "--blocked"
                    ])
            })
    );
    assert!(
        targeted["next_commands"]
            .as_array()
            .expect("targeted next commands")
            .iter()
            .any(|command| {
                command
                    == &serde_json::json!([
                        "lfw",
                        "sync",
                        "lightflow.text_plan",
                        "--auto-model",
                        "--apply"
                    ])
            })
    );
    assert!(
        targeted["next_commands"]
            .as_array()
            .expect("targeted next commands")
            .iter()
            .any(|command| {
                command.as_array().is_some_and(|parts| {
                    parts.first() == Some(&serde_json::json!("lfw"))
                        && parts.get(1) == Some(&serde_json::json!("replay"))
                        && parts
                            .get(2)
                            .and_then(serde_json::Value::as_str)
                            .is_some_and(|run_id| run_id.starts_with("run-"))
                })
            })
    );
    assert!(
        targeted["next_commands"]
            .as_array()
            .expect("targeted next commands")
            .iter()
            .any(|command| command == &serde_json::json!(["lfw", "loop", "changes"]))
    );
    assert!(
        targeted["next_commands"]
            .as_array()
            .expect("targeted next commands")
            .iter()
            .any(|command| command == &serde_json::json!(["lfw", "loop", "projects"]))
    );
    assert!(
        targeted["next_commands"]
            .as_array()
            .expect("targeted next commands")
            .iter()
            .any(|command| command == &serde_json::json!(["lfw", "publish", "lightflow.text_plan"]))
    );
    Ok(())
}

#[test]
fn mcp_exposes_backend_tools() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    write_project_specs(&root)?;
    write_workflow_crate(
        &root.join("projects/lightflow-std"),
        "lightflow.std_project",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.std_project")
        .version("0.1.0")
        .name("Std Project")
        .description("Std project publish resource fixture.")
        .input("value", "json")
        .input_description("value", "Fixture input value.")
        .output("value", "json")
        .output_description("value", "Fixture output value.")
        .build()
}
"#,
    )?;
    let service = ApiService::new(&root);

    let mcp_help = lfw_command(&root).args(["mcp", "--help"]).output()?;
    assert!(!mcp_help.status.success());
    let mcp_help_stderr = String::from_utf8_lossy(&mcp_help.stderr);
    assert!(
        mcp_help_stderr.contains("usage:\n  lfw mcp [<json|-|@file>]"),
        "mcp help stderr:\n{mcp_help_stderr}"
    );
    assert!(
        mcp_help_stderr.contains("resources/templates/list")
            && mcp_help_stderr.contains("lightflow://workflows/{workflow_id}/plan"),
        "mcp help stderr:\n{mcp_help_stderr}"
    );
    assert!(
        mcp_help_stderr.contains("resources:")
            && mcp_help_stderr.contains("  lightflow://mcp")
            && mcp_help_stderr.contains("resource templates:")
            && mcp_help_stderr.contains("  lightflow://workflows/{workflow_id}/publish")
            && mcp_help_stderr.contains("  lightflow://patches/{name}"),
        "mcp help stderr:\n{mcp_help_stderr}"
    );
    assert!(
        !mcp_help_stderr.contains("invalid number"),
        "mcp help stderr:\n{mcp_help_stderr}"
    );

    let empty_mcp = lfw_command(&root).arg("mcp").output()?;
    assert!(!empty_mcp.status.success());
    let empty_mcp_stderr = String::from_utf8_lossy(&empty_mcp.stderr);
    assert!(
        empty_mcp_stderr.contains("usage:\n  lfw mcp [<json|-|@file>]")
            && empty_mcp_stderr.contains("resources/templates/list")
            && !empty_mcp_stderr.contains("EOF while parsing"),
        "empty mcp stderr:\n{empty_mcp_stderr}"
    );

    let mcp_file_request = root.join("mcp-ping.json");
    fs::write(
        &mcp_file_request,
        r#"{"jsonrpc":"2.0","id":45,"method":"ping"}"#,
    )?;
    let mcp_file = lfw(&root, ["mcp", &format!("@{}", mcp_file_request.display())])?;
    assert_eq!(mcp_file["id"], 45);
    assert_eq!(mcp_file["result"], serde_json::json!({}));

    let mut mcp_stdin_child = lfw_command(&root)
        .args(["mcp", "-"])
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()?;
    {
        use std::io::Write;
        let stdin = mcp_stdin_child.stdin.as_mut().expect("mcp stdin pipe");
        stdin.write_all(br#"{"jsonrpc":"2.0","id":46,"method":"ping"}"#)?;
    }
    let mcp_stdin_output = mcp_stdin_child.wait_with_output()?;
    assert!(
        mcp_stdin_output.status.success(),
        "mcp stdin stdout:\n{}\nstderr:\n{}",
        String::from_utf8_lossy(&mcp_stdin_output.stdout),
        String::from_utf8_lossy(&mcp_stdin_output.stderr)
    );
    let mcp_stdin: serde_json::Value = serde_json::from_slice(&mcp_stdin_output.stdout)?;
    assert_eq!(mcp_stdin["id"], 46);
    assert_eq!(mcp_stdin["result"], serde_json::json!({}));

    let invalid_request = mcp_response(&service, serde_json::json!({ "id": 47 }));
    assert_eq!(invalid_request["id"], 47);
    assert_mcp_error(&invalid_request, -32600, "invalid JSON-RPC request");

    let unknown_method = mcp_response(
        &service,
        serde_json::json!({ "id": 48, "method": "lightflow.nope" }),
    );
    assert_eq!(unknown_method["id"], 48);
    assert_mcp_error(
        &unknown_method,
        -32601,
        "unknown MCP method: lightflow.nope",
    );

    let missing_resource_uri = mcp_response(
        &service,
        serde_json::json!({ "id": 49, "method": "resources/read", "params": {} }),
    );
    assert_eq!(missing_resource_uri["id"], 49);
    assert_mcp_error(
        &missing_resource_uri,
        -32602,
        "resources/read requires params.uri",
    );

    let initialize = mcp_result(
        &service,
        serde_json::json!({ "id": 44, "method": "initialize" }),
    );
    assert_eq!(initialize["protocolVersion"], "2024-11-05");
    assert_eq!(initialize["serverInfo"]["name"], "lightflow");
    assert_eq!(
        initialize["serverInfo"]["version"],
        env!("CARGO_PKG_VERSION")
    );
    assert_eq!(
        initialize["capabilities"]["tools"],
        serde_json::json!({ "listChanged": false })
    );
    assert_eq!(
        initialize["capabilities"]["resources"],
        serde_json::json!({ "subscribe": false, "listChanged": false })
    );

    let tools = mcp_result(
        &service,
        serde_json::json!({ "id": 1, "method": "tools/list" }),
    );
    let tool_names = tools["tools"]
        .as_array()
        .expect("tools/list returns an array")
        .iter()
        .map(|tool| tool["name"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(
        tool_names,
        vec![
            "lightflow.workflow.list",
            "lightflow.workflow.get",
            "lightflow.workflow.dependencies",
            "lightflow.workflow.plan",
            "lightflow.workflow.publish_check",
            "lightflow.workflow.publish_list",
            "lightflow.workflow.run",
            "lightflow.workflow.validate",
            "lightflow.workflow.save",
            "lightflow.node.list",
            "lightflow.node.get",
            "lightflow.executor.list",
            "lightflow.model.list",
            "lightflow.run.list",
            "lightflow.run.get",
            "lightflow.run.events",
            "lightflow.run.replay",
            "lightflow.run.rm",
            "lightflow.artifact.list",
            "lightflow.patch.list",
            "lightflow.patch.get",
            "lightflow.patch.save",
            "lightflow.patch.validate",
            "lightflow.patch.rm",
            "lightflow.loop.check",
            "lightflow.loop.changes",
            "lightflow.loop.projects",
            "lightflow.release.check"
        ]
    );

    let cli_tools = lfw(
        &root,
        ["mcp", r#"{"jsonrpc":"2.0","id":7,"method":"tools/list"}"#],
    )?;
    assert_eq!(cli_tools["jsonrpc"], "2.0");
    assert_eq!(cli_tools["id"], 7);
    assert_eq!(
        cli_tools["result"]["tools"][0]["name"],
        "lightflow.workflow.list"
    );
    for tool_name in ["lightflow.workflow.validate", "lightflow.workflow.save"] {
        let workflow_tool = tools["tools"]
            .as_array()
            .expect("tools")
            .iter()
            .find(|tool| tool["name"] == tool_name)
            .expect("workflow schema tool");
        assert!(
            workflow_tool["inputSchema"]["properties"]["workflow"]["description"]
                .as_str()
                .expect("workflow schema description")
                .contains("WorkflowSpec JSON object"),
            "workflow tool:\n{workflow_tool:#?}"
        );
    }
    let workflow_get_tool = tools["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|tool| tool["name"] == "lightflow.workflow.get")
        .expect("workflow get tool");
    assert!(
        workflow_get_tool["inputSchema"]["properties"]["workflow_id"]["description"]
            .as_str()
            .expect("workflow id description")
            .contains("discovered workflow"),
        "workflow get tool:\n{workflow_get_tool:#?}"
    );
    let run_get_tool = tools["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|tool| tool["name"] == "lightflow.run.get")
        .expect("run get tool");
    assert!(
        run_get_tool["inputSchema"]["properties"]["run_id"]["description"]
            .as_str()
            .expect("run id description")
            .contains("use last"),
        "run get tool:\n{run_get_tool:#?}"
    );
    let patch_get_tool = tools["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|tool| tool["name"] == "lightflow.patch.get")
        .expect("patch get tool");
    assert!(
        patch_get_tool["inputSchema"]["properties"]["name"]["description"]
            .as_str()
            .expect("patch name description")
            .contains(".lightflow/patches"),
        "patch get tool:\n{patch_get_tool:#?}"
    );
    let workflow_run_tool = tools["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|tool| tool["name"] == "lightflow.workflow.run")
        .expect("workflow run tool");
    assert!(
        workflow_run_tool["inputSchema"]["properties"]["inputs"]["description"]
            .as_str()
            .expect("workflow run inputs description")
            .contains("input port name"),
        "workflow run tool:\n{workflow_run_tool:#?}"
    );
    assert!(
        workflow_run_tool["inputSchema"]["properties"]["disabled_nodes"]["description"]
            .as_str()
            .expect("workflow run disabled nodes description")
            .contains("Graph node ids"),
        "workflow run tool:\n{workflow_run_tool:#?}"
    );
    assert!(
        workflow_run_tool["inputSchema"]["properties"]["patch"]["description"]
            .as_str()
            .expect("workflow run patch description")
            .contains("Serializable run patch"),
        "workflow run tool:\n{workflow_run_tool:#?}"
    );
    let patch_save_tool = tools["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|tool| tool["name"] == "lightflow.patch.save")
        .expect("patch save tool");
    assert!(
        patch_save_tool["inputSchema"]["properties"]["name"]["description"]
            .as_str()
            .expect("patch save name description")
            .contains(".lightflow/patches"),
        "patch save tool:\n{patch_save_tool:#?}"
    );
    assert!(
        patch_save_tool["inputSchema"]["properties"]["patch"]["description"]
            .as_str()
            .expect("patch save patch description")
            .contains("node-keyed replacement"),
        "patch save tool:\n{patch_save_tool:#?}"
    );
    let patch_validate_tool = tools["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|tool| tool["name"] == "lightflow.patch.validate")
        .expect("patch validate tool");
    assert!(
        patch_validate_tool["inputSchema"]["properties"]["patch"]["description"]
            .as_str()
            .expect("patch validate patch description")
            .contains("node-keyed replacement"),
        "patch validate tool:\n{patch_validate_tool:#?}"
    );
    let loop_tool = tools["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|tool| tool["name"] == "lightflow.loop.check")
        .expect("loop check tool");
    assert_eq!(
        loop_tool["inputSchema"]["properties"]["require_replay"]["type"],
        "boolean"
    );
    let loop_projects_tool = tools["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|tool| tool["name"] == "lightflow.loop.projects")
        .expect("loop projects tool");
    let loop_projects_description = loop_projects_tool["description"]
        .as_str()
        .expect("loop projects description");
    assert!(
        loop_projects_description.contains("project config metadata")
            && loop_projects_description.contains("optional workspaces")
            && loop_projects_description.contains("submodule initialization commands")
            && loop_projects_description.contains("child stage/commit/push commands")
            && loop_projects_description.contains("parent gitlink staging commands"),
        "loop projects tool:\n{loop_projects_tool:#?}"
    );
    assert!(
        loop_projects_tool["inputSchema"]["properties"]["project"]["description"]
            .as_str()
            .expect("loop projects project description")
            .contains("std, flux, rig, auto-editing"),
        "loop projects tool:\n{loop_projects_tool:#?}"
    );
    assert!(
        loop_projects_tool["inputSchema"]["properties"]["dirty"]["description"]
            .as_str()
            .expect("loop projects dirty description")
            .contains("stale parent gitlinks"),
        "loop projects tool:\n{loop_projects_tool:#?}"
    );
    let release_tool = tools["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|tool| tool["name"] == "lightflow.release.check")
        .expect("release check tool");
    let release_description = release_tool["description"]
        .as_str()
        .expect("release description");
    assert!(
        release_description.contains("project config metadata")
            && release_description.contains("known optional workspaces")
            && release_description.contains("configured submodule initialization commands"),
        "release tool:\n{release_tool:#?}"
    );
    assert_eq!(
        release_tool["inputSchema"]["properties"]["workflow_id"]["type"],
        "string"
    );
    assert_eq!(
        release_tool["inputSchema"]["properties"]["project"]["type"],
        "string"
    );
    assert!(
        release_tool["inputSchema"]["properties"]["project"]["description"]
            .as_str()
            .expect("release project description")
            .contains("std, flux, rig, auto-editing"),
        "release tool:\n{release_tool:#?}"
    );
    assert!(
        release_tool["description"]
            .as_str()
            .expect("release tool description")
            .contains("source-change review")
    );
    let run_list_tool = tools["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|tool| tool["name"] == "lightflow.run.list")
        .expect("run list tool");
    assert_eq!(
        run_list_tool["inputSchema"]["properties"]["limit"]["type"],
        "integer"
    );
    assert_eq!(
        run_list_tool["inputSchema"]["properties"]["workflow_id"]["type"],
        "string"
    );
    assert_eq!(
        run_list_tool["inputSchema"]["properties"]["status"]["enum"],
        serde_json::json!(["completed", "failed", "unknown"])
    );
    let model_list_tool = tools["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .find(|tool| tool["name"] == "lightflow.model.list")
        .expect("model list tool");
    assert_eq!(
        model_list_tool["inputSchema"]["properties"]["status"]["enum"],
        serde_json::json!(["all", "available", "blocked"])
    );

    let workflow = mcp_tool(
        &service,
        "lightflow.workflow.get",
        serde_json::json!({ "workflow_id": "lightflow.parent" }),
    );
    assert_eq!(workflow["id"], "lightflow.parent");
    assert_eq!(workflow["nodes"][0]["workflow_id"], "lightflow.child");

    let dependencies = mcp_tool(
        &service,
        "lightflow.workflow.dependencies",
        serde_json::json!({ "workflow_id": "lightflow.parent" }),
    );
    assert_eq!(dependencies["complete"], true);

    let plan = mcp_tool(
        &service,
        "lightflow.workflow.plan",
        serde_json::json!({ "workflow_id": "lightflow.parent" }),
    );
    assert_eq!(plan["workflow_id"], "lightflow.parent");
    assert_eq!(plan["nodes"][0]["runtime"]["executor_id"], "passthrough");

    let workflow_resource = mcp_result(
        &service,
        serde_json::json!({
            "id": 35,
            "method": "resources/read",
            "params": { "uri": "lightflow://workflows/lightflow.parent" }
        }),
    );
    let workflow_resource_json = mcp_resource_json(&workflow_resource)?;
    assert_eq!(workflow_resource_json["id"], "lightflow.parent");
    assert_eq!(
        workflow_resource_json["nodes"][0]["workflow_id"],
        "lightflow.child"
    );

    let dependencies_resource = mcp_result(
        &service,
        serde_json::json!({
            "id": 36,
            "method": "resources/read",
            "params": { "uri": "lightflow://workflows/lightflow.parent/dependencies" }
        }),
    );
    let dependencies_resource_json = mcp_resource_json(&dependencies_resource)?;
    assert_eq!(dependencies_resource_json["complete"], true);

    let plan_resource = mcp_result(
        &service,
        serde_json::json!({
            "id": 37,
            "method": "resources/read",
            "params": { "uri": "lightflow://workflows/lightflow.parent/plan" }
        }),
    );
    let plan_resource_json = mcp_resource_json(&plan_resource)?;
    assert_eq!(plan_resource_json["workflow_id"], "lightflow.parent");
    assert_eq!(
        plan_resource_json["nodes"][0]["runtime"]["executor_id"],
        "passthrough"
    );

    let invalid_workflow_resource = mcp_response(
        &service,
        serde_json::json!({
            "id": 42,
            "method": "resources/read",
            "params": { "uri": "lightflow://workflows/lightflow.parent/unknown" }
        }),
    );
    assert_eq!(invalid_workflow_resource["error"]["code"], -32602);
    assert!(
        invalid_workflow_resource["error"]["message"]
            .as_str()
            .expect("invalid workflow resource message")
            .contains("unknown resource: lightflow://workflows/lightflow.parent/unknown"),
        "invalid workflow resource:\n{invalid_workflow_resource:#?}"
    );

    let publish = mcp_tool(
        &service,
        "lightflow.workflow.publish_check",
        serde_json::json!({ "workflow_id": "lightflow.parent" }),
    );
    assert_eq!(publish["workflow_id"], "lightflow.parent");
    assert_eq!(publish["publishable"], false);
    assert!(
        publish["command"]
            .as_array()
            .expect("publish command")
            .iter()
            .any(|part| part == "publish")
    );

    let publish_resource = mcp_result(
        &service,
        serde_json::json!({
            "id": 43,
            "method": "resources/read",
            "params": { "uri": "lightflow://workflows/lightflow.parent/publish" }
        }),
    );
    let publish_resource_json = mcp_resource_json(&publish_resource)?;
    assert_eq!(publish_resource_json["workflow_id"], "lightflow.parent");
    assert_eq!(publish_resource_json["publishable"], false);
    assert!(
        publish_resource_json["command"]
            .as_array()
            .expect("publish resource command")
            .iter()
            .any(|part| part == "publish")
    );

    let publish_list = mcp_tool(
        &service,
        "lightflow.workflow.publish_list",
        serde_json::json!({}),
    );
    assert_eq!(publish_list["publishable"], false);
    let publish_checks = publish_list["checks"].as_array().expect("publish checks");
    let publishable_count = publish_checks
        .iter()
        .filter(|check| check["publishable"] == true)
        .count();
    let blocked_count = publish_checks
        .iter()
        .filter(|check| check["publishable"] == false)
        .count();
    assert_eq!(publish_list["total"], publish_checks.len());
    assert_eq!(publish_list["publishable_count"], publishable_count);
    assert_eq!(publish_list["blocked_count"], blocked_count);
    assert_eq!(
        publish_list["commands"]
            .as_array()
            .expect("publish commands")
            .len(),
        publish_checks.len()
    );
    assert!(
        publish_list["commands"]
            .as_array()
            .expect("publish commands")
            .iter()
            .any(|command| {
                let parts = command.as_array().expect("publish command parts");
                parts.first().and_then(serde_json::Value::as_str) == Some("cargo")
                    && parts.get(1).and_then(serde_json::Value::as_str) == Some("publish")
                    && parts.get(2).and_then(serde_json::Value::as_str) == Some("--manifest-path")
                    && parts
                        .get(3)
                        .and_then(serde_json::Value::as_str)
                        .is_some_and(|path| {
                            path.ends_with(".lightflow/workflows/tests/parent/Cargo.toml")
                        })
                    && parts.get(4).and_then(serde_json::Value::as_str) == Some("--dry-run")
            }),
        "publish list:\n{publish_list:#?}"
    );
    assert!(
        publish_checks
            .iter()
            .any(|check| check["workflow_id"] == "lightflow.parent")
    );

    let execution = mcp_tool(
        &service,
        "lightflow.workflow.run",
        serde_json::json!({
            "workflow_id": "lightflow.parent",
            "inputs": { "in": "hello" },
            "disabled_nodes": ["sink"]
        }),
    );
    assert_eq!(execution["outputs"]["out"], "hello");
    assert_eq!(execution["nodes"][1]["status"], "skipped");
    let run_id = execution["run_id"].as_str().expect("mcp run id");

    let nodes = mcp_tool(&service, "lightflow.node.list", serde_json::json!({}));
    assert!(
        nodes["nodes"]
            .as_array()
            .expect("nodes")
            .iter()
            .any(|node| node["id"] == "lightflow.parent")
    );

    let node = mcp_tool(
        &service,
        "lightflow.node.get",
        serde_json::json!({ "workflow_id": "lightflow.parent" }),
    );
    assert_eq!(node["id"], "lightflow.parent");

    let node_resource = mcp_result(
        &service,
        serde_json::json!({
            "id": 38,
            "method": "resources/read",
            "params": { "uri": "lightflow://nodes/lightflow.parent" }
        }),
    );
    let node_resource_json = mcp_resource_json(&node_resource)?;
    assert_eq!(node_resource_json["id"], "lightflow.parent");
    assert_eq!(node_resource_json["kind"], "composite");
    assert_eq!(node_resource_json["graph"]["nodes"], 2);
    assert_eq!(node_resource_json["validation"]["valid"], true);

    let executors = mcp_tool(&service, "lightflow.executor.list", serde_json::json!({}));
    assert!(
        executors["executors"]
            .as_array()
            .expect("executors")
            .iter()
            .any(|executor| {
                executor["id"] == "passthrough"
                    && executor["status"] == "builtin"
                    && executor["data_policy"] == "json_values"
            })
    );

    let models = mcp_tool(&service, "lightflow.model.list", serde_json::json!({}));
    assert_eq!(models["total"], 0);
    assert_eq!(models["available_count"], 0);
    assert_eq!(models["blocked_count"], 0);
    assert_eq!(models["issues"], serde_json::json!([]));
    assert_eq!(models["models"].as_array().expect("models").len(), 0);
    let filtered_models = mcp_tool(
        &service,
        "lightflow.model.list",
        serde_json::json!({
            "workflow_id": "lightflow.parent",
            "status": "blocked"
        }),
    );
    assert_eq!(filtered_models["total"], 0);
    assert_eq!(
        filtered_models["models"].as_array().expect("models").len(),
        0
    );

    let runs = mcp_tool(&service, "lightflow.run.list", serde_json::json!({}));
    assert_eq!(runs["last"], run_id);
    assert_eq!(runs["total"], 1);
    assert_eq!(runs["completed_count"], 1);
    assert_eq!(runs["failed_count"], 0);
    assert_eq!(runs["unknown_count"], 0);
    let focused_runs = mcp_tool(
        &service,
        "lightflow.run.list",
        serde_json::json!({
            "limit": 1,
            "workflow_id": "lightflow.parent",
            "status": "completed"
        }),
    );
    assert_eq!(focused_runs["total"], 1);
    assert_eq!(focused_runs["runs"][0]["run_id"], run_id);
    assert_eq!(focused_runs["runs"][0]["workflow_id"], "lightflow.parent");
    let empty_runs = mcp_tool(
        &service,
        "lightflow.run.list",
        serde_json::json!({ "workflow_id": "lightflow.missing" }),
    );
    assert_eq!(empty_runs["total"], 0);

    let filtered_runs_resource = mcp_result(
        &service,
        serde_json::json!({
            "id": 27,
            "method": "resources/read",
            "params": {
                "uri": "lightflow://runs?workflow_id=lightflow.parent&status=completed&limit=1"
            }
        }),
    );
    let filtered_runs_resource_json = mcp_resource_json(&filtered_runs_resource)?;
    assert_eq!(filtered_runs_resource_json["total"], 1);
    assert_eq!(filtered_runs_resource_json["runs"][0]["run_id"], run_id);

    let filtered_models_resource = mcp_result(
        &service,
        serde_json::json!({
            "id": 28,
            "method": "resources/read",
            "params": {
                "uri": "lightflow://models?workflow_id=lightflow.parent&status=blocked"
            }
        }),
    );
    let filtered_models_resource_json = mcp_resource_json(&filtered_models_resource)?;
    assert_eq!(filtered_models_resource_json["total"], 0);
    assert_eq!(
        filtered_models_resource_json["models"]
            .as_array()
            .expect("filtered models resource entries")
            .len(),
        0
    );

    let invalid_models_resource = mcp_response(
        &service,
        serde_json::json!({
            "id": 30,
            "method": "resources/read",
            "params": {
                "uri": "lightflow://models?workflow_id=lightflow.parent&status=bogus"
            }
        }),
    );
    assert_eq!(invalid_models_resource["error"]["code"], -32602);
    assert!(
        invalid_models_resource["error"]["message"]
            .as_str()
            .expect("invalid model status message")
            .contains("unsupported model status bogus"),
        "invalid models resource:\n{invalid_models_resource:#?}"
    );

    let invalid_runs_resource = mcp_response(
        &service,
        serde_json::json!({
            "id": 31,
            "method": "resources/read",
            "params": {
                "uri": "lightflow://runs?workflow_id=lightflow.parent&limit=abc"
            }
        }),
    );
    assert_eq!(invalid_runs_resource["error"]["code"], -32602);
    assert!(
        invalid_runs_resource["error"]["message"]
            .as_str()
            .expect("invalid runs limit message")
            .contains("lightflow://runs limit must be a non-negative integer"),
        "invalid runs resource:\n{invalid_runs_resource:#?}"
    );

    let run = mcp_tool(
        &service,
        "lightflow.run.get",
        serde_json::json!({ "run_id": run_id }),
    );
    assert_eq!(run["execution"]["outputs"]["out"], "hello");

    let run_resource = mcp_result(
        &service,
        serde_json::json!({
            "id": 39,
            "method": "resources/read",
            "params": { "uri": format!("lightflow://runs/{run_id}") }
        }),
    );
    let run_resource_json = mcp_resource_json(&run_resource)?;
    assert_eq!(run_resource_json["run_id"], run_id);
    assert_eq!(run_resource_json["execution"]["outputs"]["out"], "hello");

    let events = mcp_tool(
        &service,
        "lightflow.run.events",
        serde_json::json!({ "run_id": run_id }),
    );
    assert_eq!(events["events"][0]["event"], "run_started");
    assert_eq!(events["events"][0]["surface"], "mcp");

    let events_resource = mcp_result(
        &service,
        serde_json::json!({
            "id": 40,
            "method": "resources/read",
            "params": { "uri": format!("lightflow://runs/{run_id}/events") }
        }),
    );
    let events_resource_json = mcp_resource_json(&events_resource)?;
    assert_eq!(events_resource_json["events"][0]["event"], "run_started");
    assert_eq!(events_resource_json["events"][0]["surface"], "mcp");

    let replay = mcp_tool(
        &service,
        "lightflow.run.replay",
        serde_json::json!({ "run_id": run_id }),
    );
    assert_eq!(replay["outputs"]["out"], "hello");
    assert_ne!(replay["run_id"], run_id);
    let replay_events = mcp_tool(
        &service,
        "lightflow.run.events",
        serde_json::json!({ "run_id": replay["run_id"] }),
    );
    assert_eq!(replay_events["events"][0]["surface"], "mcp");

    let artifacts = mcp_tool(&service, "lightflow.artifact.list", serde_json::json!({}));
    assert_eq!(
        artifacts["artifacts"].as_array().expect("artifacts").len(),
        0
    );

    let removed_run = mcp_tool(
        &service,
        "lightflow.run.rm",
        serde_json::json!({ "run_id": run_id }),
    );
    assert_eq!(removed_run["removed"], true);
    assert_eq!(removed_run["run_id"], run_id);

    let saved_patch = mcp_tool(
        &service,
        "lightflow.patch.save",
        serde_json::json!({
            "name": "qa-debug",
            "patch": {
                "nodes": {
                    "nested": {
                        "retry": 2
                    }
                }
            }
        }),
    );
    assert_eq!(saved_patch["saved"], true);
    assert_eq!(saved_patch["name"], "qa-debug");

    let patches = mcp_tool(&service, "lightflow.patch.list", serde_json::json!({}));
    assert_eq!(patches["patches"][0]["name"], "qa-debug");

    let patch = mcp_tool(
        &service,
        "lightflow.patch.get",
        serde_json::json!({ "name": "qa-debug" }),
    );
    assert_eq!(patch["patch"]["nodes"]["nested"]["retry"], 2);

    let patch_resource = mcp_result(
        &service,
        serde_json::json!({
            "id": 41,
            "method": "resources/read",
            "params": { "uri": "lightflow://patches/qa-debug" }
        }),
    );
    let patch_resource_json = mcp_resource_json(&patch_resource)?;
    assert_eq!(patch_resource_json["name"], "qa-debug");
    assert_eq!(patch_resource_json["patch"]["nodes"]["nested"]["retry"], 2);

    let validation = mcp_tool(
        &service,
        "lightflow.patch.validate",
        serde_json::json!({ "patch": { "nodes": { "sink": { "disable": true } } } }),
    );
    assert_eq!(validation["valid"], true);
    assert_eq!(validation["issues"], serde_json::json!([]));
    assert_eq!(validation["patch"]["nodes"]["sink"]["disable"], true);

    let selected_validation = mcp_tool(
        &service,
        "lightflow.patch.validate",
        serde_json::json!({
            "workflow_id": "lightflow.parent",
            "patch": {
                "nodes": {
                    "missing": {
                        "disable": true
                    }
                }
            }
        }),
    );
    assert_eq!(selected_validation["valid"], false);
    assert!(
        selected_validation["issues"]
            .as_array()
            .expect("selected validation issues")
            .iter()
            .any(|issue| issue.as_str().unwrap().contains(
                "patch node missing does not match any node in workflow lightflow.parent"
            ))
    );

    let invalid_validation = mcp_tool(
        &service,
        "lightflow.patch.validate",
        serde_json::json!({
            "patch": {
                "nodes": {
                    "missing": {
                        "replace_with": "lightflow.nope",
                        "retry": 0
                    }
                }
            }
        }),
    );
    assert_eq!(invalid_validation["valid"], false);
    assert!(
        invalid_validation["issues"]
            .as_array()
            .expect("patch validation issues")
            .iter()
            .any(|issue| issue.as_str().unwrap().contains(
                "patch node missing replacement workflow lightflow.nope is not available"
            ))
    );

    let removed_patch = mcp_tool(
        &service,
        "lightflow.patch.rm",
        serde_json::json!({ "name": "qa-debug" }),
    );
    assert_eq!(removed_patch["removed"], true);

    let loop_check = mcp_tool(
        &service,
        "lightflow.loop.check",
        serde_json::json!({ "workflow_id": "lightflow.parent" }),
    );
    assert_eq!(loop_check["valid"], false);
    assert_eq!(loop_check["workflow_id"], "lightflow.parent");
    assert!(
        loop_check["failed"].as_u64().expect("loop failed count") > 0,
        "loop check:\n{loop_check:#?}"
    );
    assert!(
        loop_check["issues"]
            .as_array()
            .expect("loop issues")
            .iter()
            .any(|issue| issue
                .as_str()
                .unwrap_or_default()
                .contains("loop.workflow.agent_skills"))
    );
    assert!(
        loop_check["checks"]
            .as_array()
            .expect("loop checks")
            .iter()
            .any(|check| check["id"] == "loop.selected.plan" && check["status"] == "passed")
    );
    assert!(
        loop_check["checks"]
            .as_array()
            .expect("loop checks")
            .iter()
            .any(|check| check["id"] == "loop.workflow.agent_skills" && check["status"] == "failed")
    );

    let strict_loop_check = mcp_tool(
        &service,
        "lightflow.loop.check",
        serde_json::json!({
            "workflow_id": "lightflow.parent",
            "require_replay": true
        }),
    );
    assert!(
        strict_loop_check["checks"]
            .as_array()
            .expect("strict loop checks")
            .iter()
            .any(|check| {
                check["id"] == "loop.selected.replay.required" && check["status"] == "passed"
            })
    );

    let failed_run = lightflow::cli::mcp::handle_request(
        &service,
        serde_json::json!({
            "jsonrpc": "2.0",
            "id": 42,
            "method": "tools/call",
            "params": {
                "name": "lightflow.workflow.run",
                "arguments": {
                    "workflow_id": "lightflow.parent",
                    "inputs": { "in": "hello" },
                    "patch": { "nodes": { "missing": { "disable": true } } }
                }
            }
        }),
    );
    assert_eq!(failed_run["jsonrpc"], "2.0");
    assert_eq!(failed_run["id"], 42);
    assert!(
        failed_run["error"]["message"]
            .as_str()
            .expect("mcp error")
            .contains("invalid execution options")
    );
    let failed_run_id = failed_run["error"]["data"]["run_id"]
        .as_str()
        .expect("failed run id");
    assert!(failed_run_id.starts_with("run-"));
    assert!(
        failed_run["error"]["data"]["trace_path"]
            .as_str()
            .expect("failed trace path")
            .ends_with("execution.json")
    );
    let failed_trace = mcp_tool(
        &service,
        "lightflow.run.get",
        serde_json::json!({ "run_id": failed_run_id }),
    );
    assert_eq!(failed_trace["manifest"]["status"], "failed");

    let loop_changes = mcp_tool(&service, "lightflow.loop.changes", serde_json::json!({}));
    assert_eq!(loop_changes["valid"], false);
    assert_eq!(loop_changes["blockers"], serde_json::json!([]));
    assert!(
        loop_changes["issues"][0]
            .as_str()
            .expect("loop changes issue")
            .contains("git status failed")
    );
    assert!(
        loop_changes["changed_workflows"]
            .as_array()
            .expect("changed workflows")
            .is_empty()
    );

    let release = mcp_tool(
        &service,
        "lightflow.release.check",
        serde_json::json!({ "workflow_id": "lightflow.parent" }),
    );
    assert_eq!(release["dry_run"], true);
    assert_eq!(release["project_config_valid"], true);
    assert_eq!(release.get("project_config_error"), None);
    assert!(
        release["warnings"]
            .as_array()
            .expect("release warnings")
            .iter()
            .any(|warning| warning
                .as_str()
                .unwrap_or_default()
                .contains("release.review.selected_workflow_loop"))
    );
    assert!(
        release["issues"]
            .as_array()
            .expect("release issues")
            .iter()
            .any(|issue| issue
                .as_str()
                .unwrap_or_default()
                .contains("release.review.workflow_change_skills"))
    );
    let release_review = release["checks"]
        .as_array()
        .expect("release checks")
        .iter()
        .find(|check| check["id"] == "release.review.workflow_change_skills")
        .expect("release review check");
    assert_eq!(release_review["kind"], "review");
    assert_eq!(release_review["status"], "failed");
    assert!(
        release_review["message"]
            .as_str()
            .expect("release review message")
            .contains("source-change safety could not be inspected")
    );
    let selected_release_review = release["checks"]
        .as_array()
        .expect("release checks")
        .iter()
        .find(|check| check["id"] == "release.review.selected_workflow_loop")
        .expect("selected release review check");
    assert_eq!(selected_release_review["kind"], "review");
    assert_eq!(selected_release_review["status"], "warning");
    assert!(
        selected_release_review["message"]
            .as_str()
            .expect("selected release review message")
            .contains("loop.selected.publish")
    );
    assert!(
        release["checks"]
            .as_array()
            .expect("release checks")
            .iter()
            .any(|check| {
                check["id"] == "release.command.selected_workflow_loop"
                    && check["command"]
                        == serde_json::json!([
                            "cargo",
                            "run",
                            "--bin",
                            "lfw",
                            "--",
                            "loop",
                            "check",
                            "lightflow.parent",
                            "--require-replay"
                        ])
            })
    );
    assert!(
        release["checks"]
            .as_array()
            .expect("release checks")
            .iter()
            .any(|check| {
                check["id"] == "release.command.project_workspaces"
                    && check["command"]
                        == serde_json::json!([
                            "cargo", "run", "--bin", "lfw", "--", "loop", "projects"
                        ])
            })
    );
    let unknown_project_release = mcp_tool(
        &service,
        "lightflow.release.check",
        serde_json::json!({ "project": "lightflow-typo" }),
    );
    assert_eq!(unknown_project_release["valid"], false);
    assert_eq!(unknown_project_release["project"], "lightflow-typo");
    assert_eq!(unknown_project_release["project_filter_matched"], false);
    assert_eq!(
        unknown_project_release.get("matched_project_workspace"),
        None
    );
    assert!(
        unknown_project_release["known_project_workspaces"]
            .as_array()
            .expect("known project workspaces")
            .iter()
            .any(|workspace| workspace == "lightflow-std"),
        "unknown project release:\n{unknown_project_release:#?}"
    );
    assert_eq!(
        unknown_project_release["known_project_aliases"]["std"],
        "lightflow-std"
    );
    assert!(
        unknown_project_release["issues"]
            .as_array()
            .expect("unknown project release issues")
            .iter()
            .any(|issue| issue.as_str().is_some_and(|issue| {
                issue.contains("project workspace catalog is unavailable")
                    || issue.contains("project workspace filter matched no workspace")
            })),
        "unknown project release:\n{unknown_project_release:#?}"
    );
    assert!(
        unknown_project_release["checks"]
            .as_array()
            .expect("unknown project release checks")
            .iter()
            .any(|check| {
                check["id"] == "release.review.project_workspaces" && check["status"] == "failed"
            }),
        "unknown project release:\n{unknown_project_release:#?}"
    );

    let resources = mcp_result(
        &service,
        serde_json::json!({ "id": 2, "method": "resources/list" }),
    );
    let uris = resources["resources"]
        .as_array()
        .expect("resources/list returns an array")
        .iter()
        .map(|resource| resource["uri"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(
        uris,
        vec![
            "lightflow://workflows",
            "lightflow://nodes",
            "lightflow://executors",
            "lightflow://models",
            "lightflow://runs",
            "lightflow://artifacts",
            "lightflow://patches",
            "lightflow://publish",
            "lightflow://loop",
            "lightflow://loop/changes",
            "lightflow://loop/projects",
            "lightflow://release",
            "lightflow://openapi",
            "lightflow://mcp"
        ]
    );

    let resource_templates = mcp_result(
        &service,
        serde_json::json!({ "id": 26, "method": "resources/templates/list" }),
    );
    let uri_templates = resource_templates["resourceTemplates"]
        .as_array()
        .expect("resources/templates/list returns an array")
        .iter()
        .map(|resource| resource["uriTemplate"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(
        uri_templates,
        vec![
            "lightflow://workflows/{workflow_id}",
            "lightflow://workflows/{workflow_id}/dependencies",
            "lightflow://workflows/{workflow_id}/plan",
            "lightflow://workflows/{workflow_id}/publish",
            "lightflow://nodes/{workflow_id}",
            "lightflow://models?workflow_id={workflow_id}",
            "lightflow://models?workflow_id={workflow_id}&status={status}",
            "lightflow://runs?workflow_id={workflow_id}&status={status}&limit={limit}",
            "lightflow://runs/{run_id}",
            "lightflow://runs/{run_id}/events",
            "lightflow://artifacts?run_id={run_id}&workflow_id={workflow_id}&kind={kind}&limit={limit}",
            "lightflow://patches/{name}",
            "lightflow://publish?project={project}",
            "lightflow://loop?workflow_id={workflow_id}",
            "lightflow://loop?workflow_id={workflow_id}&require_replay={require_replay}",
            "lightflow://loop/projects?project={project}",
            "lightflow://loop/projects?project={project}&dirty={dirty}",
            "lightflow://release?workflow_id={workflow_id}",
            "lightflow://release?workflow_id={workflow_id}&project={project}"
        ]
    );

    let mcp_resource = mcp_result(
        &service,
        serde_json::json!({
            "id": 24,
            "method": "resources/read",
            "params": { "uri": "lightflow://mcp" }
        }),
    );
    let mcp_json = mcp_resource_json(&mcp_resource)?;
    let tool_names = tools["tools"]
        .as_array()
        .expect("tools")
        .iter()
        .map(|tool| tool["name"].as_str().unwrap_or_default())
        .collect::<Vec<_>>();
    assert_eq!(
        mcp_json["tools"]
            .as_array()
            .expect("mcp resource tools")
            .iter()
            .map(|tool| tool.as_str().unwrap_or_default())
            .collect::<Vec<_>>(),
        tool_names
    );
    assert_eq!(
        mcp_json["resources"]
            .as_array()
            .expect("mcp resource resources")
            .iter()
            .map(|resource| resource.as_str().unwrap_or_default())
            .collect::<Vec<_>>(),
        uris
    );
    assert_eq!(
        mcp_json["resourceTemplates"]
            .as_array()
            .expect("mcp resource templates")
            .iter()
            .map(|template| template.as_str().unwrap_or_default())
            .collect::<Vec<_>>(),
        uri_templates
    );
    assert!(
        mcp_json["tools"]
            .as_array()
            .expect("mcp resource tools")
            .iter()
            .any(|tool| tool == "lightflow.release.check")
    );
    assert!(
        mcp_json["resources"]
            .as_array()
            .expect("mcp resource resources")
            .iter()
            .any(|resource| resource == "lightflow://release")
    );
    assert!(
        !mcp_json["resources"]
            .as_array()
            .expect("mcp resource resources")
            .iter()
            .any(|resource| resource == "lightflow://publish?project={project}")
    );
    assert!(
        mcp_json["methods"]
            .as_array()
            .expect("mcp resource methods")
            .iter()
            .any(|method| method == "resources/templates/list")
    );
    assert!(
        mcp_json["resourceTemplates"]
            .as_array()
            .expect("mcp resource templates")
            .iter()
            .any(|template| {
                template
                    == "lightflow://runs?workflow_id={workflow_id}&status={status}&limit={limit}"
            })
    );
    assert!(
        mcp_json["resourceTemplates"]
            .as_array()
            .expect("mcp resource templates")
            .iter()
            .any(|template| {
                template == "lightflow://artifacts?run_id={run_id}&workflow_id={workflow_id}&kind={kind}&limit={limit}"
            })
    );
    assert!(
        mcp_json["resourceTemplates"]
            .as_array()
            .expect("mcp resource templates")
            .iter()
            .any(|template| template == "lightflow://publish?project={project}")
    );
    assert!(
        mcp_json["resourceTemplates"]
            .as_array()
            .expect("mcp resource templates")
            .iter()
            .any(|template| template == "lightflow://loop/projects?project={project}")
    );
    assert!(
        mcp_json["resourceTemplates"]
            .as_array()
            .expect("mcp resource templates")
            .iter()
            .any(|template| {
                template == "lightflow://release?workflow_id={workflow_id}&project={project}"
            })
    );

    let artifact_run = root.join(".lightflow/runs/run-artifact");
    fs::create_dir_all(&artifact_run)?;
    fs::write(
        artifact_run.join("manifest.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "kind": "workflow_run",
            "run_id": "run-artifact",
            "status": "completed",
            "stage_input_resolution": "resolved",
            "started_at_ms": 1,
            "completed_at_ms": 2,
            "stages": [{
                "workflow_id": "lightflow.fixture",
                "execution": {"inputs": {}, "disabled_nodes": [], "enabled_nodes": []}
            }]
        }))?,
    )?;
    fs::write(
        artifact_run.join("execution.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "workflow_id": "lightflow.fixture",
            "version": "0.1.0",
            "inputs": {},
            "outputs": {},
            "artifacts": [{
                "id": "image",
                "kind": "image",
                "path": "/tmp/image.png",
                "mime_type": "image/png",
                "metadata": {}
            }],
            "nodes": []
        }))?,
    )?;
    fs::write(
        artifact_run.join("events.jsonl"),
        "{\"event\":\"run_started\",\"run_id\":\"run-artifact\",\"at_ms\":1,\"surface\":\"mcp\"}\n",
    )?;
    let other_artifact_run = root.join(".lightflow/runs/run-other-artifact");
    fs::create_dir_all(&other_artifact_run)?;
    fs::write(
        other_artifact_run.join("manifest.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "kind": "workflow_run",
            "run_id": "run-other-artifact",
            "status": "completed",
            "stage_input_resolution": "resolved",
            "started_at_ms": 3,
            "completed_at_ms": 4,
            "stages": [{
                "workflow_id": "lightflow.other",
                "execution": {"inputs": {}, "disabled_nodes": [], "enabled_nodes": []}
            }]
        }))?,
    )?;
    fs::write(
        other_artifact_run.join("execution.json"),
        serde_json::to_vec_pretty(&serde_json::json!({
            "workflow_id": "lightflow.other",
            "version": "0.1.0",
            "inputs": {},
            "outputs": {},
            "artifacts": [{
                "id": "mask",
                "kind": "mask",
                "path": "/tmp/mask.png",
                "mime_type": "image/png",
                "metadata": {}
            }],
            "nodes": []
        }))?,
    )?;
    fs::write(
        other_artifact_run.join("events.jsonl"),
        "{\"event\":\"run_started\",\"run_id\":\"run-other-artifact\",\"at_ms\":3,\"surface\":\"cli\"}\n",
    )?;

    let artifacts = mcp_tool(&service, "lightflow.artifact.list", serde_json::json!({}));
    let artifact_entry = artifacts["artifacts"]
        .as_array()
        .expect("artifact entries")
        .iter()
        .find(|artifact| artifact["run_id"] == "run-artifact")
        .expect("run-artifact entry");
    assert_eq!(artifact_entry["stage_index"], 0);
    assert_eq!(artifact_entry["workflow_id"], "lightflow.fixture");
    assert_eq!(artifact_entry["artifact"]["kind"], "image");

    let filtered_mcp_artifacts = mcp_tool(
        &service,
        "lightflow.artifact.list",
        serde_json::json!({
            "run_id": "run-artifact",
            "workflow_id": "lightflow.fixture",
            "kind": "image",
            "limit": 1
        }),
    );
    assert_eq!(
        filtered_mcp_artifacts["artifacts"]
            .as_array()
            .expect("filtered mcp artifact entries")
            .len(),
        1
    );
    assert_eq!(
        filtered_mcp_artifacts["artifacts"][0]["run_id"],
        "run-artifact"
    );

    let cli_artifacts = lfw(&root, ["artifacts"])?;
    let cli_artifact_entry = cli_artifacts["artifacts"]
        .as_array()
        .expect("cli artifact entries")
        .iter()
        .find(|artifact| artifact["run_id"] == "run-artifact")
        .expect("cli run-artifact entry");
    assert_eq!(cli_artifact_entry["stage_index"], 0);
    assert_eq!(cli_artifact_entry["workflow_id"], "lightflow.fixture");
    assert_eq!(cli_artifact_entry["artifact"]["path"], "/tmp/image.png");

    let filtered_cli_artifacts = lfw(
        &root,
        [
            "artifacts",
            "--run",
            "run-artifact",
            "--workflow",
            "lightflow.fixture",
            "--kind",
            "image",
            "--limit",
            "1",
        ],
    )?;
    assert_eq!(
        filtered_cli_artifacts["artifacts"]
            .as_array()
            .expect("filtered cli artifact entries")
            .len(),
        1
    );
    assert_eq!(
        filtered_cli_artifacts["artifacts"][0]["run_id"],
        "run-artifact"
    );
    assert_eq!(
        filtered_cli_artifacts["artifacts"][0]["artifact"]["kind"],
        "image"
    );

    let missing_kind_artifacts = lfw(&root, ["artifacts", "--kind", "video"])?;
    assert!(
        missing_kind_artifacts["artifacts"]
            .as_array()
            .expect("video artifact entries")
            .is_empty()
    );

    let artifacts_resource = mcp_result(
        &service,
        serde_json::json!({
            "id": 22,
            "method": "resources/read",
            "params": { "uri": "lightflow://artifacts" }
        }),
    );
    let artifacts_resource_json = mcp_resource_json(&artifacts_resource)?;
    assert!(
        artifacts_resource_json["artifacts"]
            .as_array()
            .expect("resource artifact entries")
            .iter()
            .any(|artifact| {
                artifact["run_id"] == "run-artifact" && artifact["stage_index"] == 0
            })
    );

    let filtered_artifacts_resource = mcp_result(
        &service,
        serde_json::json!({
            "id": 29,
            "method": "resources/read",
            "params": {
                "uri": "lightflow://artifacts?run_id=run-artifact&workflow_id=lightflow.fixture&kind=image&limit=1"
            }
        }),
    );
    let filtered_artifacts_resource_json = mcp_resource_json(&filtered_artifacts_resource)?;
    assert_eq!(
        filtered_artifacts_resource_json["artifacts"]
            .as_array()
            .expect("filtered resource artifact entries")
            .len(),
        1
    );
    assert_eq!(
        filtered_artifacts_resource_json["artifacts"][0]["run_id"],
        "run-artifact"
    );
    assert_eq!(
        filtered_artifacts_resource_json["artifacts"][0]["artifact"]["kind"],
        "image"
    );

    let invalid_artifacts_resource = mcp_response(
        &service,
        serde_json::json!({
            "id": 32,
            "method": "resources/read",
            "params": {
                "uri": "lightflow://artifacts?run_id=run-artifact&limit=abc"
            }
        }),
    );
    assert_eq!(invalid_artifacts_resource["error"]["code"], -32602);
    assert!(
        invalid_artifacts_resource["error"]["message"]
            .as_str()
            .expect("invalid artifacts limit message")
            .contains("lightflow://artifacts limit must be a non-negative integer"),
        "invalid artifacts resource:\n{invalid_artifacts_resource:#?}"
    );

    let openapi = mcp_result(
        &service,
        serde_json::json!({
            "id": 3,
            "method": "resources/read",
            "params": { "uri": "lightflow://openapi" }
        }),
    );
    assert_eq!(openapi["contents"][0]["mimeType"], "application/yaml");
    assert!(
        openapi["contents"][0]["text"]
            .as_str()
            .expect("openapi text")
            .starts_with("openapi: 3.1.0")
    );

    let loop_resource = mcp_result(
        &service,
        serde_json::json!({
            "id": 4,
            "method": "resources/read",
            "params": { "uri": "lightflow://loop" }
        }),
    );
    assert_eq!(loop_resource["contents"][0]["mimeType"], "application/json");
    let loop_text = mcp_resource_text(&loop_resource);
    assert!(loop_text.contains("\"valid\""));

    let scoped_loop_resource = mcp_result(
        &service,
        serde_json::json!({
            "id": 33,
            "method": "resources/read",
            "params": {
                "uri": "lightflow://loop?workflow_id=lightflow.parent&require_replay=true"
            }
        }),
    );
    assert_eq!(
        scoped_loop_resource["contents"][0]["uri"],
        "lightflow://loop?workflow_id=lightflow.parent&require_replay=true"
    );
    let scoped_loop_json = mcp_resource_json(&scoped_loop_resource)?;
    assert_eq!(scoped_loop_json["workflow_id"], "lightflow.parent");
    assert!(
        scoped_loop_json["checks"]
            .as_array()
            .expect("scoped loop checks")
            .iter()
            .any(|check| check["id"] == "loop.selected.replay.required"),
        "scoped loop resource:\n{scoped_loop_json:#?}"
    );

    let invalid_loop_resource = mcp_response(
        &service,
        serde_json::json!({
            "id": 34,
            "method": "resources/read",
            "params": { "uri": "lightflow://loop?require_replay=true" }
        }),
    );
    assert_eq!(invalid_loop_resource["error"]["code"], -32602);
    assert!(
        invalid_loop_resource["error"]["message"]
            .as_str()
            .expect("invalid loop resource message")
            .contains("lightflow://loop require_replay requires workflow_id"),
        "invalid loop resource:\n{invalid_loop_resource:#?}"
    );

    let loop_changes_resource = mcp_result(
        &service,
        serde_json::json!({
            "id": 6,
            "method": "resources/read",
            "params": { "uri": "lightflow://loop/changes" }
        }),
    );
    assert_eq!(
        loop_changes_resource["contents"][0]["mimeType"],
        "application/json"
    );
    let loop_changes_text = mcp_resource_text(&loop_changes_resource);
    assert!(loop_changes_text.contains("\"changed_workflows\""));
    assert!(loop_changes_text.contains("\"blockers\""));

    let loop_projects_resource = mcp_result(
        &service,
        serde_json::json!({
            "id": 16,
            "method": "resources/read",
            "params": { "uri": "lightflow://loop/projects" }
        }),
    );
    assert_eq!(
        loop_projects_resource["contents"][0]["mimeType"],
        "application/json"
    );
    let loop_projects_text = mcp_resource_text(&loop_projects_resource);
    assert!(loop_projects_text.contains("\"workspaces\""));
    assert!(loop_projects_text.contains("\"linked_count\""));
    assert!(loop_projects_text.contains("\"project_config_path\""));
    assert!(loop_projects_text.contains("\"project_config_valid\""));
    assert!(loop_projects_text.contains("\"known_optional_workspace_names\""));
    assert!(loop_projects_text.contains("\"project_submodule_update_command\""));

    let scoped_loop_projects_resource = mcp_result(
        &service,
        serde_json::json!({
            "id": 17,
            "method": "resources/read",
            "params": { "uri": "lightflow://loop/projects?project=std" }
        }),
    );
    assert_eq!(
        scoped_loop_projects_resource["contents"][0]["mimeType"],
        "application/json"
    );
    assert_eq!(
        scoped_loop_projects_resource["contents"][0]["uri"],
        "lightflow://loop/projects?project=std"
    );
    let scoped_loop_projects = mcp_resource_json(&scoped_loop_projects_resource)?;
    assert_eq!(scoped_loop_projects["project_filter"], "std");
    assert_eq!(scoped_loop_projects["project_filter_matched"], true);
    assert_eq!(
        scoped_loop_projects["matched_project_workspace"],
        "lightflow-std"
    );
    assert_eq!(scoped_loop_projects["present_count"], 1);
    assert_eq!(
        scoped_loop_projects["workspaces"][0]["label"],
        "projects/lightflow-std"
    );

    let publish_resource = mcp_result(
        &service,
        serde_json::json!({
            "id": 5,
            "method": "resources/read",
            "params": { "uri": "lightflow://publish" }
        }),
    );
    assert_eq!(
        publish_resource["contents"][0]["mimeType"],
        "application/json"
    );
    let publish_text = mcp_resource_text(&publish_resource);
    assert!(publish_text.contains("\"checks\""));

    let scoped_publish_resource = mcp_result(
        &service,
        serde_json::json!({
            "id": 25,
            "method": "resources/read",
            "params": { "uri": "lightflow://publish?project=std" }
        }),
    );
    assert_eq!(
        scoped_publish_resource["contents"][0]["mimeType"],
        "application/json"
    );
    assert_eq!(
        scoped_publish_resource["contents"][0]["uri"],
        "lightflow://publish?project=std"
    );
    let scoped_publish = mcp_resource_json(&scoped_publish_resource)?;
    assert_eq!(scoped_publish["project"], "std");
    assert_eq!(scoped_publish["matched_project_workspace"], "lightflow-std");
    assert_eq!(scoped_publish["total"], 1);
    assert_eq!(
        scoped_publish["checks"][0]["workspace"],
        "projects/lightflow-std"
    );
    assert_eq!(
        scoped_publish["checks"][0]["workflow_id"],
        "lightflow.std_project"
    );

    let release_resource = mcp_result(
        &service,
        serde_json::json!({
            "id": 23,
            "method": "resources/read",
            "params": { "uri": "lightflow://release" }
        }),
    );
    assert_eq!(
        release_resource["contents"][0]["mimeType"],
        "application/json"
    );
    let release_text = mcp_resource_text(&release_resource);
    let release_resource_json = mcp_resource_json(&release_resource)?;
    assert_eq!(release_resource_json["dry_run"], true);
    assert_eq!(release_resource_json["project_config_valid"], true);
    assert_eq!(release_resource_json.get("project_config_error"), None);
    assert!(release_resource_json["project_submodule_update_command"].is_array());
    assert!(release_text.contains("\"project_config_path\""));
    assert!(release_text.contains("\"project_config_valid\""));
    assert!(release_text.contains("\"known_optional_workspace_names\""));
    let release_resource_review = release_resource_json["checks"]
        .as_array()
        .expect("release resource checks")
        .iter()
        .find(|check| check["id"] == "release.review.workflow_change_skills")
        .expect("release resource review check");
    assert_eq!(release_resource_review["kind"], "review");
    assert_eq!(release_resource_review["status"], "failed");
    assert!(
        release_resource_review["message"]
            .as_str()
            .expect("release resource review message")
            .contains("source-change safety could not be inspected")
    );

    let scoped_release_resource = mcp_result(
        &service,
        serde_json::json!({
            "id": 27,
            "method": "resources/read",
            "params": { "uri": "lightflow://release?workflow_id=lightflow.std_project&project=std" }
        }),
    );
    assert_eq!(
        scoped_release_resource["contents"][0]["mimeType"],
        "application/json"
    );
    assert_eq!(
        scoped_release_resource["contents"][0]["uri"],
        "lightflow://release?workflow_id=lightflow.std_project&project=std"
    );
    let scoped_release = mcp_resource_json(&scoped_release_resource)?;
    assert_eq!(scoped_release["workflow_id"], "lightflow.std_project");
    assert_eq!(scoped_release["project"], "std");
    assert_eq!(scoped_release["project_filter_matched"], true);
    assert_eq!(scoped_release["matched_project_workspace"], "lightflow-std");
    assert!(
        scoped_release["checks"]
            .as_array()
            .expect("scoped release checks")
            .iter()
            .any(|check| {
                check["id"] == "release.command.workflow_publish_ready"
                    && check["command"]
                        == serde_json::json!([
                            "cargo",
                            "run",
                            "--bin",
                            "lfw",
                            "--",
                            "publish",
                            "--workflows",
                            "--require-publishable",
                            "--project",
                            "std"
                        ])
            }),
        "scoped release resource:\n{scoped_release:#?}"
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn lfw_models_requirements_lists_workflow_model_catalog() -> Result<(), Box<dyn std::error::Error>>
{
    let root = unique_temp_root();
    write_project_specs(&root)?;
    write_workflow_crate(
        &root,
        "lightflow.modelled",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.modelled")
        .version("0.1.0")
        .name("Modelled")
        .input("prompt", "text")
        .output("image", "artifact")
        .model("image_model", "text-to-image")
        .input_model_requirement("prompt", "image_model")
        .build()
}
"#,
    )?;

    let catalog = lfw(&root, ["models", "requirements"])?;
    assert_eq!(catalog["total"], 1);
    assert_eq!(catalog["available_count"], 0);
    assert_eq!(catalog["blocked_count"], 1);
    assert_eq!(
        catalog["issues"][0],
        "lightflow.modelled::image_model: model lock is missing_lock"
    );
    let model = &catalog["models"][0];
    assert_eq!(model["workflow_id"], "lightflow.modelled");
    assert_eq!(model["requirement"]["id"], "image_model");
    assert_eq!(model["lock"]["status"], "missing_lock");
    assert_eq!(
        model["sync_command"],
        serde_json::json!([
            "lfw",
            "sync",
            "lightflow.modelled",
            "--auto-model",
            "--apply"
        ])
    );
    assert_eq!(
        model["verify_command"],
        serde_json::json!(["lfw", "sync", "lightflow.modelled", "--locked", "--apply"])
    );

    let alias = lfw(&root, ["models", "reqs"])?;
    assert_eq!(alias["models"][0]["workflow_id"], "lightflow.modelled");

    let filtered = lfw(&root, ["models", "requirements", "lightflow.modelled"])?;
    assert_eq!(filtered["total"], 1);
    assert_eq!(filtered["models"][0]["workflow_id"], "lightflow.modelled");
    assert_eq!(
        filtered["issues"][0],
        "lightflow.modelled::image_model: model lock is missing_lock"
    );

    let blocked = lfw(&root, ["models", "requirements", "--blocked"])?;
    assert_eq!(blocked["total"], 1);
    assert_eq!(blocked["blocked_count"], 1);
    assert_eq!(blocked["models"][0]["workflow_id"], "lightflow.modelled");

    let filtered_blocked = lfw(
        &root,
        [
            "models",
            "requirements",
            "lightflow.modelled",
            "--status",
            "blocked",
        ],
    )?;
    assert_eq!(filtered_blocked["total"], 1);
    assert_eq!(filtered_blocked["blocked_count"], 1);

    let available = lfw(&root, ["models", "requirements", "--available"])?;
    assert_eq!(available["total"], 0);
    assert_eq!(available["available_count"], 0);
    assert_eq!(available["blocked_count"], 0);
    assert!(
        available["issues"]
            .as_array()
            .expect("available issues")
            .is_empty()
    );

    let filtered_flag = lfw(
        &root,
        ["models", "requirements", "--workflow", "lightflow.parent"],
    )?;
    assert_eq!(filtered_flag["total"], 0);
    assert_eq!(filtered_flag["blocked_count"], 0);
    assert!(
        filtered_flag["issues"]
            .as_array()
            .expect("filtered issues")
            .is_empty()
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn workflow_versions_use_exact_semver_requirements() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    write_project_specs(&root)?;

    write_workflow_crate(
        &root,
        "lightflow.parent",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.parent")
        .version("0.1.0")
        .name("Parent")
        .input("in", "json")
        .output("out", "json")
        .depends_on("lightflow.child", "9.9.9")
        .node("nested", "lightflow.child")
        .build()
}
"#,
    )?;

    let deps = lfw(&root, ["deps", "lightflow.parent"])?;
    assert_eq!(deps["complete"], false);
    assert_eq!(
        deps["version_mismatches"][0]["workflow_id"],
        "lightflow.child"
    );
    assert_eq!(deps["version_mismatches"][0]["required"], "9.9.9");
    assert_eq!(deps["version_mismatches"][0]["found"], "0.1.0");
    assert_eq!(
        deps["version_mismatches"][0]["required_by"],
        "lightflow.parent"
    );

    let service = ApiService::new(&root);
    let execution_error = service
        .execute_workflow("lightflow.parent", Default::default())
        .expect_err("execution should reject dependency version mismatches")
        .to_string();
    assert!(execution_error.contains("lightflow.child requires version 9.9.9"));

    let validation = lightflow(
        &root,
        [
            "workflows",
            "validate",
            r#"{
              "id": "lightflow.invalid_version",
              "version": "not-semver",
              "name": "Invalid Version",
              "nodes": [],
              "edges": []
            }"#,
        ],
    )?;
    assert_eq!(validation["valid"], false);
    assert!(
        validation["issues"][0]
            .as_str()
            .unwrap()
            .contains("must be semantic version")
    );

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn execution_rejects_recursive_workflow_dependency_cycles() -> Result<(), Box<dyn std::error::Error>>
{
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;

    write_workflow_crate(
        &root,
        "lightflow.a",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.a")
        .version("0.1.0")
        .name("A")
        .input("in", "json")
        .output("out", "json")
        .node("b", "lightflow.b")
        .build()
}
"#,
    )?;
    write_workflow_crate(
        &root,
        "lightflow.b",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow("lightflow.b")
        .version("0.1.0")
        .name("B")
        .input("in", "json")
        .output("out", "json")
        .node("a", "lightflow.a")
        .build()
}
"#,
    )?;

    let service = ApiService::new(&root);
    let execution_error = service
        .execute_workflow("lightflow.a", Default::default())
        .expect_err("execution should reject recursive workflow cycles")
        .to_string();
    assert!(execution_error.contains("workflow dependency cycle"));

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn runs_rm_rejects_run_id_path_traversal() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    let outside = root.join("outside-run-dir");
    fs::create_dir_all(&outside)?;

    let output = lfw_command(&root)
        .args(["runs", "rm", "../../outside-run-dir"])
        .output()?;

    assert!(!output.status.success());
    assert!(outside.exists());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid run id path segment"));

    let _ = fs::remove_dir_all(root);
    Ok(())
}

fn complete_generated_workflow_metadata(
    root: &Path,
    category: &str,
    name: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let path = root
        .join(".lightflow/workflows")
        .join(category)
        .join(name)
        .join("src/lib.rs");
    let source = fs::read_to_string(&path)?
        .replace(
            "TODO: describe this workflow.",
            "Release-ready test workflow.",
        )
        .replace(
            "TODO: describe the input value.",
            "Input value for release readiness.",
        )
        .replace(
            "TODO: describe the output value.",
            "Output value for release readiness.",
        )
        .replace(
            "TODO: describe the runtime input value.",
            "Runtime input value for release readiness.",
        )
        .replace(
            "TODO: describe the runtime output value.",
            "Runtime output value for release readiness.",
        );
    fs::write(path, source)?;
    Ok(())
}
