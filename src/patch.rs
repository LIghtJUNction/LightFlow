//! Typed hook and patch primitives for code-first workflows.

use async_trait::async_trait;
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

type BoxNodeFuture<O> = Pin<Box<dyn Future<Output = anyhow::Result<O>> + Send>>;
type NodeFn<I, O> = Arc<dyn Fn(I) -> BoxNodeFuture<O> + Send + Sync>;

/// Retry policy for a patched node.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct RetryPolicy {
    /// Total attempts, including the first attempt.
    pub attempts: usize,
}

impl RetryPolicy {
    /// Build a retry policy. Values below one are normalized to one.
    pub fn attempts(attempts: usize) -> Self {
        Self {
            attempts: attempts.max(1),
        }
    }
}

/// A before/after/error hook for one typed node boundary.
#[async_trait]
pub trait NodeHook<I, O>: Send + Sync {
    /// Called before node execution.
    async fn before(&self, _input: &I) -> anyhow::Result<()> {
        Ok(())
    }

    /// Called after successful node execution.
    async fn after(&self, _input: &I, _output: &O) -> anyhow::Result<()> {
        Ok(())
    }

    /// Called when node execution fails.
    async fn on_error(&self, _input: &I, _error: &anyhow::Error) -> anyhow::Result<()> {
        Ok(())
    }
}

/// The rest of an around-hook chain.
pub struct Next<I, O> {
    call: Arc<dyn Fn(I) -> BoxNodeFuture<O> + Send + Sync>,
}

impl<I, O> Clone for Next<I, O> {
    fn clone(&self) -> Self {
        Self {
            call: self.call.clone(),
        }
    }
}

impl<I, O> Next<I, O> {
    /// Continue execution.
    pub async fn run(&self, input: I) -> anyhow::Result<O> {
        (self.call)(input).await
    }
}

/// Middleware that can wrap a typed node.
#[async_trait]
pub trait AroundHook<I, O>: Send + Sync {
    /// Run this middleware and call `next` when execution should continue.
    async fn call(&self, input: I, next: Next<I, O>) -> anyhow::Result<O>;
}

/// Patch registry for one typed node input/output shape.
///
/// A registry is intentionally typed. Different node signatures should use
/// different registries so Rust keeps composition honest.
pub struct HookRegistry<I, O> {
    hooks: BTreeMap<String, Vec<Arc<dyn NodeHook<I, O>>>>,
    around: BTreeMap<String, Vec<Arc<dyn AroundHook<I, O>>>>,
    disabled: BTreeSet<String>,
    disabled_fallbacks: BTreeMap<String, NodeFn<I, O>>,
    replacements: BTreeMap<String, NodeFn<I, O>>,
    retries: BTreeMap<String, RetryPolicy>,
    timeouts: BTreeMap<String, Duration>,
}

impl<I, O> Clone for HookRegistry<I, O> {
    fn clone(&self) -> Self {
        Self {
            hooks: self.hooks.clone(),
            around: self.around.clone(),
            disabled: self.disabled.clone(),
            disabled_fallbacks: self.disabled_fallbacks.clone(),
            replacements: self.replacements.clone(),
            retries: self.retries.clone(),
            timeouts: self.timeouts.clone(),
        }
    }
}

impl<I, O> Default for HookRegistry<I, O> {
    fn default() -> Self {
        Self {
            hooks: BTreeMap::new(),
            around: BTreeMap::new(),
            disabled: BTreeSet::new(),
            disabled_fallbacks: BTreeMap::new(),
            replacements: BTreeMap::new(),
            retries: BTreeMap::new(),
            timeouts: BTreeMap::new(),
        }
    }
}

impl<I, O> HookRegistry<I, O> {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a before/after/error hook for a node id.
    pub fn hook<H>(mut self, node_id: impl Into<String>, hook: H) -> Self
    where
        H: NodeHook<I, O> + 'static,
    {
        self.hooks
            .entry(node_id.into())
            .or_default()
            .push(Arc::new(hook));
        self
    }

    /// Register around middleware for a node id.
    pub fn around<H>(mut self, node_id: impl Into<String>, hook: H) -> Self
    where
        H: AroundHook<I, O> + 'static,
    {
        self.around
            .entry(node_id.into())
            .or_default()
            .push(Arc::new(hook));
        self
    }

