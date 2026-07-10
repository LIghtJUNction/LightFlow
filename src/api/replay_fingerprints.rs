pub(super) fn replay_report(
    selector: &str,
    original_execution: &serde_json::Value,
    replayed_execution: &serde_json::Value,
) -> serde_json::Value {
    let original_runtime = runtime_fingerprints(original_execution);
    let replayed_runtime = runtime_fingerprints(replayed_execution);
    let original_model_locks = model_lock_fingerprints(original_execution);
    let replayed_model_locks = model_lock_fingerprints(replayed_execution);
    serde_json::json!({
        "replayed_from": selector,
        "runtime_changed": original_runtime != replayed_runtime,
        "model_lock_changed": original_model_locks != replayed_model_locks,
        "original_runtime": original_runtime,
        "replayed_runtime": replayed_runtime,
        "original_model_locks": original_model_locks,
        "replayed_model_locks": replayed_model_locks,
    })
}

fn model_lock_fingerprints(execution: &serde_json::Value) -> Vec<serde_json::Value> {
    execution
        .get("model_locks")
        .and_then(serde_json::Value::as_array)
        .cloned()
        .unwrap_or_default()
}

fn runtime_fingerprints(execution: &serde_json::Value) -> Vec<serde_json::Value> {
    let mut runtimes = Vec::new();
    collect_runtime_fingerprints(execution, None, &mut runtimes);
    runtimes
}

fn collect_runtime_fingerprints(
    execution: &serde_json::Value,
    stage_index: Option<usize>,
    runtimes: &mut Vec<serde_json::Value>,
) {
    if execution
        .get("pipeline")
        .and_then(serde_json::Value::as_bool)
        == Some(true)
    {
        for (index, stage) in execution
            .get("stages")
            .and_then(serde_json::Value::as_array)
            .into_iter()
            .flatten()
            .enumerate()
        {
            collect_runtime_fingerprints(stage, Some(index), runtimes);
        }
        return;
    }

    let workflow_id = execution
        .get("workflow_id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or_default();
    if let Some(runtime) = execution
        .get("runtime")
        .filter(|runtime| !runtime.is_null())
    {
        runtimes.push(runtime_fingerprint(
            stage_index,
            workflow_id,
            None,
            None,
            runtime.clone(),
        ));
    }
    collect_node_runtime_fingerprints(execution.get("nodes"), stage_index, "", runtimes);
}

fn collect_node_runtime_fingerprints(
    nodes: Option<&serde_json::Value>,
    stage_index: Option<usize>,
    parent_path: &str,
    runtimes: &mut Vec<serde_json::Value>,
) {
    for node in nodes
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        let node_id = node
            .get("node_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default();
        let node_path = if parent_path.is_empty() {
            node_id.to_owned()
        } else {
            format!("{parent_path}/{node_id}")
        };
        let Some(runtime) = node.get("runtime").filter(|runtime| !runtime.is_null()) else {
            collect_node_runtime_fingerprints(node.get("nodes"), stage_index, &node_path, runtimes);
            continue;
        };
        let selected_workflow_id = node
            .get("selected_workflow_id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or_else(|| {
                node.get("workflow_id")
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
            });
        runtimes.push(runtime_fingerprint(
            stage_index,
            selected_workflow_id,
            Some(node_id),
            Some(&node_path),
            runtime.clone(),
        ));
        collect_node_runtime_fingerprints(node.get("nodes"), stage_index, &node_path, runtimes);
    }
}

fn runtime_fingerprint(
    stage_index: Option<usize>,
    workflow_id: &str,
    node_id: Option<&str>,
    node_path: Option<&str>,
    runtime: serde_json::Value,
) -> serde_json::Value {
    let mut value = serde_json::Map::new();
    if let Some(stage_index) = stage_index {
        value.insert("stage_index".to_owned(), stage_index.into());
    }
    value.insert("workflow_id".to_owned(), workflow_id.to_owned().into());
    if let Some(node_id) = node_id {
        value.insert("node_id".to_owned(), node_id.to_owned().into());
    }
    if let Some(node_path) = node_path {
        value.insert("node_path".to_owned(), node_path.to_owned().into());
    }
    value.insert("runtime".to_owned(), runtime);
    value.into()
}
