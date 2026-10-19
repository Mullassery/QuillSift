//! Graph construction and execution errors.

use std::time::Duration;

use thiserror::Error;

use scrape_core::ScrapeError;

/// Failures that prevent a graph from being built or executed.
#[derive(Debug, Error)]
#[non_exhaustive]
pub enum GraphError {
    /// The graph definition is structurally invalid.
    #[error("invalid graph: {0}")]
    InvalidGraph(String),

    /// The graph contains a directed cycle.
    #[error("graph contains a cycle")]
    Cycle,

    /// The caller cancelled execution.
    #[error("execution cancelled")]
    Cancelled,

    /// The configured wall-clock deadline elapsed.
    #[error("execution deadline exceeded after {0:?}")]
    DeadlineExceeded(Duration),

    /// A node failed and the graph is configured to fail fast.
    #[error("node `{node}` failed: {source}")]
    NodeFailed {
        /// Name of the failed node.
        node: String,
        /// Underlying error.
        #[source]
        source: ScrapeError,
    },

    /// An executor task panicked or was aborted unexpectedly.
    #[error("node task failed: {0}")]
    Join(String),
}
