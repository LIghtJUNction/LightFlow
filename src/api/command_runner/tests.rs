use super::*;
use crate::api::plan::RUNNER_ENGINE;
use crate::workflow::{PortSpec, WorkflowArtifact, workflow_with_identity};
use std::fs;

fn workflow() -> WorkflowSpec {
    workflow_with_identity("lightflow.test_command", "0.1.0")
        .output("result", "text")
        .build()
}

fn response(outputs: Map<String, Value>) -> CommandResponse {
    CommandResponse {
        outputs,
        artifacts: Vec::new(),
        replay_fingerprint: json!({"runner": "test", "version": 1}),
    }
}

fn validate_legacy_response(
    root: &Path,
    workflow: &WorkflowSpec,
    response: CommandResponse,
) -> ApiResult<CommandExecution> {
    validate_response(
        root,
        workflow,
        response,
        COMMAND_ENGINE,
        ResponsePolicy::LegacyCommand,
    )
}

#[test]
fn timeout_parser_uses_bounded_positive_milliseconds() {
    assert_eq!(
        command_timeout(Some("25")).unwrap(),
        Duration::from_millis(25)
    );
    assert_eq!(
        command_timeout(None).unwrap(),
        Duration::from_millis(DEFAULT_TIMEOUT_MS)
    );
    assert!(
        command_timeout(Some("0"))
            .unwrap_err()
            .contains("must be from")
    );
    assert!(
        command_timeout(Some("invalid"))
            .unwrap_err()
            .contains("must be an integer")
    );
    assert!(
        command_timeout(Some("86400001"))
            .unwrap_err()
            .contains("must be from")
    );
}

#[test]
fn response_outputs_must_match_declared_ports() {
    let root = tempfile::tempdir().expect("tempdir");
    let missing =
        validate_legacy_response(root.path(), &workflow(), response(Map::new())).unwrap_err();
    assert!(missing.to_string().contains("missing [result]"));

    let mut outputs = Map::new();
    outputs.insert("result".to_owned(), "ok".into());
    outputs.insert("extra".to_owned(), true.into());
    let unknown =
        validate_legacy_response(root.path(), &workflow(), response(outputs)).unwrap_err();
    assert!(unknown.to_string().contains("unknown [extra]"));
}

#[test]
fn response_rejects_missing_and_duplicate_artifacts() {
    let root = tempfile::tempdir().expect("tempdir");
    let artifact = WorkflowArtifact {
        id: "video".to_owned(),
        kind: "video".to_owned(),
        path: "missing.mp4".to_owned(),
        mime_type: "video/mp4".to_owned(),
        metadata: Map::new(),
    };
    let error = validate_artifacts(
        root.path(),
        std::slice::from_ref(&artifact),
        ResponsePolicy::LegacyCommand,
    )
    .unwrap_err();
    assert!(error.to_string().contains("does not name an existing file"));

    fs::write(root.path().join("video.mp4"), b"video").expect("artifact");
    let existing = WorkflowArtifact {
        path: "video.mp4".to_owned(),
        ..artifact
    };
    let error = validate_artifacts(
        root.path(),
        &[existing.clone(), existing],
        ResponsePolicy::LegacyCommand,
    )
    .unwrap_err();
    assert!(error.to_string().contains("duplicate artifact id video"));
}

#[test]
fn response_outputs_must_match_declared_types() {
    let root = tempfile::tempdir().expect("tempdir");
    let mut outputs = Map::new();
    outputs.insert("result".to_owned(), true.into());
    let error = validate_legacy_response(root.path(), &workflow(), response(outputs)).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("output result must match declared type text, got boolean")
    );
}

#[test]
fn response_requires_object_replay_fingerprint() {
    let root = tempfile::tempdir().expect("tempdir");
    let mut outputs = Map::new();
    outputs.insert("result".to_owned(), "ok".into());
    let response = CommandResponse {
        outputs,
        artifacts: Vec::new(),
        replay_fingerprint: Value::Null,
    };
    let error = validate_legacy_response(root.path(), &workflow(), response).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("replay_fingerprint must be a JSON object")
    );
}

#[test]
fn package_response_requires_non_empty_implementation_identity() {
    let root = tempfile::tempdir().expect("tempdir");
    let mut outputs = Map::new();
    outputs.insert("result".to_owned(), "ok".into());
    for fingerprint in [json!({}), json!({"implementation": "  "})] {
        let response = CommandResponse {
            outputs: outputs.clone(),
            artifacts: Vec::new(),
            replay_fingerprint: fingerprint,
        };
        let error = validate_response(
            root.path(),
            &workflow(),
            response,
            RUNNER_ENGINE,
            ResponsePolicy::Runner,
        )
        .expect_err("missing implementation identity");
        assert!(
            error
                .to_string()
                .contains("non-empty implementation identity")
        );
    }
}

