use serde_json::{Value, json};

pub(super) fn id_schema(id_name: &str) -> Value {
    let description = match id_name {
        "workflow_id" => {
            "Workflow id to inspect, such as lightflow.text_plan or another discovered workflow."
        }
        "run_id" => "Recorded run id to inspect, replay, or remove; use last for the newest run.",
        "name" => "Reusable patch registry name under .lightflow/patches/<name>.json.",
        _ => "Required string identifier.",
    };
    json!({
        "type": "object",
        "required": [id_name],
        "properties": {
            id_name: {
                "type": "string",
                "description": description
            }
        }
    })
}

pub(super) fn publish_list_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "project": {
                "type": "string",
                "description": "Narrow publish readiness to one linked project workspace by full name, label, path, or lightflow-* short alias such as std, flux, rig, auto-editing, or custom-tools."
            }
        }
    })
}

pub(super) fn loop_check_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "workflow_id": { "type": "string" },
            "require_replay": {
                "type": "boolean",
                "description": "Fail selected workflow readiness when no completed run can be replayed."
            },
            "require_selected_replay": {
                "type": "boolean",
                "description": "Alias for require_replay."
            }
        }
    })
}

pub(super) fn release_check_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "workflow_id": {
                "type": "string",
                "description": "Selected workflow for the replay-required release gate. Defaults to lightflow.text_plan."
            },
            "project": {
                "type": "string",
                "description": "Narrow project workspace review and project commands to one workspace by full name, label, path, or lightflow-* short alias such as std, flux, rig, auto-editing, or custom-tools."
            }
        }
    })
}

pub(super) fn run_list_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "limit": {
                "type": "integer",
                "minimum": 0,
                "description": "Maximum number of newest run summaries to return after filtering."
            },
            "workflow_id": {
                "type": "string",
                "description": "Return only runs whose recorded stages include this workflow id."
            },
            "status": {
                "type": "string",
                "enum": ["completed", "failed", "unknown"],
                "description": "Return only runs with this summary status, such as completed, failed, or unknown."
            }
        }
    })
}

pub(super) fn artifact_list_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "limit": {
                "type": "integer",
                "minimum": 0,
                "description": "Maximum number of artifact rows to return after filtering."
            },
            "run_id": {
                "type": "string",
                "description": "Return only artifacts from this run id, or last."
            },
            "workflow_id": {
                "type": "string",
                "description": "Return only artifacts produced by this workflow id."
            },
            "kind": {
                "type": "string",
                "description": "Return only artifacts with this artifact kind, such as image or mask."
            }
        }
    })
}

pub(super) fn model_list_schema() -> Value {
    json!({
        "type": "object",
        "properties": {
            "workflow_id": {
                "type": "string",
                "description": "Return only model requirements declared by this workflow id."
            },
            "status": {
                "type": "string",
                "enum": ["all", "available", "blocked"],
                "description": "Return all, available, or blocked model requirements."
            }
        }
    })
}

pub(super) fn workflow_schema() -> Value {
    json!({
        "type": "object",
        "required": ["workflow"],
        "properties": {
            "workflow": {
                "type": "object",
                "additionalProperties": true,
                "description": "WorkflowSpec JSON object with id, version, name, inputs, outputs, nodes, edges, runtimes, and models."
            }
        }
    })
}

pub(super) fn run_schema() -> Value {
    json!({
        "type": "object",
        "required": ["workflow_id"],
        "properties": {
            "workflow_id": {
                "type": "string",
                "description": "Workflow id to execute, such as lightflow.text_plan."
            },
            "inputs": {
                "type": "object",
                "additionalProperties": true,
                "description": "Workflow input values keyed by input port name."
            },
            "disabled_nodes": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Graph node ids to skip for this run."
            },
            "enabled_nodes": {
                "type": "array",
                "items": { "type": "string" },
                "description": "Graph node ids to force-enable for this run when they would otherwise be skipped."
            },
            "patch": {
                "type": "object",
                "additionalProperties": true,
                "description": "Serializable run patch for replacement, fallback, retry, or timeout behavior."
            }
        }
    })
}

pub(super) fn patch_validate_schema() -> Value {
    json!({
        "type": "object",
        "required": ["patch"],
        "properties": {
            "patch": {
                "type": "object",
                "additionalProperties": true,
                "description": "Serializable workflow patch object with node-keyed replacement, fallback, retry, timeout, or toggle rules."
            },
            "workflow_id": {
                "type": "string",
                "description": "When set, validate patch node keys and replacement/fallback workflow contracts against this selected workflow."
            }
        }
    })
}

pub(super) fn patch_save_schema() -> Value {
    json!({
        "type": "object",
        "required": ["name", "patch"],
        "properties": {
            "name": {
                "type": "string",
                "description": "Registry name to store under .lightflow/patches/<name>.json."
            },
            "patch": {
                "type": "object",
                "additionalProperties": true,
                "description": "Serializable workflow patch object with node-keyed replacement, fallback, retry, timeout, or toggle rules."
            }
        }
    })
}
