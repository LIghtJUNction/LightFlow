#![allow(unused_imports)]

mod cli_project_support;
mod support;

use cli_project_support::*;
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
