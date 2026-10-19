//! # scrape-graph
//!
//! A typed, bounded DAG execution engine.
//!
//! Nodes are `Send + Sync` units that exchange `serde_json::Value`s. The engine
//! validates the graph (unknown edges, duplicates, cycles), runs independent
//! nodes concurrently under a [`tokio::sync::Semaphore`], enforces a wall-clock
//! deadline, supports cancellation and retries retryable errors with bounded
//! exponential backoff.
//!
//! ```
//! use std::sync::Arc;
//! use serde_json::json;
//! use scrape_graph::{ExecuteOptions, FnNode, Graph, NodeContext, NodeInput};
//!
//! # #[tokio::main]
//! # async fn main() {
//! let fetch = Arc::new(FnNode::new(
//!     "fetch",
//!     |_ctx: NodeContext, input: NodeInput| async move {
//!         Ok(json!({ "body": input.seed() }))
//!     },
//! ));
//! let parse = Arc::new(FnNode::new(
//!     "parse",
//!     |_ctx: NodeContext, input: NodeInput| async move {
//!         let body = input.require_dep("fetch")?["body"].clone();
//!         Ok(body)
//!     },
//! ));
//! let graph = Graph::builder()
//!     .node(fetch)
//!     .node(parse)
//!     .edge("fetch", "parse")
//!     .build()
//!     .unwrap();
//!
//! let report = graph
//!     .execute(json!("<html></html>"), &ExecuteOptions::new())
//!     .await
//!     .unwrap();
//! assert_eq!(report.output("parse"), Some(&json!("<html></html>")));
//! # }
//! ```

#![forbid(unsafe_code)]
#![warn(missing_docs)]

mod error;
mod graph;
mod node;
pub mod retry;

pub use error::GraphError;
pub use graph::{ExecuteOptions, ExecutionReport, Graph, GraphBuilder};
pub use node::{FnNode, Node, NodeContext, NodeInput};
