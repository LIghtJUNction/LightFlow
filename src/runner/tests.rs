use super::{
    CommandRequest, CommandResponse, MAX_COMMAND_REQUEST_BYTES, parse_typed_input,
    read_command_request, write_command_response,
};
use crate::workflow::{ContextWorkflow, Runnable, WorkflowState};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use std::io::Cursor;

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

#[test]
fn command_request_requires_the_live_protocol_and_exact_package_identity() {
    let outbound = CommandRequest::new(
        "lightflow.example",
        "0.1.0",
        Map::from_iter([("value".to_owned(), Value::String("ok".to_owned()))]),
    );
    let mut reader = Cursor::new(serde_json::to_vec(&outbound).expect("request encoding"));
    let request = read_command_request(&mut reader).expect("valid command request");
    assert_eq!(request.workflow_identity(), ("lightflow.example", "0.1.0"));
    request
        .validate_for("lightflow.example", "0.1.0")
        .expect("matching identity");
    assert!(request.validate_for("lightflow.other", "0.1.0").is_err());

    let mut wrong_protocol = Cursor::new(
        br#"{"protocol":"lightflow.runner.v1","workflow":{"id":"lightflow.example","version":"0.1.0"},"inputs":{}}"#,
    );
    assert!(read_command_request(&mut wrong_protocol).is_err());

    let mut unknown_field = Cursor::new(
        br#"{"protocol":"lightflow.command.v1","workflow":{"id":"lightflow.example","version":"0.1.0"},"inputs":{},"unexpected":true}"#,
    );
    assert!(read_command_request(&mut unknown_field).is_err());

    let mut duplicate_field = Cursor::new(
        br#"{"protocol":"lightflow.command.v1","protocol":"lightflow.command.v1","workflow":{"id":"lightflow.example","version":"0.1.0"},"inputs":{}}"#,
    );
    assert!(read_command_request(&mut duplicate_field).is_err());
}

#[test]
fn command_request_is_bounded_before_json_decode() {
    let mut reader = Cursor::new(vec![b'x'; MAX_COMMAND_REQUEST_BYTES + 1]);
    let error = read_command_request(&mut reader).expect_err("oversize request must fail");
    assert!(error.to_string().contains("exceeds its protocol limit"));
}

#[test]
fn command_response_writer_emits_one_json_document_to_its_given_stream() {
    let response = CommandResponse {
        outputs: Map::from_iter([("result".to_owned(), Value::String("ok".to_owned()))]),
        artifacts: Vec::new(),
        replay_fingerprint: Map::from_iter([("runner".to_owned(), json!("test"))]),
    };
    let mut bytes = Vec::new();
    write_command_response(&mut bytes, &response).expect("response encoding");
    assert!(bytes.ends_with(b"\n"));
    let decoded: Value = serde_json::from_slice(&bytes).expect("single JSON response");
    assert_eq!(decoded["outputs"]["result"], "ok");
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
