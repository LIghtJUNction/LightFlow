mod support;

use lightflow::api::ApiService;
use std::path::Path;
use support::*;

#[test]
fn repository_standard_control_nodes_are_discoverable_and_runnable()
-> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let service = ApiService::new(root);
    for (workflow_id, capability) in [
        ("lightflow.control.if", "lightflow.control.if"),
        ("lightflow.control.switch", "lightflow.control.switch"),
        ("lightflow.control.merge", "lightflow.control.merge"),
        ("lightflow.control.split", "lightflow.control.split"),
    ] {
        let workflow = service.get_workflow(workflow_id)?;
        assert_eq!(workflow.category.as_deref(), Some("std"));
        assert_eq!(workflow.runtimes[0].capability, capability);
    }

    let if_run = lfw(
        root,
        [
            "run",
            "lightflow.control.if",
            "-i",
            "condition=true",
            "-i",
            "then_value=\"yes\"",
            "-i",
            "else_value=\"no\"",
        ],
    )?;
    assert_eq!(if_run["outputs"]["value"], "yes");
    assert_eq!(if_run["outputs"]["selected"], "then");

    let switch_run = lfw(
        root,
        [
            "run",
            "lightflow.control.switch",
            "-i",
            "selector=final",
            "-i",
            "cases={\"draft\":\"loose\",\"final\":\"polished\"}",
            "-i",
            "default=\"loose\"",
        ],
    )?;
    assert_eq!(switch_run["outputs"]["value"], "polished");
    assert_eq!(switch_run["outputs"]["selected"], "final");

    let merge_run = lfw(
        root,
        [
            "run",
            "lightflow.control.merge",
            "-i",
            "a={\"prompt\":\"cat\"}",
            "-i",
            "b={\"seed\":1}",
            "-i",
            "mode=object",
        ],
    )?;
    assert_eq!(merge_run["outputs"]["value"]["prompt"], "cat");
    assert_eq!(merge_run["outputs"]["value"]["seed"], 1);

    let split_run = lfw(
        root,
        [
            "run",
            "lightflow.control.split",
            "-i",
            "value=[\"first\",\"second\",\"third\"]",
        ],
    )?;
    assert_eq!(split_run["outputs"]["first"], "first");
    assert_eq!(
        split_run["outputs"]["rest"],
        serde_json::json!(["second", "third"])
    );
    assert_eq!(
        split_run["outputs"]["items"],
        serde_json::json!(["first", "second", "third"])
    );

    Ok(())
}

#[test]
fn repository_standard_control_nodes_pass_node_conformance()
-> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for workflow_id in [
        "lightflow.control.if",
        "lightflow.control.switch",
        "lightflow.control.merge",
        "lightflow.control.split",
    ] {
        let report = lfw(root, ["node", "test", workflow_id])?;
        assert_eq!(report["valid"], true, "{workflow_id}");
    }
    Ok(())
}