#[test]
fn package_artifacts_reject_absolute_and_parent_paths() {
    let root = tempfile::tempdir().expect("tempdir");
    fs::write(root.path().join("artifact.txt"), b"artifact").expect("artifact");
    let artifact = WorkflowArtifact {
        id: "artifact".to_owned(),
        kind: "text".to_owned(),
        path: root.path().join("artifact.txt").display().to_string(),
        mime_type: "text/plain".to_owned(),
        metadata: Map::new(),
    };
    let error = validate_artifacts(
        root.path(),
        std::slice::from_ref(&artifact),
        ResponsePolicy::Runner,
    )
    .expect_err("absolute path");
    assert!(error.to_string().contains("must be relative"));

    let artifact = WorkflowArtifact {
        path: "../artifact.txt".to_owned(),
        ..artifact
    };
    let error = validate_artifacts(root.path(), &[artifact], ResponsePolicy::Runner)
        .expect_err("parent path");
    assert!(error.to_string().contains("cannot contain `..`"));
}

#[cfg(unix)]
#[test]
fn package_artifacts_reject_symlink_escape() {
    use std::os::unix::fs::symlink;

    let root = tempfile::tempdir().expect("root");
    let outside = tempfile::tempdir().expect("outside");
    fs::write(outside.path().join("artifact.txt"), b"artifact").expect("artifact");
    symlink(
        outside.path().join("artifact.txt"),
        root.path().join("artifact.txt"),
    )
    .expect("symlink");
    let artifact = WorkflowArtifact {
        id: "artifact".to_owned(),
        kind: "text".to_owned(),
        path: "artifact.txt".to_owned(),
        mime_type: "text/plain".to_owned(),
        metadata: Map::new(),
    };

    let error = validate_artifacts(root.path(), &[artifact], ResponsePolicy::Runner)
        .expect_err("symlink escape");
    assert!(error.to_string().contains("escapes project root"));
}

#[test]
fn capped_reader_rejects_oversized_output() {
    let error = read_capped(&b"12345"[..], 4).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}

#[cfg(unix)]
#[test]
fn oversized_output_fails_fast_instead_of_stalling_until_timeout() {
    let root = tempfile::tempdir().expect("tempdir");
    // Write far past the stderr cap plus pipe capacity; without draining,
    // the child blocks on the full pipe until the timeout kill.
    let runner = executable_script(
        root.path(),
        r#"cat >/dev/null
head -c 524288 /dev/zero | tr '\0' 'e' >&2
printf '%s\n' '{"outputs":{"result":"ok"},"artifacts":[],"replay_fingerprint":{"runner":"fixture","version":1}}'"#,
    );
    let started = Instant::now();
    let error = execute_with_runner(
        root.path(),
        &workflow(),
        &Map::new(),
        &runner,
        Duration::from_secs(5),
    )
    .unwrap_err();

    assert!(
        error.to_string().contains("exceeds"),
        "expected output-limit error, got: {error}"
    );
    assert!(
        started.elapsed() < Duration::from_secs(4),
        "oversized output must fail before the timeout"
    );
}

#[cfg(unix)]
fn executable_script(root: &Path, body: &str) -> PathBuf {
    use std::os::unix::fs::PermissionsExt;

    let path = root.join("runner");
    let staged = root.join(".runner.staged");
    fs::write(&staged, format!("#!/bin/sh\nset -eu\n{body}\n")).expect("write staged runner");
    fs::set_permissions(&staged, fs::Permissions::from_mode(0o700)).expect("chmod staged runner");
    fs::rename(&staged, &path).expect("publish runner atomically");
    path
}

#[cfg(unix)]
#[test]
fn command_runner_exchanges_json_without_a_shell() {
    let root = tempfile::tempdir().expect("tempdir");
    let runner = executable_script(
        root.path(),
        r#"cat >/dev/null
printf '%s\n' '{"outputs":{"result":"ok"},"artifacts":[],"replay_fingerprint":{"runner":"fixture","version":1}}'"#,
    );
    let execution = execute_with_runner(
        root.path(),
        &workflow(),
        &Map::new(),
        &runner,
        Duration::from_secs(1),
    )
    .expect("command execution");

    assert_eq!(execution.outputs["result"], "ok");
    assert_eq!(
        execution.replay_fingerprint,
        Some(json!({
            "engine": COMMAND_ENGINE,
            "runner": {"runner": "fixture", "version": 1},
        }))
    );
}

#[cfg(unix)]
#[test]
fn command_runner_enforces_timeout() {
    let root = tempfile::tempdir().expect("tempdir");
    let runner = executable_script(root.path(), "cat >/dev/null\nsleep 1");
    let started = Instant::now();
    let error = execute_with_runner(
        root.path(),
        &workflow(),
        &Map::new(),
        &runner,
        Duration::from_millis(20),
    )
    .unwrap_err();

    assert!(error.to_string().contains("timed out after 20 ms"));
    assert!(
        started.elapsed() < Duration::from_millis(500),
        "timeout must terminate runner descendants promptly"
    );
}

#[test]
fn test_workflow_has_one_declared_output() {
    assert_eq!(workflow().outputs, vec![PortSpec::new("result", "text")]);
}
