use lightflow::preload::*;

#[derive(Clone)]
struct UserInput {
    message: String,
}

#[derive(Clone)]
struct Intent {
    message: String,
}

struct FinalAnswer {
    answer: String,
}

async fn classify(input: UserInput) -> lightflow::anyhow::Result<Intent> {
    Ok(Intent {
        message: input.message,
    })
}

async fn classify_with_hooks(
    input: UserInput,
    hooks: &HookRegistry<UserInput, Intent>,
) -> lightflow::anyhow::Result<Intent> {
    run_node("classify", input, classify, hooks).await
}

#[derive(Debug, Clone, Copy, Default)]
struct Qa;

#[async_trait::async_trait]
impl Runnable<UserInput, FinalAnswer> for Qa {
    async fn run(&self, input: UserInput) -> lightflow::anyhow::Result<FinalAnswer> {
        let intent = classify(input).await?;
        Ok(FinalAnswer {
            answer: format!("回答：{}", intent.message),
        })
    }
}

impl Qa {
    fn name(&self) -> &'static str {
        "qa"
    }

    fn schema(&self) -> serde_json::Value {
        serde_json::json!({
            "name": "qa",
            "kind": "workflow",
            "input": "UserInput",
            "output": "FinalAnswer"
        })
    }
}

#[tokio::test]
async fn typed_runnable_entrypoint_runs_without_proc_macros() {
    let qa = Qa;
    let output = qa
        .run(UserInput {
            message: "hello".to_owned(),
        })
        .await
        .unwrap();

    assert_eq!(output.answer, "回答：hello");
    assert_eq!(qa.name(), "qa");
    assert_eq!(qa.schema()["kind"], "workflow");
}

#[tokio::test]
async fn hooked_node_entrypoint_runs_without_proc_macros() {
    let hooks = HookRegistry::new().replace("classify", |_input: UserInput| async {
        Ok(Intent {
            message: "patched".to_owned(),
        })
    });

    let intent = classify_with_hooks(
        UserInput {
            message: "original".to_owned(),
        },
        &hooks,
    )
    .await
    .unwrap();

    assert_eq!(intent.message, "patched");
}
