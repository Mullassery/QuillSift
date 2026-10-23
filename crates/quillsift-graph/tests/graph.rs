use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use serde_json::json;
use tokio_util::sync::CancellationToken;

use quillsift_core::{RetryPolicy, ScrapeError};
use quillsift_graph::{ExecuteOptions, FnNode, Graph, GraphError, Node, NodeContext, NodeInput};

fn strict_retry(max_attempts: u32) -> RetryPolicy {
    RetryPolicy {
        max_attempts,
        initial_backoff: Duration::from_millis(10),
        max_backoff: Duration::from_millis(100),
        jitter: false,
    }
}

fn constant_node(name: &'static str, value: serde_json::Value) -> Arc<dyn Node> {
    Arc::new(FnNode::new(
        name,
        move |_ctx: NodeContext, _input: NodeInput| {
            let value = value.clone();
            async move { Ok(value) }
        },
    ))
}

fn failing_node(name: &'static str, message: &'static str) -> Arc<dyn Node> {
    Arc::new(FnNode::new(
        name,
        move |_ctx: NodeContext, _input: NodeInput| async move {
            Err(ScrapeError::Config(message.to_owned()))
        },
    ))
}

fn tracking_node(
    name: &'static str,
    active: Arc<AtomicUsize>,
    peak: Arc<AtomicUsize>,
    millis: u64,
) -> Arc<dyn Node> {
    Arc::new(FnNode::new(
        name,
        move |_ctx: NodeContext, _input: NodeInput| {
            let active = Arc::clone(&active);
            let peak = Arc::clone(&peak);
            async move {
                let current = active.fetch_add(1, Ordering::SeqCst) + 1;
                peak.fetch_max(current, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(millis)).await;
                active.fetch_sub(1, Ordering::SeqCst);
                Ok(json!(name))
            }
        },
    ))
}

#[tokio::test]
async fn linear_graph_passes_dependency_outputs() {
    let seed = json!({ "n": 1 });
    let first = Arc::new(FnNode::new(
        "first",
        |_ctx: NodeContext, input: NodeInput| {
            let seed = input.seed().clone();
            async move { Ok(json!({ "seen": seed })) }
        },
    ));
    let second = Arc::new(FnNode::new(
        "second",
        |_ctx: NodeContext, input: NodeInput| async move {
            let forwarded = input.require_dep("first")?["seen"].clone();
            Ok(json!({ "echo": forwarded, "seed": input.seed() }))
        },
    ));

    let graph = Graph::builder()
        .node(first)
        .node(second)
        .edge("first", "second")
        .build()
        .unwrap();

    let report = graph
        .execute(seed.clone(), &ExecuteOptions::new())
        .await
        .unwrap();
    assert!(report.is_success());
    assert_eq!(report.output("first"), Some(&json!({ "seen": seed })));
    assert_eq!(
        report.output("second").unwrap()["echo"],
        seed,
        "dependency output is forwarded"
    );
    assert_eq!(report.output("second").unwrap()["seed"], seed);
    assert_eq!(report.skipped.len(), 0);
}

#[tokio::test(start_paused = true)]
async fn concurrency_is_bounded_by_the_limit() {
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let mut builder = Graph::builder().max_concurrency(2);
    for name in ["a", "b", "c", "d"] {
        builder = builder.node(tracking_node(
            name,
            Arc::clone(&active),
            Arc::clone(&peak),
            50,
        ));
    }
    let graph = builder.build().unwrap();

    let report = graph
        .execute(json!(null), &ExecuteOptions::new())
        .await
        .unwrap();
    assert!(report.is_success());
    assert_eq!(report.outputs.len(), 4);
    assert_eq!(peak.load(Ordering::SeqCst), 2);
    assert_eq!(active.load(Ordering::SeqCst), 0);
}

