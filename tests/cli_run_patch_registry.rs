mod support;

use serde_json::Value;
use std::fs;
use support::*;

#[test]
fn lfw_run_applies_patch_files_at_node_boundaries() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    write_project_specs(&root)?;
    write_workflow_crate(
        &root,
        "lightflow.replacement",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
        .name("Replacement")
        .input("in", "json")
        .output("out", "json")
        .build()
}
"#,
    )?;
    write_workflow_crate(
        &root,
        "lightflow.fallback",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
        .name("Fallback")
        .input("in", "json")
        .output("out", "json")
        .build()
}
"#,
    )?;
    write_workflow_crate(
        &root,
        "lightflow.no_output",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
        .name("No Output")
        .input("in", "json")
        .build()
}
"#,
    )?;
    write_workflow_crate(
        &root,
        "lightflow.extra_required",
        r#"use lightflow::preload::*;

pub fn define() -> WorkflowSpec {
    workflow!()
        .name("Extra Required")
        .input("in", "json")
        .input("extra", "json")
        .input_required("extra", true)
        .output("out", "json")
        .build()
}
"#,
    )?;

    let patch_path = root.join("patch.json");
    fs::write(
        &patch_path,
        r#"{
  "nodes": {
    "nested": {
      "replace_with": "lightflow.replacement",
      "retry": 2,
      "timeout_ms": 1000
    }
  }
}
"#,
    )?;
    let saved_patch = lfw(
        &root,
        [
            "patch",
            "save",
            "qa-debug",
            &format!("@{}", patch_path.display()),
        ],
    )?;
    assert_eq!(saved_patch["saved"], true);
    assert_eq!(saved_patch["name"], "qa-debug");
    assert!(root.join(".lightflow/patches/qa-debug.json").exists());

    let patches = lfw(&root, ["patch", "list"])?;
    assert_eq!(patches["patches"][0]["name"], "qa-debug");

    let registered_patch = lfw(&root, ["patch", "get", "qa-debug"])?;
    assert_eq!(
        registered_patch["patch"]["nodes"]["nested"]["replace_with"],
        "lightflow.replacement"
    );
    let validated_patch = lfw(&root, ["patch", "validate", "qa-debug"])?;
    assert_eq!(validated_patch["valid"], true);
    assert_eq!(validated_patch["issues"], serde_json::json!([]));
    assert_eq!(
        validated_patch["patch"]["nodes"]["nested"]["replace_with"],
        "lightflow.replacement"
    );
    let selected_validation = lfw(
        &root,
        [
            "patch",
            "validate",
            "qa-debug",
            "--workflow",
            "lightflow.parent",
        ],
    )?;
    assert_eq!(selected_validation["valid"], true);

    lfw(
        &root,
        [
            "patch",
            "save",
            "bad-debug",
            r#"{"nodes":{"missing":{"replace_with":"lightflow.nope","retry":0}}}"#,
        ],
    )?;
    let invalid_patch = lfw_command(&root)
        .args(["patch", "validate", "bad-debug"])
        .output()?;
    assert!(!invalid_patch.status.success());
    let invalid_stderr = String::from_utf8_lossy(&invalid_patch.stderr);
    assert!(
        invalid_stderr.contains("patch node missing does not match any available workflow node"),
        "stderr:\n{invalid_stderr}"
    );
    assert!(
        invalid_stderr
            .contains("patch node missing replacement workflow lightflow.nope is not available"),
        "stderr:\n{invalid_stderr}"
    );
    assert!(
        invalid_stderr.contains("patch node missing retry must be greater than zero"),
        "stderr:\n{invalid_stderr}"
    );
    let bad_loop = lfw_command(&root).args(["loop", "check"]).output()?;
    assert!(!bad_loop.status.success());
    let bad_loop_stderr = String::from_utf8_lossy(&bad_loop.stderr);
    assert!(
        bad_loop_stderr.contains("saved patches are invalid: bad-debug"),
        "stderr:\n{bad_loop_stderr}"
    );
    assert!(
        bad_loop_stderr.contains("patch node missing does not match any available workflow node"),
        "stderr:\n{bad_loop_stderr}"
    );
    lfw(&root, ["patch", "rm", "bad-debug"])?;

    let wrong_workflow_patch = lfw_command(&root)
        .args([
            "run",
            "lightflow.child",
            "--input",
            "in=hello",
            "--patch",
            r#"{"nodes":{"nested":{"disable":true}}}"#,
        ])
        .output()?;
    assert!(!wrong_workflow_patch.status.success());
    let wrong_workflow_stderr = String::from_utf8_lossy(&wrong_workflow_patch.stderr);
    assert!(
        wrong_workflow_stderr
            .contains("patch node nested does not match any node in workflow lightflow.child"),
        "stderr:\n{wrong_workflow_stderr}"
    );

    let unknown_patch_node = lfw_command(&root)
        .args([
            "run",
            "lightflow.parent",
            "--input",
            "in=hello",
            "--patch",
            r#"{"nodes":{"missing":{"disable":true}}}"#,
        ])
        .output()?;
    assert!(!unknown_patch_node.status.success());
    let unknown_patch_stderr = String::from_utf8_lossy(&unknown_patch_node.stderr);
    assert!(
        unknown_patch_stderr
            .contains("patch node missing does not match any node in workflow lightflow.parent"),
        "stderr:\n{unknown_patch_stderr}"
    );

    let unknown_toggle = lfw_command(&root)
        .args([
            "run",
            "lightflow.parent",
            "--input",
            "in=hello",
            "--disable",
            "missing",
        ])
        .output()?;
    assert!(!unknown_toggle.status.success());
    let unknown_toggle_stderr = String::from_utf8_lossy(&unknown_toggle.stderr);
    assert!(
        unknown_toggle_stderr
            .contains("disabled node missing does not match any node in workflow lightflow.parent"),
        "stderr:\n{unknown_toggle_stderr}"
    );

    let incompatible_replacement = lfw_command(&root)
        .args([
            "run",
            "lightflow.parent",
            "--input",
            "in=hello",
            "--patch",
            r#"{"nodes":{"nested":{"replace_with":"lightflow.no_output"}}}"#,
        ])
        .output()?;
    assert!(!incompatible_replacement.status.success());
    let incompatible_replacement_stderr = String::from_utf8_lossy(&incompatible_replacement.stderr);
    assert!(
        incompatible_replacement_stderr.contains(
            "patch node nested replacement workflow lightflow.no_output is missing output port out"
        ),
        "stderr:\n{incompatible_replacement_stderr}"
    );

    let incompatible_preflight = lfw_command(&root)
        .args([
            "patch",
            "validate",
            r#"{"nodes":{"nested":{"replace_with":"lightflow.no_output"}}}"#,
            "--workflow",
            "lightflow.parent",
        ])
        .output()?;
    assert!(!incompatible_preflight.status.success());
    let incompatible_preflight_stderr = String::from_utf8_lossy(&incompatible_preflight.stderr);
    assert!(
        incompatible_preflight_stderr.contains(
            "patch node nested replacement workflow lightflow.no_output is missing output port out"
        ),
        "stderr:\n{incompatible_preflight_stderr}"
    );

    lfw(
        &root,
        [
            "patch",
            "save",
            "wrong-shape",
            r#"{"nodes":{"nested":{"replace_with":"lightflow.no_output"}}}"#,
        ],
    )?;
    let selected_loop_output = lfw_command(&root)
        .args(["loop", "check", "lightflow.parent"])
        .output()?;
    assert!(!selected_loop_output.status.success());
    let selected_loop = serde_json::from_slice::<serde_json::Value>(&selected_loop_output.stderr)?;
    assert!(
        selected_loop["checks"]
            .as_array()
            .expect("selected loop checks")
            .iter()
            .any(|check| {
                check["id"] == "loop.selected.patches"
                    && check["status"] == "warning"
                    && check["message"].as_str().unwrap().contains("wrong-shape")
                    && check["message"]
                        .as_str()
                        .unwrap()
                        .contains("missing output port out")
            }),
        "selected loop checks:\n{selected_loop}"
    );
    lfw(&root, ["patch", "rm", "wrong-shape"])?;

    let unsatisfied_extra_input = lfw_command(&root)
        .args([
            "run",
            "lightflow.parent",
            "--input",
            "in=hello",
            "--patch",
            r#"{"nodes":{"nested":{"replace_with":"lightflow.extra_required"}}}"#,
        ])
        .output()?;
    assert!(!unsatisfied_extra_input.status.success());
    let unsatisfied_extra_input_stderr = String::from_utf8_lossy(&unsatisfied_extra_input.stderr);
    assert!(
        unsatisfied_extra_input_stderr.contains(
            "patch node nested replacement workflow lightflow.extra_required has unsatisfied required input port extra"
        ),
        "stderr:\n{unsatisfied_extra_input_stderr}"
    );

    let patched = lfw(
        &root,
        [
            "run",
            "lightflow.parent",
            "--input",
            "in=hello",
            "--patch",
            "qa-debug",
        ],
    )?;
    assert_eq!(patched["outputs"]["out"], "hello");
    assert_eq!(patched["nodes"][0]["node_id"], "nested");
    assert_eq!(patched["nodes"][0]["workflow_id"], "lightflow.child");
    assert_eq!(
        patched["nodes"][0]["selected_workflow_id"],
        "lightflow.replacement"
    );
    assert_eq!(patched["nodes"][0]["attempts"], 1);

    let trace = lfw(&root, ["trace", patched["run_id"].as_str().unwrap()])?;
    assert_eq!(
        trace["manifest"]["stages"][0]["execution"]["patch"]["nodes"]["nested"]["replace_with"],
        "lightflow.replacement"
    );
    assert_eq!(
        trace["manifest"]["stages"][0]["execution"]["patch"]["nodes"]["nested"]["retry"],
        2
    );
    assert_eq!(trace["events"][1]["event"], "node_completed");
    assert_eq!(trace["events"][1]["node_id"], "nested");
    assert_eq!(
        trace["events"][1]["selected_workflow_id"],
        "lightflow.replacement"
    );

    let fallback_patch = r#"{
  "nodes": {
    "nested": {
      "disable": true,
      "fallback_workflow_id": "lightflow.fallback"
    }
  }
}"#;
    let fallback = lfw(
        &root,
        [
            "run",
            "lightflow.parent",
            "--input",
            "in=hello",
            "--patch",
            fallback_patch,
        ],
    )?;
    assert_eq!(fallback["nodes"][0]["status"], "completed");
    assert_eq!(
        fallback["nodes"][0]["selected_workflow_id"],
        "lightflow.fallback"
    );
    assert_eq!(fallback["outputs"]["out"], "hello");

    let disabled = lfw(
        &root,
        [
            "run",
            "lightflow.parent",
            "--input",
            "in=hello",
            "--patch",
            r#"{"nodes":{"nested":{"disable":true}}}"#,
        ],
    )?;
    assert_eq!(disabled["nodes"][0]["status"], "skipped");
    assert!(disabled["nodes"][0]["duration_ms"].is_number());
    assert_eq!(disabled["nodes"][0]["attempts"], 0);
    assert_eq!(disabled["outputs"]["out"], Value::Null);

    let disabled_trace = lfw(&root, ["trace", disabled["run_id"].as_str().unwrap()])?;
    assert_eq!(disabled_trace["events"][1]["event"], "node_skipped");
    assert_eq!(disabled_trace["events"][1]["node_id"], "nested");

    let enabled = lfw(
        &root,
        [
            "run",
            "lightflow.parent",
            "--input",
            "in=hello",
            "--disable",
            "nested",
            "--patch",
            r#"{"nodes":{"nested":{"enable":true}}}"#,
        ],
    )?;
    assert_eq!(enabled["nodes"][0]["status"], "completed");
    assert_eq!(enabled["outputs"]["out"], "hello");

    let removed_patch = lfw(&root, ["patch", "rm", "qa-debug"])?;
    assert_eq!(removed_patch["removed"], true);
    assert!(!root.join(".lightflow/patches/qa-debug.json").exists());

    let _ = fs::remove_dir_all(root);
    Ok(())
}

#[test]
fn patch_registry_rejects_path_traversal_names() -> Result<(), Box<dyn std::error::Error>> {
    let root = unique_temp_root();
    fs::create_dir_all(&root)?;
    let patch = r#"{"nodes":{}}"#;

    for args in [
        vec!["patch", "get", "../outside"],
        vec!["patch", "save", "../outside", patch],
        vec!["patch", "validate", "../outside"],
        vec!["patch", "rm", "../outside"],
    ] {
        let output = lfw_command(&root).args(args).output()?;
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains("patch name must be a single non-empty file name"),
            "stderr:\n{}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    assert!(!root.join(".lightflow/outside.json").exists());
    assert!(!root.join("outside.json").exists());

    let _ = fs::remove_dir_all(root);
    Ok(())
}
