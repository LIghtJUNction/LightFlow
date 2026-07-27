use crate::workflow::PortSpec;
use serde_json::{Map, Value};
use std::collections::BTreeSet;

pub(super) fn validate_port_map(
    ports: &[PortSpec],
    values: &Map<String, Value>,
    subject: &str,
) -> Vec<String> {
    let mut issues = Vec::new();
    let names = ports
        .iter()
        .map(|port| port.name.as_str())
        .collect::<BTreeSet<_>>();

    for name in values.keys() {
        if !names.contains(name.as_str()) {
            issues.push(format!("unknown {subject} `{name}`"));
        }
    }

    for port in ports {
        let value = values.get(&port.name);
        if port.required == Some(true) && port.default.is_none() && value.is_none_or(Value::is_null)
        {
            issues.push(format!("required {subject} `{}` is missing", port.name));
            continue;
        }
        if let Some(value) = value
            && !value.is_null()
        {
            validate_port_value(port, value, subject, &mut issues);
        }
    }

    issues
}

fn validate_port_value(port: &PortSpec, value: &Value, subject: &str, issues: &mut Vec<String>) {
    let valid_type = match port.ty.as_str() {
        "text" | "string" => value.is_string(),
        "path" => valid_path(value),
        "integer" => value.as_i64().is_some() || value.as_u64().is_some(),
        "number" => value.is_number(),
        "boolean" | "bool" => value.is_boolean(),
        "artifact" => value.is_object(),
        "artifact[]" => value
            .as_array()
            .is_some_and(|items| items.iter().all(Value::is_object)),
        "path[]" => value
            .as_array()
            .is_some_and(|items| items.iter().all(valid_path)),
        "json" | "any" => true,
        ty if ty.ends_with("[]") => value.is_array(),
        // Existing workflows may use application-defined type names. Node
        // Schema v1 keeps those values permissive until a type is standardized.
        _ => true,
    };
    if !valid_type {
        issues.push(format!(
            "{subject} `{}` must have type `{}`",
            port.name, port.ty
        ));
        return;
    }

    if !port.enum_values.is_empty() && !port.enum_values.contains(value) {
        issues.push(format!(
            "{subject} `{}` must be one of its declared enum values",
            port.name
        ));
    }

    let Some(number) = value.as_f64() else {
        return;
    };
    if let Some(min) = port.min
        && number < min
    {
        issues.push(format!("{subject} `{}` must be at least {min}", port.name));
    }
    if let Some(max) = port.max
        && number > max
    {
        issues.push(format!("{subject} `{}` must be at most {max}", port.name));
    }
}

fn valid_path(value: &Value) -> bool {
    value
        .as_str()
        .is_some_and(|path| !path.is_empty() && !path.contains('\0'))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn port(name: &str, ty: &str) -> PortSpec {
        PortSpec::new(name, ty)
    }

    #[test]
    fn validates_required_unknown_and_basic_types() {
        let mut required = port("title", "text");
        required.required = Some(true);
        let ports = vec![required, port("enabled", "boolean")];
        let values = Map::from_iter([
            ("enabled".to_owned(), json!("yes")),
            ("extra".to_owned(), json!(true)),
        ]);

        let issues = validate_port_map(&ports, &values, "input");
        assert!(
            issues
                .iter()
                .any(|issue| issue.contains("required input `title`"))
        );
        assert!(
            issues
                .iter()
                .any(|issue| issue.contains("unknown input `extra`"))
        );
        assert!(issues.iter().any(|issue| issue.contains("type `boolean`")));
    }

    #[test]
    fn validates_numeric_range_enum_and_paths() {
        let mut count = port("count", "integer");
        count.min = Some(1.0);
        count.max = Some(4.0);
        let mut mode = port("mode", "text");
        mode.enum_values = vec![json!("fast"), json!("safe")];
        let ports = vec![count, mode, port("source", "path"), port("files", "path[]")];
        let values = Map::from_iter([
            ("count".to_owned(), json!(5)),
            ("mode".to_owned(), json!("other")),
            ("source".to_owned(), json!("")),
            ("files".to_owned(), json!(["ok", "bad\0path"])),
        ]);

        let issues = validate_port_map(&ports, &values, "input");
        assert!(issues.iter().any(|issue| issue.contains("at most 4")));
        assert!(issues.iter().any(|issue| issue.contains("enum values")));
        assert_eq!(
            issues
                .iter()
                .filter(|issue| issue.contains("type `path"))
                .count(),
            2
        );
    }

    #[test]
    fn preserves_custom_type_compatibility() {
        let values = Map::from_iter([("value".to_owned(), json!({"custom": true}))]);
        assert!(validate_port_map(&[port("value", "domain/object")], &values, "input").is_empty());
    }
}