    /// Disable a node. Use [`run_node_with_fallback`] to provide fallback output.
    pub fn disable(mut self, node_id: impl Into<String>) -> Self {
        self.disabled.insert(node_id.into());
        self
    }

    /// Disable a node and use a typed fallback in its place.
    pub fn disable_with<F, Fut>(mut self, node_id: impl Into<String>, fallback: F) -> Self
    where
        I: Send + Sync + 'static,
        O: Send + Sync + 'static,
        F: Fn(I) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = anyhow::Result<O>> + Send + 'static,
    {
        let node_id = node_id.into();
        self.disabled.insert(node_id.clone());
        self.disabled_fallbacks.insert(node_id, node_fn(fallback));
        self
    }

    /// Replace a node implementation without changing workflow source.
    pub fn replace<F, Fut>(mut self, node_id: impl Into<String>, replacement: F) -> Self
    where
        I: Send + Sync + 'static,
        O: Send + Sync + 'static,
        F: Fn(I) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = anyhow::Result<O>> + Send + 'static,
    {
        self.replacements
            .insert(node_id.into(), node_fn(replacement));
        self
    }

    /// Retry a node up to `attempts` total attempts.
    pub fn retry(mut self, node_id: impl Into<String>, attempts: usize) -> Self {
        self.retries
            .insert(node_id.into(), RetryPolicy::attempts(attempts));
        self
    }

    /// Fail a node attempt when it exceeds `duration`.
    pub fn timeout(mut self, node_id: impl Into<String>, duration: Duration) -> Self {
        self.timeouts.insert(node_id.into(), duration);
        self
    }

    /// Fail a node attempt when it exceeds `milliseconds`.
    pub fn timeout_ms(self, node_id: impl Into<String>, milliseconds: u64) -> Self {
        self.timeout(node_id, Duration::from_millis(milliseconds))
    }

    /// Whether a node id is disabled by this registry.
    pub fn is_disabled(&self, node_id: &str) -> bool {
        self.disabled.contains(node_id)
    }

    async fn before(&self, node_id: &str, input: &I) -> anyhow::Result<()> {
        for hook in self.hooks.get(node_id).into_iter().flatten() {
            hook.before(input).await?;
        }
        Ok(())
    }

    async fn after(&self, node_id: &str, input: &I, output: &O) -> anyhow::Result<()> {
        for hook in self.hooks.get(node_id).into_iter().flatten() {
            hook.after(input, output).await?;
        }
        Ok(())
    }

    async fn on_error(
        &self,
        node_id: &str,
        input: &I,
        error: &anyhow::Error,
    ) -> anyhow::Result<()> {
        for hook in self.hooks.get(node_id).into_iter().flatten() {
            hook.on_error(input, error).await?;
        }
        Ok(())
    }
}