#[tokio::test(start_paused = true)]
async fn concurrency_limit_one_is_sequential() {
    let active = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let a = tracking_node("a", Arc::clone(&active), Arc::clone(&peak), 10);
    let b = tracking_node("b", Arc::clone(&active), Arc::clone(&peak), 10);
    let graph = Graph::builder()
        .node(a)
        .node(b)
        .max_concurrency(1)
        .build()
        .unwrap();

    graph
        .execute(json!(null), &ExecuteOptions::new())
        .await
        .unwrap();
    assert_eq!(peak.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn deadline_stops_long_jobs() {
    let slow = Arc::new(FnNode::new(
        "slow",
        |_ctx: NodeContext, _input: NodeInput| async move {
            tokio::time::sleep(Duration::from_secs(3600)).await;
            Ok(json!(null))
        },
    ));
    let graph = Graph::builder().node(slow).build().unwrap();

    let err = graph
        .execute(
            json!(null),
            &ExecuteOptions::new().with_timeout(Duration::from_secs(5)),
        )
        .await
        .unwrap_err();
    match err {
        GraphError::DeadlineExceeded(budget) => assert_eq!(budget, Duration::from_secs(5)),
        other => panic!("unexpected error: {other}"),
    }
}

#[tokio::test(start_paused = true)]
async fn cancellation_returns_cancelled() {
    let waiting = Arc::new(FnNode::new(
        "waiting",
        |ctx: NodeContext, _input: NodeInput| async move {
            ctx.cancelled().await;
            Err(ScrapeError::Cancelled)
        },
    ));
    let graph = Graph::builder().node(waiting).build().unwrap();

    let token = CancellationToken::new();
    let trigger = token.clone();
    let handle = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        trigger.cancel();
    });

    let err = graph
        .execute(json!(null), &ExecuteOptions::new().with_cancellation(token))
        .await
        .unwrap_err();
    assert!(matches!(err, GraphError::Cancelled));
    handle.await.unwrap();
}

#[tokio::test(start_paused = true)]
async fn retries_retryable_errors_until_success() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&attempts);
    let flaky = Arc::new(FnNode::new(
        "flaky",
        move |_ctx: NodeContext, _input: NodeInput| {
            let counter = Arc::clone(&counter);
            async move {
                let attempt = counter.fetch_add(1, Ordering::SeqCst) + 1;
                if attempt < 3 {
                    Err(ScrapeError::Transport("temporary".into()))
                } else {
                    Ok(json!(attempt))
                }
            }
        },
    ));

    let graph = Graph::builder()
        .node(flaky)
        .retry(strict_retry(3))
        .build()
        .unwrap();
    let report = graph
        .execute(json!(null), &ExecuteOptions::new())
        .await
        .unwrap();
    assert_eq!(report.output("flaky"), Some(&json!(3)));
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
}

#[tokio::test(start_paused = true)]
async fn non_retryable_errors_are_not_retried() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&attempts);
    let node = Arc::new(FnNode::new(
        "bad_config",
        move |_ctx: NodeContext, _input: NodeInput| {
            let counter = Arc::clone(&counter);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Err(ScrapeError::Config("invalid".into()))
            }
        },
    ));
    let graph = Graph::builder()
        .node(node)
        .retry(strict_retry(5))
        .build()
        .unwrap();

    let err = graph
        .execute(json!(null), &ExecuteOptions::new())
        .await
        .unwrap_err();
    match err {
        GraphError::NodeFailed { node, source } => {
            assert_eq!(node, "bad_config");
            assert!(matches!(source, ScrapeError::Config(_)));
        }
        other => panic!("unexpected error: {other}"),
    }
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn retries_stop_after_max_attempts() {
    let attempts = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&attempts);
    let node = Arc::new(FnNode::new(
        "always_fails",
        move |_ctx: NodeContext, _input: NodeInput| {
            let counter = Arc::clone(&counter);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Err(ScrapeError::Transport("down".into()))
            }
        },
    ));
    let graph = Graph::builder()
        .node(node)
        .retry(strict_retry(3))
        .build()
        .unwrap();

    let err = graph
        .execute(json!(null), &ExecuteOptions::new())
        .await
        .unwrap_err();
    assert!(matches!(err, GraphError::NodeFailed { .. }));
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
}

