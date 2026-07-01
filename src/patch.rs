//! Typed hook and patch primitives for code-first workflows.

use async_trait::async_trait;
use std::collections::{BTreeMap, BTreeSet};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

mod execution;
pub use execution::{run_node, run_node_borrowed, run_node_with_fallback};

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
