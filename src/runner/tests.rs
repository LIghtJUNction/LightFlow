use super::parse_typed_input;
use crate::workflow::{ContextWorkflow, Runnable, WorkflowState};
use serde::{Deserialize, Serialize};

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
