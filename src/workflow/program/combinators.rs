use super::{Runnable, Workflow};
use async_trait::async_trait;
use std::future::Future;
use std::time::Duration;

pub(super) struct Task<F> {
    pub(super) task: F,
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

pub(super) struct Then<A, B> {
    pub(super) first: A,
    pub(super) second: B,
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

pub(super) struct Branch<P, A, B, C> {
    pub(super) first: A,
    pub(super) predicate: P,
    pub(super) then_flow: B,
    pub(super) else_flow: C,
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

pub(super) struct Parallel<A, B> {
    pub(super) left: A,
    pub(super) right: B,
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

pub(super) struct Retry<W> {
    pub(super) workflow: W,
    pub(super) attempts: usize,
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

pub(super) struct Timeout<W> {
    pub(super) workflow: W,
    pub(super) duration: Duration,
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
