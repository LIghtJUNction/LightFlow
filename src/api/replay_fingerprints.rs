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
            runtime.clone(),
        ));
    }
    for node in execution
        .get("nodes")
        .and_then(serde_json::Value::as_array)
        .into_iter()
        .flatten()
    {
        let Some(runtime) = node.get("runtime").filter(|runtime| !runtime.is_null()) else {
            continue;
        };
        let node_id = node.get("node_id").and_then(serde_json::Value::as_str);
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
            node_id,
            runtime.clone(),
        ));
    }
}

fn runtime_fingerprint(
    stage_index: Option<usize>,
    workflow_id: &str,
    node_id: Option<&str>,
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
    value.insert("runtime".to_owned(), runtime);
    value.into()
}
