use async_trait::async_trait;
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

struct Task<F> {
    task: F,
}

#[async_trait]
impl<I, O, F, Fut> Runnable<I, O> for Task<F>
where
    I: Send + 'static,
    O: Send + 'static,
    F: Fn(I) -> Fut + Send + Sync,
    Fut: Future<Output = anyhow::Result<O>> + Send,
{
    async fn run(&self, input: I) -> anyhow::Result<O> {
        (self.task)(input).await
    }
}

struct Then<A, B> {
    first: A,
    second: B,
}

#[async_trait]
impl<I, M, O> Runnable<I, O> for Then<Workflow<I, M>, Workflow<M, O>>
where
    I: Send + 'static,
    M: Send + 'static,
    O: Send + 'static,
{
    async fn run(&self, input: I) -> anyhow::Result<O> {
        let middle = self.first.run(input).await?;
        self.second.run(middle).await
    }
}

struct Branch<P, A, B, C> {
    first: A,
    predicate: P,
    then_flow: B,
    else_flow: C,
}

#[async_trait]
impl<I, M, O, P> Runnable<I, O> for Branch<P, Workflow<I, M>, Workflow<M, O>, Workflow<M, O>>
where
    I: Send + 'static,
    M: Send + 'static,
    O: Send + 'static,
    P: Fn(&M) -> bool + Send + Sync,
{
    async fn run(&self, input: I) -> anyhow::Result<O> {
        let middle = self.first.run(input).await?;
        if (self.predicate)(&middle) {
            self.then_flow.run(middle).await
        } else {
            self.else_flow.run(middle).await
        }
    }
}

struct Parallel<A, B> {
    left: A,
    right: B,
}

#[async_trait]
impl<I, L, R> Runnable<I, (L, R)> for Parallel<Workflow<I, L>, Workflow<I, R>>
where
    I: Clone + Send + 'static,
    L: Send + 'static,
    R: Send + 'static,
{
    async fn run(&self, input: I) -> anyhow::Result<(L, R)> {
        let right_input = input.clone();
        let (left, right) = tokio::try_join!(self.left.run(input), self.right.run(right_input))?;
        Ok((left, right))
    }
}

struct Retry<W> {
    workflow: W,
    attempts: usize,
}

#[async_trait]
impl<I, O> Runnable<I, O> for Retry<Workflow<I, O>>
where
    I: Clone + Send + 'static,
    O: Send + 'static,
{
    async fn run(&self, input: I) -> anyhow::Result<O> {
        let attempts = self.attempts.max(1);
        let mut last_error = None;
        for _ in 0..attempts {
            match self.workflow.run(input.clone()).await {
                Ok(output) => return Ok(output),
                Err(error) => last_error = Some(error),
            }
        }
        Err(last_error.expect("retry attempts are always at least one"))
    }
}

struct Timeout<W> {
    workflow: W,
    duration: Duration,
}

#[async_trait]
impl<I, O> Runnable<I, O> for Timeout<Workflow<I, O>>
where
    I: Send + 'static,
    O: Send + 'static,
{
    async fn run(&self, input: I) -> anyhow::Result<O> {
        tokio::time::timeout(self.duration, self.workflow.run(input))
            .await
            .map_err(|_| anyhow::anyhow!("workflow timed out after {:?}", self.duration))?
    }
}

/// A state value used by a context-backed workflow.
///
/// The terminal state should return `true` from [`WorkflowState::is_end`].
pub trait WorkflowState {
    /// Whether this state ends workflow execution.
    fn is_end(&self) -> bool;
}

/// A workflow implementation style for state machines with private context.
///
/// This trait is intentionally an implementation detail behind a public
/// `Workflow<I, O>` contract. Nodes mutate `Context` and return the next state;
/// `Output` is assembled once at the end.
#[async_trait]
pub trait ContextWorkflow: Send + Sync {
    /// Public input accepted by [`ContextWorkflow::run`].
    type Input: Send + 'static;
    /// Public output returned by [`ContextWorkflow::run`].
    type Output: Send + 'static;
    /// Internal mutable state shared by workflow nodes.
    type Context: Send;
    /// Control-flow state for the workflow.
    type State: WorkflowState + Send;

    /// Build the initial context from public input.
    fn context(&self, input: Self::Input) -> Self::Context;

    /// Initial state for the workflow state machine.
    fn initial_state(&self) -> Self::State;

    /// Execute one node/state and return the next state.
    async fn step(
        &self,
        state: Self::State,
        context: &mut Self::Context,
    ) -> anyhow::Result<Self::State>;

    /// Assemble the final public output from context.
    fn output(&self, context: Self::Context) -> anyhow::Result<Self::Output>;

    /// Run the workflow to completion.
    async fn run(&self, input: Self::Input) -> anyhow::Result<Self::Output> {
        let mut context = self.context(input);
        let mut state = self.initial_state();
        loop {
            if state.is_end() {
                break;
            }
            state = self.step(state, &mut context).await?;
        }
        self.output(context)
    }

    /// Convert this context-backed implementation into a composable workflow.
    fn into_workflow(self) -> Workflow<Self::Input, Self::Output>
    where
        Self: Sized + 'static,
    {
        Workflow::new(ContextRunnable { workflow: self })
    }
}

struct ContextRunnable<W> {
    workflow: W,
}

#[async_trait]
impl<W> Runnable<W::Input, W::Output> for ContextRunnable<W>
where
    W: ContextWorkflow + 'static,
{
    async fn run(&self, input: W::Input) -> anyhow::Result<W::Output> {
        self.workflow.run(input).await
    }
}

#[cfg(test)]
mod tests;