#[tokio::test(start_paused = true)]
async fn fail_fast_aborts_sibling_nodes() {
    let failing = failing_node("failing", "nope");
    let slow = Arc::new(FnNode::new(
        "slow",
        |_ctx: NodeContext, _input: NodeInput| async move {
            tokio::time::sleep(Duration::from_secs(3600)).await;
            Ok(json!(null))
        },
    ));
    let graph = Graph::builder().node(failing).node(slow).build().unwrap();

    let started = tokio::time::Instant::now();
    let err = graph
        .execute(json!(null), &ExecuteOptions::new())
        .await
        .unwrap_err();
    assert!(matches!(err, GraphError::NodeFailed { .. }), "{err}");
    assert!(
        started.elapsed() < Duration::from_secs(1),
        "sibling should be aborted rather than awaited"
    );
}

#[tokio::test(start_paused = true)]
async fn continue_mode_records_failure_and_skips_dependents() {
    let failing = failing_node("failing", "nope");
    let healthy = constant_node("healthy", json!("ok"));
    let child = constant_node("child", json!("never"));

    let graph = Graph::builder()
        .node(failing)
        .node(healthy)
        .node(child)
        .edge("failing", "child")
        .retry(strict_retry(1))
        .build()
        .unwrap();

    let report = graph
        .execute(json!(null), &ExecuteOptions::new().with_fail_fast(false))
        .await
        .unwrap();
    assert!(!report.is_success());
    assert!(report.failures.contains_key("failing"));
    assert_eq!(report.output("healthy"), Some(&json!("ok")));
    assert!(report.skipped.contains("child"));
    assert!(!report.outputs.contains_key("child"));
}

#[tokio::test]
async fn cycles_are_rejected() {
    let a = constant_node("a", json!(1));
    let b = constant_node("b", json!(2));
    let err = Graph::builder()
        .node(a)
        .node(b)
        .edge("a", "b")
        .edge("b", "a")
        .build()
        .unwrap_err();
    assert!(matches!(err, GraphError::Cycle), "{err}");
}

#[tokio::test]
async fn structural_errors_are_rejected() {
    let a = constant_node("a", json!(1));
    let b = constant_node("b", json!(2));

    let unknown = Graph::builder()
        .node(constant_node("a", json!(1)))
        .edge("a", "missing")
        .build()
        .unwrap_err();
    assert!(matches!(unknown, GraphError::InvalidGraph(_)));

    let duplicate = Graph::builder()
        .node(constant_node("a", json!(1)))
        .node(constant_node("a", json!(2)))
        .build()
        .unwrap_err();
    assert!(matches!(duplicate, GraphError::InvalidGraph(_)));

    let duplicate_edge = Graph::builder()
        .node(constant_node("a", json!(1)))
        .node(constant_node("b", json!(2)))
        .edge("a", "b")
        .edge("a", "b")
        .build()
        .unwrap_err();
    assert!(matches!(duplicate_edge, GraphError::InvalidGraph(_)));

    let incomplete_entries = Graph::builder()
        .node(a)
        .node(b)
        .entry("a")
        .build()
        .unwrap_err();
    assert!(matches!(incomplete_entries, GraphError::InvalidGraph(_)));

    let zero_concurrency = Graph::builder()
        .node(constant_node("a", json!(1)))
        .max_concurrency(0)
        .build()
        .unwrap_err();
    assert!(matches!(zero_concurrency, GraphError::InvalidGraph(_)));

    let no_nodes = Graph::builder().build().unwrap_err();
    assert!(matches!(no_nodes, GraphError::InvalidGraph(_)));
}
