//! Graph construction, validation and bounded execution.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use serde_json::Value;
use tokio::sync::Semaphore;
use tokio::task::JoinSet;
use tokio::time::Instant;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

use scrape_core::{Result, RetryPolicy, ScrapeError};

use crate::error::GraphError;
use crate::node::{Node, NodeContext, NodeInput};
use crate::retry;

type NodeTask = (String, Result<Value>);

/// Options controlling one graph execution.
#[derive(Debug, Clone)]
pub struct ExecuteOptions {
    /// Token used to cancel the job; cancelled tokens abort in-flight nodes.
    pub cancellation: CancellationToken,
    /// Optional wall-clock budget for the whole job.
    pub timeout: Option<Duration>,
    /// Abort on the first node failure (`true`) or record it and continue
    /// independent branches (`false`).
    pub fail_fast: bool,
}

impl Default for ExecuteOptions {
    fn default() -> Self {
        Self {
            cancellation: CancellationToken::new(),
            timeout: None,
            fail_fast: true,
        }
    }
}

impl ExecuteOptions {
    /// Creates default options.
    pub fn new() -> Self {
        Self::default()
    }

    /// Sets the wall-clock budget.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    /// Sets fail-fast behaviour.
    #[must_use]
    pub fn with_fail_fast(mut self, fail_fast: bool) -> Self {
        self.fail_fast = fail_fast;
        self
    }

    /// Uses the given cancellation token.
    #[must_use]
    pub fn with_cancellation(mut self, token: CancellationToken) -> Self {
        self.cancellation = token;
        self
    }
}

/// The result of a completed (non-fail-fast) execution.
#[derive(Debug)]
pub struct ExecutionReport {
    /// Successful node outputs, ordered by node name.
    pub outputs: BTreeMap<String, Value>,
    /// Node failures, ordered by node name.
    pub failures: BTreeMap<String, ScrapeError>,
    /// Nodes that never ran because a dependency failed.
    pub skipped: BTreeSet<String>,
    /// Wall-clock (or virtual, under paused time) duration.
    pub elapsed: Duration,
}

impl ExecutionReport {
    /// Returns `true` when every node succeeded.
    pub fn is_success(&self) -> bool {
        self.failures.is_empty() && self.skipped.is_empty()
    }

    /// Returns a node output by name.
    pub fn output(&self, name: &str) -> Option<&Value> {
        self.outputs.get(name)
    }
}

/// A validated directed acyclic graph of [`Node`]s.
pub struct Graph {
    nodes: BTreeMap<String, Arc<dyn Node>>,
    dependencies: BTreeMap<String, Vec<String>>,
    dependents: BTreeMap<String, Vec<String>>,
    in_degree: BTreeMap<String, usize>,
    entries: Vec<String>,
    max_concurrency: usize,
    retry: RetryPolicy,
}

impl std::fmt::Debug for Graph {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Graph")
            .field("nodes", &self.nodes.keys().collect::<Vec<_>>())
            .field("entries", &self.entries)
            .field("max_concurrency", &self.max_concurrency)
            .finish_non_exhaustive()
    }
}

impl Graph {
    /// Starts a builder.
    pub fn builder() -> GraphBuilder {
        GraphBuilder::new()
    }

    /// Returns the number of nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Returns the configured concurrency limit.
    pub fn max_concurrency(&self) -> usize {
        self.max_concurrency
    }

