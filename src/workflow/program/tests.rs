use super::{ContextWorkflow, Runnable, Workflow, WorkflowState};

#[derive(Debug, Clone, Eq, PartialEq)]
struct UserInput {
    user_message: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
enum Intent {
    Search(String),
    Direct(String),
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct SearchResult {
    text: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct FinalAnswer {
    answer: String,
}

#[derive(Debug, Clone, Eq, PartialEq)]
struct Context {
    input: UserInput,
    intent: Option<Intent>,
}

#[derive(Debug, Clone, Copy, Eq, PartialEq)]
enum State {
    Classify,
    End,
}

impl WorkflowState for State {
    fn is_end(&self) -> bool {
        matches!(self, Self::End)
    }
}

struct ClassifyWorkflow;

#[async_trait::async_trait]
impl ContextWorkflow for ClassifyWorkflow {
    type Input = UserInput;
    type Output = Intent;
    type Context = Context;
    type State = State;

    fn context(&self, input: Self::Input) -> Self::Context {
        Context {
            input,
            intent: None,
        }
    }

    fn initial_state(&self) -> Self::State {
        State::Classify
    }

    async fn step(
        &self,
        state: Self::State,
        context: &mut Self::Context,
    ) -> anyhow::Result<Self::State> {
        match state {
            State::Classify => {
                context.intent = Some(if context.input.user_message.contains("最新") {
                    Intent::Search(context.input.user_message.clone())
                } else {
                    Intent::Direct(context.input.user_message.clone())
                });
                Ok(State::End)
            }
            State::End => Ok(State::End),
        }
    }

    fn output(&self, context: Self::Context) -> anyhow::Result<Self::Output> {
        context
            .intent
            .ok_or_else(|| anyhow::anyhow!("missing classified intent"))
    }
}

#[tokio::test]
async fn workflow_then_composes_checked_input_output_types() {
    let classify_flow: Workflow<UserInput, Intent> = ClassifyWorkflow.into_workflow();
    let search_flow: Workflow<Intent, SearchResult> = Workflow::task(|intent| async move {
        Ok(match intent {
            Intent::Search(query) => SearchResult {
                text: format!("搜索结果：{query}"),
            },
            Intent::Direct(message) => SearchResult { text: message },
        })
    });
    let answer_flow: Workflow<SearchResult, FinalAnswer> =
        Workflow::task(|result: SearchResult| async move {
            Ok(FinalAnswer {
                answer: format!("回答：{}", result.text),
            })
        });

    let output = classify_flow
        .then(search_flow)
        .then(answer_flow)
        .run(UserInput {
            user_message: "帮我查最新消息".to_owned(),
        })
        .await
        .unwrap();

    assert_eq!(
        output,
        FinalAnswer {
            answer: "回答：搜索结果：帮我查最新消息".to_owned()
        }
    );
}

#[tokio::test]
async fn workflow_branch_routes_after_typed_output() {
    let classify_flow: Workflow<UserInput, Intent> = ClassifyWorkflow.into_workflow();
    let search = Workflow::task(|intent| async move {
        let Intent::Search(query) = intent else {
            return Err(anyhow::anyhow!("expected search intent"));
        };
        Ok(FinalAnswer {
            answer: format!("搜索后回答：{query}"),
        })
    });
    let direct = Workflow::task(|intent| async move {
        let Intent::Direct(message) = intent else {
            return Err(anyhow::anyhow!("expected direct intent"));
        };
        Ok(FinalAnswer {
            answer: format!("直接回答：{message}"),
        })
    });

    let output = classify_flow
        .branch(|intent| matches!(intent, Intent::Search(_)), search, direct)
        .run(UserInput {
            user_message: "hello".to_owned(),
        })
        .await
        .unwrap();

    assert_eq!(
        output,
        FinalAnswer {
            answer: "直接回答：hello".to_owned()
        }
    );
}
