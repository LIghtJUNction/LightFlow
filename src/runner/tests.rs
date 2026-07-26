use super::{
    ModelBinding, PROTOCOL, Request, Response, WorkflowIdentity, parse_typed_input, read_request,
    write_response,
};
use crate::workflow::{ContextWorkflow, Runnable, WorkflowState};
use serde::{Deserialize, Serialize};
use serde_json::{Map, json};
use std::path::PathBuf;

#[derive(Debug, Deserialize, Eq, PartialEq)]
struct Input {
    user_message: String,
}

#[derive(Debug, Serialize, Eq, PartialEq)]
struct Output {
    answer: String,
}

struct Context {
    input: Input,
    answer: Option<String>,
}

#[derive(Clone, Copy)]
enum State {
    Answer,
    End,
}

impl WorkflowState for State {
    fn is_end(&self) -> bool {
        matches!(self, Self::End)
    }
}

struct ExampleWorkflow;

#[async_trait::async_trait]
impl ContextWorkflow for ExampleWorkflow {
    type Input = Input;
    type Output = Output;
    type Context = Context;
    type State = State;

    fn context(&self, input: Self::Input) -> Self::Context {
        Context {
            input,
            answer: None,
        }
    }

    fn initial_state(&self) -> Self::State {
        State::Answer
    }

    async fn step(
        &self,
        state: Self::State,
        context: &mut Self::Context,
    ) -> anyhow::Result<Self::State> {
        match state {
            State::Answer => {
                context.answer = Some(format!("回答：{}", context.input.user_message));
                Ok(State::End)
            }
            State::End => Ok(State::End),
        }
    }

    fn output(&self, context: Self::Context) -> anyhow::Result<Self::Output> {
        Ok(Output {
            answer: context.answer.unwrap_or_default(),
        })
    }
}

#[test]
fn typed_input_accepts_json_argument() {
    let args = vec![
        "--input".to_owned(),
        r#"{"user_message":"帮我查最新消息"}"#.to_owned(),
    ];
    let input: Input = parse_typed_input(&args).unwrap();
    assert_eq!(
        input,
        Input {
            user_message: "帮我查最新消息".to_owned()
        }
    );
}

#[tokio::test]
async fn typed_workflow_runs_through_unified_entrypoint() {
    let output = ExampleWorkflow
        .into_workflow()
        .run(Input {
            user_message: "hello".to_owned(),
        })
        .await
        .unwrap();
    assert_eq!(
        output,
        Output {
            answer: "回答：hello".to_owned()
        }
    );
}

#[test]
fn protocol_roundtrip_preserves_request_and_response() {
    let models = std::collections::BTreeMap::from([(
        "image_model".to_owned(),
        ModelBinding {
            requirement_id: "image_model".to_owned(),
            variant_id: "tiny".to_owned(),
            path: PathBuf::from("models/tiny.gguf"),
            sha256: Some("abc".to_owned()),
            size_bytes: Some(3),
            snapshot_revision: Some("revision".to_owned()),
        },
    )]);
    let request = Request {
        protocol: PROTOCOL.to_owned(),
        workflow: WorkflowIdentity {
            id: "lightflow.example".to_owned(),
            version: "0.1.0".to_owned(),
        },
        inputs: Map::from_iter([("value".to_owned(), json!("hello"))]),
        models,
    };
    let bytes = serde_json::to_vec(&request).expect("serialize request");
    assert_eq!(
        read_request(bytes.as_slice()).expect("read request"),
        request
    );

    let response = Response {
        outputs: Map::from_iter([("value".to_owned(), json!("hello"))]),
        artifacts: Vec::new(),
        replay_fingerprint: Map::from_iter([("algorithm".to_owned(), json!("example.v1"))]),
    };
    let mut encoded = Vec::new();
    write_response(&mut encoded, &response).expect("write response");
    assert_eq!(
        serde_json::from_slice::<Response>(&encoded).expect("decode response"),
        response
    );
}

#[test]
fn request_rejects_unknown_protocol() {
    let error = read_request(
        br#"{"protocol":"other","workflow":{"id":"x","version":"1"},"inputs":{}}"#.as_slice(),
    )
    .expect_err("unknown protocol");
    assert!(error.to_string().contains(PROTOCOL));
}

#[test]
fn request_rejects_forged_workflow_id_and_version() {
    let mut request = Request {
        protocol: PROTOCOL.to_owned(),
        workflow: WorkflowIdentity {
            id: "lightflow.forged".to_owned(),
            version: "0.1.0".to_owned(),
        },
        inputs: Map::new(),
        models: Default::default(),
    };
    let error = request
        .validate_for("lightflow.expected", "0.1.0")
        .expect_err("forged id");
    assert!(error.to_string().contains("expected workflow id"));

    request.workflow.id = "lightflow.expected".to_owned();
    request.workflow.version = "9.9.9".to_owned();
    let error = request
        .validate_for("lightflow.expected", "0.1.0")
        .expect_err("forged version");
    assert!(error.to_string().contains("expected workflow version"));
}
