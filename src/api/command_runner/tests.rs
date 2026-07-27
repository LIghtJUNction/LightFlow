use super::*;
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
        replay_fingerprint: Map::from_iter([
            ("runner".to_owned(), "test".into()),
            ("version".to_owned(), 1.into()),
        ]),
    }
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
    let missing = validate_response(root.path(), &workflow(), response(Map::new())).unwrap_err();
    assert!(missing.to_string().contains("missing [result]"));

    let mut outputs = Map::new();
    outputs.insert("result".to_owned(), "ok".into());
    outputs.insert("extra".to_owned(), true.into());
    let unknown = validate_response(root.path(), &workflow(), response(outputs)).unwrap_err();
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
    let error = validate_artifacts(root.path(), std::slice::from_ref(&artifact)).unwrap_err();
    assert!(error.to_string().contains("does not name an existing file"));

    fs::write(root.path().join("video.mp4"), b"video").expect("artifact");
    let existing = WorkflowArtifact {
        path: "video.mp4".to_owned(),
        ..artifact
    };
    let error = validate_artifacts(root.path(), &[existing.clone(), existing]).unwrap_err();
    assert!(error.to_string().contains("duplicate artifact id video"));
}

#[test]
fn response_outputs_must_match_declared_types() {
    let root = tempfile::tempdir().expect("tempdir");
    let mut outputs = Map::new();
    outputs.insert("result".to_owned(), true.into());
    let error = validate_response(root.path(), &workflow(), response(outputs)).unwrap_err();
    assert!(
        error
            .to_string()
            .contains("output result must match declared type text, got boolean")
    );
}

#[test]
fn response_rejects_non_object_replay_fingerprint_during_decode() {
    let error = serde_json::from_str::<CommandResponse>(
        r#"{"outputs":{"result":"ok"},"artifacts":[],"replay_fingerprint":null}"#,
    )
    .expect_err("a replay fingerprint must be an object");
    assert!(error.to_string().contains("invalid type"));
}

#[test]
fn capped_reader_rejects_oversized_output() {
    let error = read_capped(&b"12345"[..], 4).unwrap_err();
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
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
