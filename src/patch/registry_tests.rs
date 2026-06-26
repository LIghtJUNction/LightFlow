use super::{AroundHook, HookRegistry, Next, NodeHook, run_node_borrowed};
use async_trait::async_trait;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

#[derive(Clone)]
struct LogHook {
    events: Arc<Mutex<Vec<&'static str>>>,
}

#[async_trait]
impl NodeHook<String, String> for LogHook {
    async fn before(&self, _input: &String) -> anyhow::Result<()> {
        self.events.lock().unwrap().push("before");
        Ok(())
    }

    async fn after(&self, _input: &String, _output: &String) -> anyhow::Result<()> {
        self.events.lock().unwrap().push("after");
        Ok(())
    }

    async fn on_error(&self, _input: &String, _error: &anyhow::Error) -> anyhow::Result<()> {
        self.events.lock().unwrap().push("error");
        Ok(())
    }
}

struct PrefixAround;

#[async_trait]
impl AroundHook<String, String> for PrefixAround {
    async fn call(&self, input: String, next: Next<String, String>) -> anyhow::Result<String> {
        Ok(format!("around:{}", next.run(input).await?))
    }
}

#[tokio::test]
async fn borrowed_node_runs_before_after_hooks() {
    let events = Arc::new(Mutex::new(Vec::new()));
    let hooks = HookRegistry::new().hook(
        "answer",
        LogHook {
            events: events.clone(),
        },
    );
    let input = "hello".to_owned();

    let output = run_node_borrowed(
        "answer",
        &input,
        || async { Ok(format!("{input}!")) },
        &hooks,
    )
    .await
    .unwrap();

    assert_eq!(output, "hello!");
    assert_eq!(*events.lock().unwrap(), vec!["before", "after"]);
}

#[tokio::test]
async fn around_hook_wraps_node_execution() {
    let hooks = HookRegistry::new().around("answer", PrefixAround);

    let output = super::run_node(
        "answer",
        "hello".to_owned(),
        |input| async move { Ok(format!("{input}!")) },
        &hooks,
    )
    .await
    .unwrap();

    assert_eq!(output, "around:hello!");
}

#[tokio::test]
async fn replace_patch_overrides_node_execution() {
    let hooks =
        HookRegistry::new().replace("search", |_input: String| async { Ok("mock".to_owned()) });

    let output = super::run_node(
        "search",
        "hello".to_owned(),
        |_input| async { Ok("real".to_owned()) },
        &hooks,
    )
    .await
    .unwrap();

    assert_eq!(output, "mock");
}

#[tokio::test]
async fn disable_patch_uses_typed_fallback() {
    let hooks = HookRegistry::new().disable_with("payment", |_input: String| async {
        Ok("disabled".to_owned())
    });

    let output = super::run_node(
        "payment",
        "hello".to_owned(),
        |_input| async { Ok("charged".to_owned()) },
        &hooks,
    )
    .await
    .unwrap();

    assert_eq!(output, "disabled");
}

#[tokio::test]
async fn disabled_node_without_fallback_fails_with_node_id() {
    let hooks: HookRegistry<String, String> = HookRegistry::new().disable("payment");

    let error = super::run_node(
        "payment",
        "hello".to_owned(),
        |_input| async { Ok("charged".to_owned()) },
        &hooks,
    )
    .await
    .unwrap_err();

    assert!(error.to_string().contains("payment"));
}

#[tokio::test]
async fn retry_patch_retries_until_success() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let hooks = HookRegistry::new().retry("flaky", 3);
    let attempts_for_node = attempts.clone();

    let output = super::run_node(
        "flaky",
        "hello".to_owned(),
        move |input| {
            let attempts = attempts_for_node.clone();
            async move {
                let attempt = attempts.fetch_add(1, Ordering::SeqCst) + 1;
                if attempt < 3 {
                    Err(anyhow::anyhow!("attempt {attempt} failed"))
                } else {
                    Ok(format!("{input}:{attempt}"))
                }
            }
        },
        &hooks,
    )
    .await
    .unwrap();

    assert_eq!(output, "hello:3");
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn timeout_patch_fails_slow_node() {
    let hooks = HookRegistry::new().timeout("slow", Duration::from_millis(1));

    let error = super::run_node(
        "slow",
        "hello".to_owned(),
        |_input| async {
            tokio::time::sleep(Duration::from_millis(25)).await;
            Ok("late".to_owned())
        },
        &hooks,
    )
    .await
    .unwrap_err();

    assert!(error.to_string().contains("timed out"));
}
