use super::super::arguments::model_status_filter;
use super::super::error::McpError;
use crate::api::{ArtifactListOptions, ModelListOptions, ModelStatusFilter, RunListOptions};

pub(super) fn split_resource_uri(uri: &str) -> (&str, Option<&str>) {
    uri.split_once('?')
        .map(|(resource_uri, query)| (resource_uri, Some(query)))
        .unwrap_or((uri, None))
}

pub(super) fn resource_query_value(query: Option<&str>, name: &str) -> Option<String> {
    resource_query_parts(query)
        .find_map(|(key, value)| (key == name).then_some(value).flatten())
        .map(decode_query_component)
}

fn resource_query_has_key(query: Option<&str>, name: &str) -> bool {
    resource_query_parts(query).any(|(key, _value)| key == name)
}

fn resource_query_parts(query: Option<&str>) -> impl Iterator<Item = (String, Option<&str>)> {
    query
        .into_iter()
        .flat_map(|query| query.split('&'))
        .filter(|part| !part.is_empty())
        .map(|part| {
            let (key, value) = part
                .split_once('=')
                .map(|(key, value)| (key, Some(value)))
                .unwrap_or((part, None));
            (decode_query_component(key), value)
        })
}

fn decode_query_component(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' if index + 2 < bytes.len() => {
                if let (Some(high), Some(low)) =
                    (hex_value(bytes[index + 1]), hex_value(bytes[index + 2]))
                {
                    decoded.push(high * 16 + low);
                    index += 3;
                } else {
                    decoded.push(bytes[index]);
                    index += 1;
                }
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8_lossy(&decoded).into_owned()
}

fn hex_value(byte: u8) -> Option<u8> {
    match byte {
        b'0'..=b'9' => Some(byte - b'0'),
        b'a'..=b'f' => Some(byte - b'a' + 10),
        b'A'..=b'F' => Some(byte - b'A' + 10),
        _ => None,
    }
}

pub(super) fn resource_id<'a>(uri: &'a str, prefix: &str) -> Option<&'a str> {
    uri.strip_prefix(prefix).filter(|id| !id.contains('/'))
}

pub(super) fn resource_child_id<'a>(uri: &'a str, prefix: &str, child: &str) -> Option<&'a str> {
    let path = uri.strip_prefix(prefix)?;
    let (id, suffix) = path.rsplit_once('/')?;
    (suffix == child && !id.contains('/')).then_some(id)
}

pub(super) fn resource_query_bool(query: Option<&str>, name: &str) -> Option<bool> {
    resource_query_value(query, name)
        .map(|value| matches!(value.as_str(), "1" | "true" | "yes"))
        .or_else(|| resource_query_has_key(query, name).then_some(true))
}

fn resource_query_usize(
    query: Option<&str>,
    name: &str,
    context: &str,
) -> Result<Option<usize>, McpError> {
    let Some(value) = resource_query_value(query, name) else {
        return Ok(None);
    };
    value.parse::<usize>().map(Some).map_err(|_| {
        McpError::new(
            -32602,
            format!("{context} {name} must be a non-negative integer"),
        )
    })
}

pub(super) fn model_list_options_query(query: Option<&str>) -> Result<ModelListOptions, McpError> {
    let status = match resource_query_value(query, "status") {
        Some(value) => model_status_filter(&value)?,
        None => ModelStatusFilter::All,
    };
    Ok(ModelListOptions {
        workflow_id: resource_query_value(query, "workflow_id"),
        status,
    })
}

pub(super) fn run_list_options_query(query: Option<&str>) -> Result<RunListOptions, McpError> {
    Ok(RunListOptions {
        limit: resource_query_usize(query, "limit", "lightflow://runs")?,
        workflow_id: resource_query_value(query, "workflow_id"),
        status: resource_query_value(query, "status"),
    })
}

pub(super) fn artifact_list_options_query(
    query: Option<&str>,
) -> Result<ArtifactListOptions, McpError> {
    Ok(ArtifactListOptions {
        limit: resource_query_usize(query, "limit", "lightflow://artifacts")?,
        run_id: resource_query_value(query, "run_id"),
        workflow_id: resource_query_value(query, "workflow_id"),
        kind: resource_query_value(query, "kind"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resource_query_value_decodes_percent_encoded_values() {
        assert_eq!(
            resource_query_value(
                Some("project=%2Ftmp%2Flightflow%2Fprojects%2Flightflow-std"),
                "project",
            )
            .as_deref(),
            Some("/tmp/lightflow/projects/lightflow-std")
        );
        assert_eq!(
            resource_query_value(Some("workflow_id=lightflow.text%2Bplan"), "workflow_id")
                .as_deref(),
            Some("lightflow.text+plan")
        );
    }

    #[test]
    fn resource_query_value_decodes_percent_encoded_keys() {
        assert_eq!(
            resource_query_value(Some("workflow%5Fid=lightflow.text_plan"), "workflow_id")
                .as_deref(),
            Some("lightflow.text_plan")
        );
    }

    #[test]
    fn resource_query_value_decodes_plus_as_space_and_keeps_malformed_percent() {
        assert_eq!(
            resource_query_value(Some("kind=image+artifact"), "kind").as_deref(),
            Some("image artifact")
        );
        assert_eq!(
            resource_query_value(Some("project=%ZZ%2Flightflow"), "project").as_deref(),
            Some("%ZZ/lightflow")
        );
    }

    #[test]
    fn typed_resource_query_helpers_use_decoded_values() {
        assert_eq!(resource_query_bool(Some("dirty=true"), "dirty"), Some(true));
        assert_eq!(resource_query_bool(Some("dirty"), "dirty"), Some(true));
        assert_eq!(
            resource_query_value(Some("limit"), "limit"),
            None,
            "bare non-boolean parameters are key presence, not values"
        );
        assert_eq!(
            resource_query_usize(Some("limit"), "limit", "lightflow://runs")
                .expect("bare limit is not a value"),
            None
        );
        assert_eq!(
            resource_query_usize(Some("limit=20"), "limit", "lightflow://runs")
                .expect("limit parse"),
            Some(20)
        );
        assert_eq!(
            model_list_options_query(Some("workflow_id=lightflow.text%2Bplan&status=blocked"))
                .expect("model options")
                .workflow_id
                .as_deref(),
            Some("lightflow.text+plan")
        );
    }
}
