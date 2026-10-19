//! Node trait, typed node input/context, and closure-backed nodes.

use std::collections::BTreeMap;
use std::future::Future;

use async_trait::async_trait;
use serde_json::Value;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;

use scrape_core::{Result, ScrapeError};

/// Input handed to a node: the job's seed value plus direct-dependency outputs.
#[derive(Debug, Clone)]
pub struct NodeInput {
    seed: Value,
    deps: BTreeMap<String, Value>,
}

impl NodeInput {
    /// Creates an input containing only the seed value.
    pub fn new(seed: Value) -> Self {
        Self {
            seed,
            deps: BTreeMap::new(),
        }
    }

    pub(crate) fn with_deps(seed: Value, deps: BTreeMap<String, Value>) -> Self {
        Self { seed, deps }
    }

    /// Returns the seed value supplied to the whole execution.
    pub fn seed(&self) -> &Value {
        &self.seed
    }

    /// Returns all direct-dependency outputs, keyed by node name.
    pub fn deps(&self) -> &BTreeMap<String, Value> {
        &self.deps
    }

    /// Returns one dependency output, if present.
    pub fn dep(&self, name: &str) -> Option<&Value> {
        self.deps.get(name)
    }

    /// Returns one dependency output or a typed error when it is missing.
    ///
    /// # Errors
    /// Returns [`ScrapeError::Extraction`] when `name` is not a dependency.
    pub fn require_dep(&self, name: &str) -> Result<&Value> {
        self.deps
            .get(name)
            .ok_or_else(|| ScrapeError::Extraction(format!("missing dependency output `{name}`")))
    }
}

/// Per-node execution context.
#[derive(Debug, Clone)]
pub struct NodeContext {
    name: String,
    cancellation: CancellationToken,
    deadline: Option<Instant>,
}

impl NodeContext {
    pub(crate) fn new(
        name: String,
        cancellation: CancellationToken,
        deadline: Option<Instant>,
    ) -> Self {
        Self {
            name,
            cancellation,
            deadline,
        }
    }

    /// Returns the node's name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Returns `true` once cancellation has been requested.
    pub fn is_cancelled(&self) -> bool {
        self.cancellation.is_cancelled()
    }

    /// Resolves when cancellation is requested.
    pub async fn cancelled(&self) {
        self.cancellation.cancelled().await;
    }

    /// Returns the absolute wall-clock deadline, if one was configured.
    pub fn deadline(&self) -> Option<Instant> {
        self.deadline
    }

    /// Returns [`ScrapeError::Cancelled`] if cancellation was requested.
    ///
    /// Long-running nodes should call this at safe points.
    ///
    /// # Errors
    /// Returns [`ScrapeError::Cancelled`] when the caller cancelled the job.
    pub fn check_cancelled(&self) -> Result<()> {
        if self.is_cancelled() {
            Err(ScrapeError::Cancelled)
        } else {
            Ok(())
        }
    }
}

/// A unit of work in a [`crate::Graph`].
///
/// Nodes exchange JSON [`Value`]s so heterogeneous stages can be composed into
/// one runtime graph while each node keeps its own domain types internally.
#[async_trait]
pub trait Node: Send + Sync {
    /// Stable, unique name used for edges, logs and result keys.
    fn name(&self) -> &str;

    /// Runs the node. Retries and cancellation are handled by the executor.
    ///
    /// # Errors
    /// Returning a retryable [`ScrapeError`] (transport/timeout) may cause the
    /// executor to retry according to the configured [`scrape_core::RetryPolicy`].
    async fn run(&self, ctx: &NodeContext, input: NodeInput) -> Result<Value>;
}

/// A [`Node`] backed by an async closure, useful for adapters and tests.
pub struct FnNode<F> {
    name: String,
    func: F,
}

impl<F> FnNode<F> {
    /// Wraps `func` in a node named `name`.
    pub fn new(name: impl Into<String>, func: F) -> Self {
        Self {
            name: name.into(),
            func,
        }
    }
}

#[async_trait]
impl<F, Fut> Node for FnNode<F>
where
    F: Fn(NodeContext, NodeInput) -> Fut + Send + Sync,
    Fut: Future<Output = Result<Value>> + Send + 'static,
{
    fn name(&self) -> &str {
        &self.name
    }

    async fn run(&self, ctx: &NodeContext, input: NodeInput) -> Result<Value> {
        (self.func)(ctx.clone(), input).await
    }
}
