use crate::api::ApiError;
use serde_json::{Value, json};

pub(super) fn error(id: Value, code: i64, message: &str, data: Option<Value>) -> Value {
    let mut value = json!({
        "jsonrpc": "2.0",
        "id": id,
        "error": {
            "code": code,
            "message": message
        }
    });
    if let Some(data) = data {
        value["error"]["data"] = data;
    }
    value
}

#[derive(Debug)]
pub(super) struct McpError {
    pub(super) code: i64,
    pub(super) message: String,
    pub(super) data: Option<Value>,
}

impl McpError {
    pub(super) fn new(code: i64, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            data: None,
        }
    }

    pub(super) fn with_data(mut self, data: Value) -> Self {
        self.data = Some(data);
        self
    }
}

impl From<ApiError> for McpError {
    fn from(error: ApiError) -> Self {
        Self::new(-32000, error.to_string())
    }
}

impl From<serde_json::Error> for McpError {
    fn from(error: serde_json::Error) -> Self {
        Self::new(-32603, error.to_string())
    }
}