    /// Executes the graph with `seed` as the input to every root node.
    ///
    /// A node receives the seed plus the outputs of its direct dependencies.
    /// Independent nodes run concurrently under the configured limit and
    /// deadline.
    ///
    /// # Errors
    /// Returns [`GraphError::Cancelled`] or [`GraphError::DeadlineExceeded`]
    /// when the job is stopped early, and [`GraphError::NodeFailed`] on the
    /// first failure when `fail_fast` is enabled.
    pub async fn execute(
        &self,
        seed: Value,
        options: &ExecuteOptions,
    ) -> std::result::Result<ExecutionReport, GraphError> {
        let started = Instant::now();
        let cancellation = options.cancellation.clone();
        if cancellation.is_cancelled() {
            return Err(GraphError::Cancelled);
        }
        let deadline = options.timeout.map(|budget| started + budget);

        let mut remaining = self.in_degree.clone();
        let mut outputs: BTreeMap<String, Value> = BTreeMap::new();
        let mut failures: BTreeMap<String, ScrapeError> = BTreeMap::new();
        let mut ready: BTreeSet<String> = self.entries.iter().cloned().collect();
        let mut tasks: JoinSet<NodeTask> = JoinSet::new();
        let permits = Arc::new(Semaphore::new(self.max_concurrency));

        loop {
            while let Some(name) = ready.pop_first() {
                let Ok(permit) = Arc::clone(&permits).try_acquire_owned() else {
                    ready.insert(name);
                    break;
                };
                let node = Arc::clone(&self.nodes[&name]);
                let deps = self.dependencies[&name]
                    .iter()
                    .filter_map(|dep| outputs.get(dep).map(|value| (dep.clone(), value.clone())))
                    .collect();
                let input = NodeInput::with_deps(seed.clone(), deps);
                let ctx = NodeContext::new(name.clone(), cancellation.clone(), deadline);
                let retry = self.retry.clone();
                let task_name = name.clone();
                tasks.spawn(async move {
                    let _permit = permit;
                    let span = tracing::info_span!("node", name = %task_name);
                    let result = run_with_retry(&node, &ctx, input, &retry)
                        .instrument(span)
                        .await;
                    (task_name, result)
                });
            }

            if tasks.is_empty() {
                break;
            }

            let completed = tokio::select! {
                biased;
                () = cancellation.cancelled() => {
                    abort_all(&mut tasks).await;
                    return Err(GraphError::Cancelled);
                }
                () = wait_for_deadline(deadline) => {
                    abort_all(&mut tasks).await;
                    return Err(GraphError::DeadlineExceeded(
                        options.timeout.unwrap_or_default(),
                    ));
                }
                joined = tasks.join_next() => joined,
            };

            match completed {
                Some(Ok((name, Ok(value)))) => {
                    outputs.insert(name.clone(), value);
                    for dependent in &self.dependents[&name] {
                        if let Some(count) = remaining.get_mut(dependent) {
                            *count = count.saturating_sub(1);
                            if *count == 0 {
                                ready.insert(dependent.clone());
                            }
                        }
                    }
                }
                Some(Ok((name, Err(err)))) => {
                    if matches!(&err, ScrapeError::Cancelled) {
                        abort_all(&mut tasks).await;
                        return Err(GraphError::Cancelled);
                    }
                    if options.fail_fast {
                        abort_all(&mut tasks).await;
                        return Err(GraphError::NodeFailed {
                            node: name,
                            source: err,
                        });
                    }
                    failures.insert(name, err);
                }
                Some(Err(join_err)) => {
                    abort_all(&mut tasks).await;
                    return Err(GraphError::Join(join_err.to_string()));
                }
                None => break,
            }
        }

        let skipped = self
            .nodes
            .keys()
            .filter(|name| !outputs.contains_key(*name) && !failures.contains_key(*name))
            .cloned()
            .collect();
        Ok(ExecutionReport {
            outputs,
            failures,
            skipped,
            elapsed: started.elapsed(),
        })
    }
}

async fn run_with_retry(
    node: &Arc<dyn Node>,
    ctx: &NodeContext,
    input: NodeInput,
    policy: &RetryPolicy,
) -> Result<Value> {
    let max_attempts = policy.max_attempts.max(1);
    let mut attempt = 0;
    loop {
        attempt += 1;
        ctx.check_cancelled()?;
        match node.run(ctx, input.clone()).await {
            Ok(value) => return Ok(value),
            Err(err) => {
                if !err.is_retryable() || attempt >= max_attempts {
                    return Err(err);
                }
                let delay = retry::backoff(policy, attempt, node.name());
                tokio::select! {
                    biased;
                    () = ctx.cancelled() => return Err(ScrapeError::Cancelled),
                    () = tokio::time::sleep(delay) => {}
                }
            }
        }
    }
}

