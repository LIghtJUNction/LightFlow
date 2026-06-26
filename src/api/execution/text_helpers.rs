use serde_json::{Map, Value};

use crate::workflow::WorkflowSpec;

pub(super) fn text_outputs(
    workflow: &WorkflowSpec,
    inputs: &Map<String, Value>,
    text: &str,
) -> Map<String, Value> {
    let mut outputs = Map::new();
    for output in &workflow.outputs {
        let value = match output.name.as_str() {
            "text" | "prompt" | "result" | "value" => Value::String(text.to_owned()),
            other => inputs.get(other).cloned().unwrap_or(Value::Null),
        };
        outputs.insert(output.name.clone(), value);
    }
    outputs
}

pub(super) fn control_outputs(
    workflow: &WorkflowSpec,
    inputs: &Map<String, Value>,
    selected_value: Value,
    selected: &str,
    capability: &str,
) -> Map<String, Value> {
    let mut outputs = Map::new();
    for output in &workflow.outputs {
        let value = match output.name.as_str() {
            "value" => selected_value.clone(),
            "selected" => Value::String(selected.to_owned()),
            "capability" => Value::String(capability.to_owned()),
            other => inputs.get(other).cloned().unwrap_or(Value::Null),
        };
        outputs.insert(output.name.clone(), value);
    }
    outputs
}

pub(super) fn merge_objects(a: Value, b: Value) -> Value {
    let mut merged = serde_json::Map::new();
    if let Value::Object(map) = a {
        merged.extend(map);
    }
    if let Value::Object(map) = b {
        merged.extend(map);
    }
    Value::Object(merged)
}

pub(super) fn split_value(value: Value) -> (Value, Value, Value) {
    match value {
        Value::Array(items) => {
            let first = items.first().cloned().unwrap_or(Value::Null);
            let rest = Value::Array(items.iter().skip(1).cloned().collect());
            let items = Value::Array(items);
            (first, rest, items)
        }
        Value::Object(map) => {
            let items = map
                .iter()
                .map(|(key, value)| serde_json::json!({ "key": key, "value": value }))
                .collect::<Vec<_>>();
            let first = items.first().cloned().unwrap_or(Value::Null);
            let rest = Value::Array(items.iter().skip(1).cloned().collect());
            (first, rest, Value::Array(items))
        }
        value => (value.clone(), Value::Null, serde_json::json!([value])),
    }
}

pub(super) fn lookup_json_path<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let path = path.trim();
    let path = path.strip_prefix('$').unwrap_or(path);
    let path = path.strip_prefix('.').unwrap_or(path);
    if path.is_empty() {
        return Some(value);
    }

    let mut current = value;
    for segment in path.split('.').filter(|segment| !segment.is_empty()) {
        current = match current {
            Value::Object(map) => map.get(segment)?,
            Value::Array(items) => items.get(segment.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(current)
}

pub(super) fn render_template(template: &str, vars: &Value) -> String {
    let mut rendered = String::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        rendered.push_str(&rest[..start]);
        let after_start = &rest[start + 2..];
        let Some(end) = after_start.find("}}") else {
            rendered.push_str(&rest[start..]);
            return rendered;
        };
        let key = after_start[..end].trim();
        if let Some(value) = lookup_json_path(vars, key) {
            rendered.push_str(&json_value_text(value));
        }

        rest = &after_start[end + 2..];
    }
    rendered.push_str(rest);
    rendered
}

pub(super) fn json_value_text(value: &Value) -> String {
    match value {
        Value::String(value) => value.clone(),
        Value::Null => String::new(),
        value => value.to_string(),
    }
}
