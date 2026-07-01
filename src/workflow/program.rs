use async_trait::async_trait;
mod combinators;
mod context;

use combinators::{Branch, Parallel, Retry, Task, Then, Timeout};
pub use context::{ContextWorkflow, WorkflowState};

use std::future::Future;
use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;

/// A typed executable unit.
///
/// `Runnable<I, O>` is the common contract for tasks, tools, workflows, and
/// sub-workflows. A workflow can therefore be used anywhere a node is expected.
#[async_trait]
pub trait Runnable<I, O>: Send + Sync {
    /// Execute this unit with strongly typed input and output.
    async fn run(&self, input: I) -> anyhow::Result<O>;
}

/// A typed, composable workflow.
///
/// The public contract is always `I -> O`. A workflow may use private context
/// internally, but that context is not part of cross-workflow composition.
#[derive(Clone)]
pub struct Workflow<I, O> {
    runner: Arc<dyn Runnable<I, O>>,
    _marker: PhantomData<fn(I) -> O>,
}

impl<I, O> Workflow<I, O> {
    /// Build a workflow from any runnable task, tool, or sub-workflow.
    pub fn new<R>(runner: R) -> Self
    where
        R: Runnable<I, O> + 'static,
    {
        Self {
            runner: Arc::new(runner),
            _marker: PhantomData,
        }
    }

    /// Build a workflow from an async function or closure.
    pub fn task<F, Fut>(task: F) -> Self
    where
        I: Send + 'static,
        O: Send + 'static,
        F: Fn(I) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = anyhow::Result<O>> + Send + 'static,
    {
        Self::new(Task { task })
    }

    /// Compose two workflows in sequence.
    pub fn then<N>(self, next: Workflow<O, N>) -> Workflow<I, N>
    where
        I: Send + 'static,
        O: Send + 'static,
        N: Send + 'static,
    {
        Workflow::new(Then {
            first: self,
            second: next,
        })
    }

    /// Route this workflow's output into one of two next workflows.
    pub fn branch<N, P>(
        self,
        predicate: P,
        then_flow: Workflow<O, N>,
        else_flow: Workflow<O, N>,
    ) -> Workflow<I, N>
    where
        I: Send + 'static,
        O: Send + 'static,
        N: Send + 'static,
        P: Fn(&O) -> bool + Send + Sync + 'static,
    {
        Workflow::new(Branch {
            first: self,
            predicate,
            then_flow,
            else_flow,
        })
    }

    /// Run two workflows with the same input and return both outputs.
    pub fn parallel<P>(self, other: Workflow<I, P>) -> Workflow<I, (O, P)>
    where
        I: Clone + Send + 'static,
        O: Send + 'static,
        P: Send + 'static,
    {
        Workflow::new(Parallel {
            left: self,
            right: other,
        })
    }

    /// Retry this workflow up to `attempts` times.
    pub fn retry(self, attempts: usize) -> Workflow<I, O>
    where
        I: Clone + Send + 'static,
        O: Send + 'static,
    {
        Workflow::new(Retry {
            workflow: self,
            attempts,
        })
    }

    /// Fail this workflow when it does not complete before `duration`.
    pub fn timeout(self, duration: Duration) -> Workflow<I, O>
    where
        I: Send + 'static,
        O: Send + 'static,
    {
        Workflow::new(Timeout {
            workflow: self,
            duration,
        })
    }
}

#[async_trait]
impl<I, O> Runnable<I, O> for Workflow<I, O>
where
    I: Send + 'static,
    O: Send + 'static,
{
    async fn run(&self, input: I) -> anyhow::Result<O> {
        self.runner.run(input).await
    }
}

#[cfg(test)]
mod tests;
