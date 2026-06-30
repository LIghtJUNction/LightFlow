mod support;

use lightflow::api::ApiService;
use std::path::Path;
use support::*;

#[test]
fn repository_standard_text_nodes_are_discoverable_and_runnable()
-> Result<(), Box<dyn std::error::Error>> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let service = ApiService::new(root);

    let concat = service.get_workflow("lightflow.text.concat")?;
    assert_eq!(concat.category.as_deref(), Some("std"));
    assert_eq!(concat.runtimes[0].capability, "lightflow.text.concat");
    assert_eq!(
        concat
            .inputs
            .iter()
            .find(|port| port.name == "separator")
            .and_then(|port| port.default.clone()),
        Some(serde_json::json!(""))
    );

    let template = service.get_workflow("lightflow.text.template")?;
    assert_eq!(template.runtimes[0].capability, "lightflow.text.template");
    assert_eq!(template.inputs[0].widget.as_deref(), Some("textarea"));

    let extract = service.get_workflow("lightflow.json.extract")?;
    assert_eq!(extract.runtimes[0].capability, "lightflow.json.extract");
    assert_eq!(extract.outputs.len(), 3);

    let regex = service.get_workflow("lightflow.text.regex")?;
    assert_eq!(regex.runtimes[0].capability, "lightflow.text.regex");
    assert!(regex.outputs.iter().any(|port| port.name == "matched"));

    let nodes = service.list_nodes()?;
    assert!(nodes.nodes.iter().any(|node| {
        node.id == "lightflow.text.concat"
            && node
                .runtimes
                .iter()
                .any(|runtime| runtime.capability == "lightflow.text.concat" && runtime.available)
    }));

    let concat_run = lfw(
        root,
        [
            "run",
            "lightflow.text.concat",
            "-i",
            "a=hello",
            "-i",
            "b=world",
            "-i",
            "separator=-",
        ],
    )?;
    assert_eq!(concat_run["outputs"]["text"], "hello-world");

    let template_run = lfw(
        root,
        [
            "run",
            "lightflow.text.template",
            "-i",
            "template=Describe {{scene}} in {{style}} style",
            "-i",
            "vars={\"scene\":\"a quiet lake\",\"style\":\"ink\"}",
        ],
    )?;
    assert_eq!(
        template_run["outputs"]["text"],
        "Describe a quiet lake in ink style"
    );

    let extract_run = lfw(
        root,
        [
            "run",
            "lightflow.json.extract",
            "-i",
            "value={\"items\":[{\"title\":\"first\"}]}",
            "-i",
            "path=items.0.title",
        ],
    )?;
    assert_eq!(extract_run["outputs"]["value"], "first");
    assert_eq!(extract_run["outputs"]["text"], "first");
    assert_eq!(extract_run["outputs"]["found"], true);

    let regex_run = lfw(
        root,
        [
            "run",
            "lightflow.text.regex",
            "-i",
            "text=item-42",
            "-i",
            "pattern=(\\d+)",
            "-i",
            "replacement=id:$1",
        ],
    )?;
    assert_eq!(regex_run["outputs"]["text"], "item-id:42");
    assert_eq!(regex_run["outputs"]["matched"], true);
    assert_eq!(regex_run["outputs"]["match_count"], 1);
    assert_eq!(regex_run["outputs"]["first_match"], "42");

    Ok(())
}

#[test]
fn repository_standard_text_nodes_pass_node_conformance() -> Result<(), Box<dyn std::error::Error>>
{
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    for workflow_id in [
        "lightflow.text.concat",
        "lightflow.text.template",
        "lightflow.json.extract",
        "lightflow.text.regex",
    ] {
        let report = lfw(root, ["node", "test", workflow_id])?;
        assert_eq!(report["valid"], true, "{workflow_id}");
    }
    Ok(())
}