/// Run one typed node through registered hooks.
pub async fn run_node<I, O, F, Fut>(
    node_id: &str,
    input: I,
    f: F,
    hooks: &HookRegistry<I, O>,
) -> anyhow::Result<O>
where
    I: Clone + Send + Sync + 'static,
    O: Send + Sync + 'static,
    F: Fn(I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = anyhow::Result<O>> + Send + 'static,
{
    let started_at = crate::trace::node_start(node_id);
    hooks.before(node_id, &input).await?;
    let next = around_chain(node_id, effective_node(node_id, f, hooks), hooks);
    let attempts = hooks
        .retries
        .get(node_id)
        .copied()
        .unwrap_or_else(|| RetryPolicy::attempts(1))
        .attempts;
    let timeout = hooks.timeouts.get(node_id).copied();
    let mut last_error = None;

    for _ in 0..attempts {
        let result = run_node_attempt(input.clone(), next.clone(), timeout).await;
        match result {
            Ok(output) => {
                hooks.after(node_id, &input, &output).await?;
                crate::trace::node_end(node_id, started_at);
                return Ok(output);
            }
            Err(error) => last_error = Some(error),
        }
    }

    let error = last_error.expect("node attempts are always at least one");
    hooks.on_error(node_id, &input, &error).await?;
    crate::trace::node_error(node_id, started_at, &error);
    Err(error)
}

/// Run one typed node through hooks, using `fallback` when disabled.
pub async fn run_node_with_fallback<I, O, F, Fut, G, FallbackFut>(
    node_id: &str,
    input: I,
    f: F,
    fallback: G,
    hooks: &HookRegistry<I, O>,
) -> anyhow::Result<O>
where
    I: Clone + Send + Sync + 'static,
    O: Send + Sync + 'static,
    F: Fn(I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = anyhow::Result<O>> + Send + 'static,
    G: Fn(I) -> FallbackFut + Send + Sync + 'static,
    FallbackFut: Future<Output = anyhow::Result<O>> + Send + 'static,
{
    let hooks = hooks.clone().disable_with(node_id.to_owned(), fallback);
    run_node(node_id, input, f, &hooks).await
}

/// Run a borrowed-input node through before/after/error hooks.
pub async fn run_node_borrowed<I, O, F, Fut>(
    node_id: &str,
    input: &I,
    f: F,
    hooks: &HookRegistry<I, O>,
) -> anyhow::Result<O>
where
    I: Send + Sync + 'static,
    O: Send + Sync + 'static,
    F: FnOnce() -> Fut,
    Fut: Future<Output = anyhow::Result<O>>,
{
    let started_at = crate::trace::node_start(node_id);
    hooks.before(node_id, input).await?;
    match f().await {
        Ok(output) => {
            hooks.after(node_id, input, &output).await?;
            crate::trace::node_end(node_id, started_at);
            Ok(output)
        }
        Err(error) => {
            hooks.on_error(node_id, input, &error).await?;
            crate::trace::node_error(node_id, started_at, &error);
            Err(error)
        }
    }
}

async fn run_node_attempt<I, O>(
    input: I,
    next: Next<I, O>,
    timeout: Option<Duration>,
) -> anyhow::Result<O>
where
    I: Send + Sync + 'static,
    O: Send + Sync + 'static,
{
    let future = next.run(input);
    if let Some(duration) = timeout {
        tokio::time::timeout(duration, future)
            .await
            .map_err(|_| anyhow::anyhow!("node timed out after {:?}", duration))?
    } else {
        future.await
    }
}

fn effective_node<I, O, F, Fut>(node_id: &str, f: F, hooks: &HookRegistry<I, O>) -> NodeFn<I, O>
where
    I: Send + Sync + 'static,
    O: Send + Sync + 'static,
    F: Fn(I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = anyhow::Result<O>> + Send + 'static,
{
    if hooks.disabled.contains(node_id) {
        return hooks
            .disabled_fallbacks
            .get(node_id)
            .cloned()
            .unwrap_or_else(|| {
                let node_id = node_id.to_owned();
                Arc::new(move |_input| {
                    let node_id = node_id.clone();
                    Box::pin(async move {
                        Err(anyhow::anyhow!(
                            "node `{}` is disabled and no fallback is registered",
                            node_id
                        ))
                    })
                })
            });
    }
    hooks
        .replacements
        .get(node_id)
        .cloned()
        .unwrap_or_else(|| node_fn(f))
}

fn around_chain<I, O>(node_id: &str, f: NodeFn<I, O>, hooks: &HookRegistry<I, O>) -> Next<I, O>
where
    I: Send + Sync + 'static,
    O: Send + Sync + 'static,
{
    let mut next = Next { call: f };
    for hook in hooks.around.get(node_id).into_iter().flatten().rev() {
        let hook = Arc::clone(hook);
        let previous = next;
        next = Next {
            call: Arc::new(move |input| {
                let hook = Arc::clone(&hook);
                let previous = previous.clone();
                Box::pin(async move { hook.call(input, previous).await })
            }),
        };
    }
    next
}

fn node_fn<I, O, F, Fut>(f: F) -> NodeFn<I, O>
where
    I: Send + Sync + 'static,
    O: Send + Sync + 'static,
    F: Fn(I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = anyhow::Result<O>> + Send + 'static,
{
    Arc::new(move |input| Box::pin(f(input)))
}

#[cfg(test)]
mod registry_tests;