async fn wait_for_deadline(deadline: Option<Instant>) {
    match deadline {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending::<()>().await,
    }
}

async fn abort_all(tasks: &mut JoinSet<NodeTask>) {
    tasks.abort_all();
    while tasks.join_next().await.is_some() {}
}

/// Builder for [`Graph`].
#[derive(Default)]
pub struct GraphBuilder {
    nodes: Vec<Arc<dyn Node>>,
    edges: Vec<(String, String)>,
    entries: Vec<String>,
    max_concurrency: Option<usize>,
    retry: RetryPolicy,
}

impl GraphBuilder {
    /// Creates an empty builder with default concurrency (4) and retry policy.
    pub fn new() -> Self {
        Self {
            nodes: Vec::new(),
            edges: Vec::new(),
            entries: Vec::new(),
            max_concurrency: None,
            retry: RetryPolicy::default(),
        }
    }

    /// Adds a node. Node names must be unique within a graph.
    #[must_use]
    pub fn node(mut self, node: Arc<dyn Node>) -> Self {
        self.nodes.push(node);
        self
    }

    /// Adds a directed edge `from -> to`.
    #[must_use]
    pub fn edge(mut self, from: impl Into<String>, to: impl Into<String>) -> Self {
        self.edges.push((from.into(), to.into()));
        self
    }

    /// Marks a node as an entry point. If no entries are set, every node with
    /// no dependencies is an entry.
    #[must_use]
    pub fn entry(mut self, name: impl Into<String>) -> Self {
        self.entries.push(name.into());
        self
    }

    /// Sets the maximum number of concurrently running nodes.
    #[must_use]
    pub fn max_concurrency(mut self, limit: usize) -> Self {
        self.max_concurrency = Some(limit);
        self
    }

    /// Sets the retry policy applied to each node.
    #[must_use]
    pub fn retry(mut self, policy: RetryPolicy) -> Self {
        self.retry = policy;
        self
    }

    /// Validates and builds the graph.
    ///
    /// # Errors
    /// Returns [`GraphError::InvalidGraph`] for duplicate names/edges, unknown
    /// nodes, a zero concurrency limit, an invalid retry policy or inconsistent
    /// entries, and [`GraphError::Cycle`] if the edges form a cycle.
    pub fn build(self) -> std::result::Result<Graph, GraphError> {
        if self.nodes.is_empty() {
            return Err(GraphError::InvalidGraph("graph has no nodes".into()));
        }
        let max_concurrency = self.max_concurrency.unwrap_or(4);
        if max_concurrency == 0 {
            return Err(GraphError::InvalidGraph(
                "max_concurrency must be greater than zero".into(),
            ));
        }
        self.retry
            .validate()
            .map_err(|err| GraphError::InvalidGraph(err.to_string()))?;

        let nodes = collect_nodes(self.nodes)?;
        let (dependencies, dependents, in_degree) = index_edges(&nodes, &self.edges)?;
        ensure_acyclic(&nodes, &dependents, &in_degree)?;
        let entries = resolve_entries(&nodes, &in_degree, self.entries)?;

        Ok(Graph {
            nodes,
            dependencies,
            dependents,
            in_degree,
            entries,
            max_concurrency,
            retry: self.retry,
        })
    }
}

fn collect_nodes(
    nodes: Vec<Arc<dyn Node>>,
) -> std::result::Result<BTreeMap<String, Arc<dyn Node>>, GraphError> {
    let mut map = BTreeMap::new();
    for node in nodes {
        let name = node.name().to_owned();
        if map.insert(name.clone(), node).is_some() {
            return Err(GraphError::InvalidGraph(format!("duplicate node `{name}`")));
        }
    }
    Ok(map)
}

