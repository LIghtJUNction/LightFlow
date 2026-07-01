use super::{Runnable, Workflow};
use async_trait::async_trait;

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
