mod schemas;

use schemas::*;
use serde_json::{Value, json};

pub(super) fn tools() -> Value {
    json!([
        tool(
            "lightflow.workflow.list",
            "List LightFlow workflows.",
            json!({ "type": "object", "properties": {} })
        ),
        tool(
            "lightflow.workflow.get",
            "Read one LightFlow workflow.",
            id_schema("workflow_id")
        ),
        tool(
            "lightflow.workflow.dependencies",
            "Resolve recursive workflow dependencies for one LightFlow workflow.",
            id_schema("workflow_id")
        ),
        tool(
            "lightflow.workflow.plan",
            "Build the executor and model plan for one LightFlow workflow without running it.",
            id_schema("workflow_id")
        ),
        tool(
            "lightflow.workflow.publish_check",
            "Check whether one local workflow crate is ready for cargo publish dry-run.",
            id_schema("workflow_id")
        ),
        tool(
            "lightflow.workflow.publish_list",
            "List cargo publish dry-run readiness for local workflow crates, optionally narrowed to one linked project workspace.",
            publish_list_schema()
        ),
        tool(
            "lightflow.workflow.run",
            "Execute one LightFlow workflow with optional inputs and node toggles.",
            run_schema()
        ),
        tool(
            "lightflow.workflow.validate",
            "Validate one LightFlow workflow.",
            workflow_schema()
        ),
        tool(
            "lightflow.workflow.save",
            "Save one LightFlow workflow.",
            workflow_schema()
        ),
        tool(
            "lightflow.node.list",
            "List editor-facing workflow-backed node cards.",
            json!({ "type": "object", "properties": {} })
        ),
        tool(
            "lightflow.node.get",
            "Read one editor-facing workflow-backed node card.",
            id_schema("workflow_id")
        ),
        tool(
            "lightflow.executor.list",
            "List runtime executors with status, availability reasons, data policies, and model-planning flags.",
            json!({ "type": "object", "properties": {} })
        ),
        tool(
            "lightflow.model.list",
            "List model requirements with lock status and port bindings, optionally filtered by workflow or status.",
            model_list_schema()
        ),
        tool(
            "lightflow.run.list",
            "List recorded LightFlow runs, optionally filtered for focused inspection.",
            run_list_schema()
        ),
        tool(
            "lightflow.run.get",
            "Read one recorded LightFlow run by id, or last.",
            id_schema("run_id")
        ),
        tool(
            "lightflow.run.events",
            "Read events for one recorded LightFlow run by id, or last.",
            id_schema("run_id")
        ),
        tool(
            "lightflow.run.replay",
            "Replay one recorded LightFlow run by id, or last.",
            id_schema("run_id")
        ),
        tool(
            "lightflow.run.rm",
            "Remove one recorded LightFlow run by id, or last.",
            id_schema("run_id")
        ),
        tool(
            "lightflow.artifact.list",
            "List artifacts found in recorded LightFlow runs with optional filters and run, stage, node, and workflow context.",
            artifact_list_schema()
        ),
        tool(
            "lightflow.patch.list",
            "List reusable project workflow patches.",
            json!({ "type": "object", "properties": {} })
        ),
        tool(
            "lightflow.patch.get",
            "Read one reusable project workflow patch.",
            id_schema("name")
        ),
        tool(
            "lightflow.patch.save",
            "Save one reusable project workflow patch.",
            patch_save_schema()
        ),
        tool(
            "lightflow.patch.validate",
            "Validate one serializable workflow patch, optionally against a selected workflow.",
            patch_validate_schema()
        ),
        tool(
            "lightflow.patch.rm",
            "Remove one reusable project workflow patch.",
            id_schema("name")
        ),
        tool(
            "lightflow.loop.check",
            "Check local workflow-loop readiness for this project, or one selected workflow.",
            loop_check_schema()
        ),
        tool(
            "lightflow.loop.changes",
            "Check workflow source changes against colocated agent skill updates and report review blockers.",
            json!({ "type": "object", "properties": {} })
        ),
        tool(
            "lightflow.loop.projects",
            "Inspect linked sibling project workspaces under projects/ for multi-repo workflow iteration, including project config metadata, optional workspaces, submodule initialization commands, git status, child stage/commit/push commands, and parent gitlink staging commands.",
            json!({
                "type": "object",
                "properties": {
                    "dirty": {
                        "type": "boolean",
                        "description": "Return only workspaces with changed paths, stale parent gitlinks, or uninspectable git status."
                    },
                    "project": {
                        "type": "string",
                        "description": "Return one workspace by full name, label, path, or lightflow-* short alias such as std, flux, rig, auto-editing, or custom-tools."
                    }
                }
            })
        ),
        tool(
            "lightflow.release.check",
            "Plan release readiness gates, including source-change review and project workspace review, without executing commands. The response includes project config metadata, known optional workspaces, configured submodule initialization commands, and planned project catalog commands.",
            release_check_schema()
        )
    ])
}

fn tool(name: &str, description: &str, input_schema: Value) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": input_schema
    })
}