type EdgeIndex = (
    BTreeMap<String, Vec<String>>,
    BTreeMap<String, Vec<String>>,
    BTreeMap<String, usize>,
);

fn index_edges(
    nodes: &BTreeMap<String, Arc<dyn Node>>,
    edges: &[(String, String)],
) -> std::result::Result<EdgeIndex, GraphError> {
    let unique: BTreeSet<(String, String)> = edges.iter().cloned().collect();
    if unique.len() != edges.len() {
        return Err(GraphError::InvalidGraph("duplicate edge".into()));
    }

    let mut dependencies: BTreeMap<String, Vec<String>> = nodes
        .keys()
        .map(|name| (name.clone(), Vec::new()))
        .collect();
    let mut dependents = dependencies.clone();
    let mut in_degree: BTreeMap<String, usize> =
        nodes.keys().map(|name| (name.clone(), 0)).collect();

    for (from, to) in unique {
        if !nodes.contains_key(&from) {
            return Err(GraphError::InvalidGraph(format!(
                "edge references unknown node `{from}`"
            )));
        }
        if !nodes.contains_key(&to) {
            return Err(GraphError::InvalidGraph(format!(
                "edge references unknown node `{to}`"
            )));
        }
        *in_degree.get_mut(&to).expect("degree entry exists") += 1;
        dependents
            .get_mut(&from)
            .expect("dependents entry exists")
            .push(to.clone());
        dependencies
            .get_mut(&to)
            .expect("dependencies entry exists")
            .push(from);
    }
    for list in dependencies.values_mut() {
        list.sort();
    }
    for list in dependents.values_mut() {
        list.sort();
    }
    Ok((dependencies, dependents, in_degree))
}

fn ensure_acyclic(
    nodes: &BTreeMap<String, Arc<dyn Node>>,
    dependents: &BTreeMap<String, Vec<String>>,
    in_degree: &BTreeMap<String, usize>,
) -> std::result::Result<(), GraphError> {
    let mut degree = in_degree.clone();
    let mut queue: VecDeque<String> = degree
        .iter()
        .filter(|(_, value)| **value == 0)
        .map(|(name, _)| name.clone())
        .collect();
    let mut visited = 0;
    while let Some(name) = queue.pop_front() {
        visited += 1;
        for dependent in &dependents[&name] {
            let value = degree.get_mut(dependent).expect("degree entry exists");
            *value -= 1;
            if *value == 0 {
                queue.push_back(dependent.clone());
            }
        }
    }
    if visited == nodes.len() {
        Ok(())
    } else {
        Err(GraphError::Cycle)
    }
}

fn resolve_entries(
    nodes: &BTreeMap<String, Arc<dyn Node>>,
    in_degree: &BTreeMap<String, usize>,
    requested: Vec<String>,
) -> std::result::Result<Vec<String>, GraphError> {
    let roots: Vec<String> = nodes
        .keys()
        .filter(|name| in_degree[*name] == 0)
        .cloned()
        .collect();
    if requested.is_empty() {
        return Ok(roots);
    }

    let mut explicit = BTreeSet::new();
    for name in requested {
        if !nodes.contains_key(&name) {
            return Err(GraphError::InvalidGraph(format!(
                "entry references unknown node `{name}`"
            )));
        }
        if in_degree[&name] != 0 {
            return Err(GraphError::InvalidGraph(format!(
                "entry `{name}` has dependencies"
            )));
        }
        explicit.insert(name);
    }
    let root_set: BTreeSet<String> = roots.into_iter().collect();
    if explicit != root_set {
        return Err(GraphError::InvalidGraph(
            "entries must include every node with no dependencies".into(),
        ));
    }
    Ok(explicit.into_iter().collect())
}
