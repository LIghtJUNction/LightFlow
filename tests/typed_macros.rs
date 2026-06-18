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

#[node("classify")]
async fn classify(input: UserInput) -> lightflow::anyhow::Result<Intent> {
    Ok(Intent {
        message: input.message,
    })
}

#[workflow("qa")]
async fn qa(input: UserInput) -> lightflow::anyhow::Result<FinalAnswer> {
    let intent = classify(input).await?;
    Ok(FinalAnswer {
        answer: format!("回答：{}", intent.message),
    })
}

#[tokio::test]
async fn workflow_macro_generates_runnable_entrypoint() {
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
async fn node_macro_generates_hooked_entrypoint() {
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
