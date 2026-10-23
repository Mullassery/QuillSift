# QuillSift — Rust Architecture (proposed)

Status: **living document**. M0 (`quillsift-core`), M1 (`quillsift-fetch`,
`quillsift-html`), M2 (`quillsift-graph`), M3 (`quillsift-llm`, `quillsift-extract`,
`quillsift-cli`) and M4a (`quillsift-crawl`, legacy compatibility fallback) are
implemented; later milestones are still proposals. Crawling and browser
rendering beyond that fallback are delegated to MudFish. This document states
intended boundaries, dependencies and acceptance criteria.
Dependency names are *candidates* until recorded as adopted in
[DEPENDENCY_AND_LICENSE_AUDIT.md](./DEPENDENCY_AND_LICENSE_AUDIT.md).

## 1. Principles

1. **Deterministic core, optional intelligence.** Fetching, parsing, metadata,
   markdown and link extraction must work with **no LLM and no browser** and no
   external service.
2. **Typed everything.** Config, node I/O and errors are Rust types, validated
   at construction. No dynamic string-keyed state with runtime resolution.
3. **Secure by default.** SSRF protection, response limits, bounded
   concurrency/time, and no code execution are defaults, not options.
4. **Observable.** `tracing` spans per node, metrics, and a typed result
   envelope.
5. **Small, reviewable milestones.** Each milestone has explicit scope and
   acceptance criteria and lands with tests.

## 2. Workspace layout

```
QuillSift/
├── Cargo.toml                # workspace
├── crates/
│   ├── quillsift-core/          # domain types, errors, config, traits
│   ├── quillsift-fetch/         # HTTP, timeouts, redirects, SSRF, limits
│   ├── quillsift-html/          # parse, metadata, links, content, markdown
│   ├── quillsift-llm/           # provider abstraction + structured generation
│   ├── quillsift-graph/         # typed DAG + scheduler/executor
│   ├── quillsift-extract/       # schema-driven extraction + validation
│   ├── quillsift-crawl/         # legacy bounded crawler + dedup (compatibility)
│   ├── quillsift-api/           # REST service + middleware (binary)
│   ├── quillsift-cli/           # CLI (binary)
│   ├── quillsift-mcp/           # MCP server (binary)
│   └── quillsift-python/        # PyO3 bindings (PyPI package `quillsift`)
├── docs/
└── tests/                    # cross-crate integration (local fixture server)
```

Crate boundaries (rationale):

- `quillsift-core` has no network/async-heavy deps; everything else depends on it.
- `quillsift-fetch` owns all HTTP and the SSRF/limit policy; nothing else opens
  sockets except `quillsift-llm`, which reuses the same address-policy types from
  `quillsift-core`.
- `quillsift-html` is pure (bytes/str in → typed structure out); easily fuzzed.
- `quillsift-graph` depends on `quillsift-core` only and drives nodes via traits, so
  pipelines are testable with mock nodes.
- Browser rendering is not implemented in QuillSift; use MudFish's opt-in
  `render_js` path instead of adding a browser dependency here.
- Binaries (`quillsift-api`, `quillsift-cli`, `quillsift-mcp`) are thin; logic lives in
  libraries.

We deliberately avoid splitting further (no per-provider crates in the first
cut) to prevent micro-crate sprawl.

## 3. Candidate dependencies

All candidates must pass the adoption gate (§5) before use. MSRV target:
**latest stable Rust** (pin an explicit MSRV once the toolchain is chosen).

| Concern | Candidates | Notes |
|---|---|---|
| Async runtime | `tokio` | LTS-ish, widely used |
| HTTP | `reqwest` (rustls) | TLS via `rustls`; no OpenSSL by default |
| HTML parsing | `scraper` (`html5ever`) / `lol_html` | Streaming option for large docs |
| HTML→Markdown | `htmd` | Verify maintenance/license |
| Serialization | `serde`, `serde_json` | |
| JSON Schema | `schemars` | Typed schema generation |
| Validation | `jsonschema` | Runtime validation of LLM output |
| Errors | `thiserror` | Typed error enums |
| Async traits | `async-trait` (only where needed) | Prefer native async fn in traits if stable enough |
| Tracing | `tracing`, `tracing-subscriber` | |
| Web | `axum`, `tower`, `tower-http` | API server |
| MCP | `rmcp` (SDK) | Verify maturity before adoption |
| CLI | `clap` | |
| IP/CIDR policy | `ipnet` / `std::net` | SSRF ranges |
| URL | `url` | Canonicalization |
| Hashing | `sha2` | Content dedup |
| Tokens | `tiktoken-rs` (verify) | Only if token budgets needed |
| Fuzzing | `cargo-fuzz` (dev) | Parsers/URL handling |
| TLS root store | `webpki-roots` / `rustls-native-certs` | |
| Time | `tokio::time` | Deadlines/timeouts |

Dev/CI tooling: `cargo fmt`, `clippy` (deny warnings), `cargo test`,
`cargo-deny` (licenses/advisories), `cargo-audit`, `cargo-fuzz`.

## 4. Core types (sketch)

```rust
// quillsift-core
pub struct FetchPolicy {
    pub max_bytes: u64,
    pub max_decompressed_bytes: u64,
    pub max_redirects: u8,
    pub connect_timeout: Duration,
    pub total_timeout: Duration,
    pub user_agent: String,
    pub headers: HeaderMap,
    pub allow_private_networks: bool, // false by default; trusted deployments only
}

pub enum FetchError {
    DnsBlocked(String),
    AddressBlocked(IpAddr),
    TooManyRedirects,
    TooLarge,
    Timeout,
    Http(u16),
    Transport(String),
}

pub struct Document { pub url: Url, pub html: String, pub fetched_at: SystemTime, pub status: u16 }
pub struct Extracted { pub text: String, pub markdown: String, pub links: Vec<Url>,
                       pub images: Vec<Url>, pub metadata: Metadata }
```

Graph nodes implement a trait with typed input/output:

```rust
// quillsift-graph
#[async_trait]
pub trait Node<I, O>: Send + Sync {
    fn name(&self) -> &'static str;
    async fn run(&self, ctx: &NodeContext, input: I) -> Result<O, NodeError>;
}
```

The DAG is built from typed node handles with explicit edges; conditional
edges are typed functions over the node output (no evaluation of string
expressions over untrusted state).

## 5. Dependency adoption gate

Before adding any dependency:

1. Verify it is actively maintained (recent releases/commits).
2. Verify license is OSI-approved and compatible (prefer MIT/Apache-2.0).
3. Record license + version in `DEPENDENCY_AND_LICENSE_AUDIT.md`.
4. Prefer crates with no network/FFI side effects for parsing crates.
5. Run `cargo-deny` and `cargo-audit` in CI.

## 6. Security controls (enforced in code, not docs)

- **SSRF:** resolve DNS ourselves, reject loopback/private/link-local/multicast/
  unique-local/unspecified/cloud-metadata (169.254.169.254), IPv4-mapped IPv6,
  and re-check every redirect hop. Reject if resolution or connection target
  differs (rebinding). `allow_private_networks` only via explicit trusted config.
- **Limits:** response bytes, decompressed bytes, redirects, crawl depth/pages,
  concurrency, wall-clock deadline.
- **No script execution** of downloaded content; browser is optional and
  isolated.
- **No secret leakage:** redact credentials/tokens from logs and errors; never
  send secrets to LLMs.
- **Prompt injection:** page content is delimited untrusted data; system policy
  is fixed; tool/action permissions are explicit and bounded.

## 7. Execution engine (quillsift-graph)

- Explicit DAG: nodes + typed edges + entry set.
- Dependency-aware scheduling: independent nodes run concurrently under a
  `Semaphore` and a global deadline.
- Cancellation via `CancellationToken`.
- Retry by error category with bounded exponential backoff + jitter.
- Per-node `tracing` span and metrics; typed `NodeError` preserves failure
  isolation (a node failure is data, not an unconditional abort, subject to
  policy).
- Deterministic output ordering where promised (e.g. multi-page results keep
  input order regardless of completion order).

## 8. Testing strategy

- Unit tests for every parser, scheduler and validator.
- Integration tests against the **local fixture server** (ported concept from
  upstream's `tests/fixtures/mock_server`, reimplemented): HTML/JSON/XML/CSV,
  `/slow`, `/error/*`, `/rate-limited`, `/dynamic`, `/pagination`, plus SSRF
  fixtures (redirect-to-localhost, DNS-rebind, oversized, gzip bomb, malformed
  HTML).
- Mocked LLM provider for generation tests; deterministic no-LLM tests never
  touch the network.
- Fuzz targets for `quillsift-html` parsing and URL/canonicalization.
- Reproducible benchmarks with published inputs/method/hardware.

## 9. Milestones & acceptance criteria

Each milestone is a separate PR-sized change. Tests must pass and results
reported before the next milestone.

### M0 — Workspace skeleton (Phase 1) — **done**
- Scope: workspace + `quillsift-core` types/errors/config; CI (fmt, clippy, test,
  deny, audit); README/ARCHITECTURE stubs.
- Acceptance: `cargo test` green; `cargo clippy -- -D warnings` clean;
  `cargo deny check` clean.
- Result (2026-10-20): 14 unit tests pass; clippy clean; `cargo deny check` ok;
  `cargo audit` exit 0. License set to Apache-2.0.

### M1 — Deterministic fetch + parse (Phase 2) — **done (core), fuzz pending**
- Scope: `quillsift-fetch` (reqwest + SSRF + limits), `quillsift-html` (parse,
  metadata, links, images, markdown, canonicalization), local inputs, typed
  errors, `tracing`.
- Acceptance: fixture-server integration tests (success, timeout, redirect,
  oversized, gzip, malformed); SSRF tests (loopback/private/metadata/IPv4-mapped
  IPv6/redirect-chain/rebind); no panics on malformed input; fuzz smoke.
- Result (2026-10-20): 55 tests pass across the workspace (23 core, 6 fetch
  unit, 11 fixture-server integration, 15 html). Implemented: manual per-hop
  redirect validation, connection-time guarded DNS resolver (rebind defence),
  literal-IP checks, compressed-size cap, timeouts, and malformed-HTML tests.
  Fuzz targets are **not yet wired** (`cargo-fuzz` is an M1 follow-up, not a
  blocker for M2).

### M2 — Typed graph engine (Phase 4 subset) — **done**
- Scope: `quillsift-graph` DAG, scheduler, cancellation/deadlines, retry policy,
  per-node tracing, mock nodes.
- Acceptance: concurrency-limit, cancellation, deadline and retry tests with a
  deterministic scheduler test harness.
- Result (2026-10-20): 71 workspace tests pass (23 core, 17 fetch, 15 html,
  15 graph, 1 doc). Graph tests use paused time for deterministic deadline,
  cancellation, retry and concurrency assertions. Implemented: Kahn cycle
  detection, duplicate/unknown-edge validation, bounded `Semaphore` scheduling,
  deadline + `CancellationToken`, fail-fast vs. continue failure isolation,
  retry with bounded deterministic jitter, and per-node `tracing` spans.

### M3 — LLM extraction (Phase 3) + CLI (Phase 6 subset) — **done**
- Scope: `quillsift-llm` (OpenAI-compatible + Ollama adapters), `quillsift-extract`
  (schemars→jsonschema validation, bounded repair), `quillsift-cli`.
- Acceptance: mocked-provider tests for structured output, retries, rate limit,
  cancellation; injection tests proving page text cannot change policy;
  schema-validation tests; deterministic no-LLM path tested without a provider.
- Result (2026-10-20): 107 tests pass (24 core, 19 fetch, 15 html, 16 graph,
  9 llm, 17 extract, 7 CLI). Provider tests use a local capture server and
  verify paths, auth headers, request bodies, JSON mode and SSRF blocking;
  extraction tests cover fences, schema-violation repair, bounded attempts,
  injected-document containment, truncation, schemars generation and config
  rejection. The CLI exposes `fetch` (markdown/text/json, no LLM) and `extract`
  (provider, base URL, model, schema, env-based API key, `--allow-private`).
  Python bindings (`quillsift-python`, PyPI package `quillsift`) expose blocking
  `fetch`/`extract` functions that release the GIL and reuse the same guarded
  fetcher and extractor; a wheel smoke test runs against a local HTTP server.
  Not covered: live provider calls (no credentials in CI) and token counting.

### M4a — Bounded crawl (Phase 5 subset) — **done, legacy**
- Status: kept for CLI/Python compatibility with bug fixes only. MudFish is the
  supported crawler and browser-rendering path; do not add crawl features to
  QuillSift.
- Scope: `quillsift-crawl` (bounded BFS, same-origin default, robots enforced by
  default with opt-out, URL dedup, crawl delay, concurrency), CLI `crawl` and
  Python `crawl`.
- Acceptance: crawl-depth/page-limit/same-origin/dedup/robots tests against the
  fixture server; polite-rate test.
- Result (2026-10-22): 122 workspace tests pass, including 6 robots/unit tests
  and 8 fixture-server crawl tests. Implemented: breadth-first traversal with
  deterministic ordering, URL-fragment dedup, depth/page caps, same-origin
  scope (including redirect checks), per-origin crawl delay, bounded
  concurrency, `robots.txt` user-agent groups/`Allow`/`Disallow`/`Crawl-delay`
  with `*` and `$` support, deny-on-unavailable robots behavior, bounded frontier
  growth, and non-fatal per-page failures. The CLI exposes
  `crawl`; the Python wheel exposes `crawl`, exercised by the wheel smoke test.
- Not covered: content-hash dedup, resumable crawls and search providers.

### M4b — Search providers (Phase 5 subset) — planned
- Scope: pluggable search provider abstraction plus a fan-out pipeline. Use
  MudFish when the fan-out requires crawling or JavaScript rendering.
- Acceptance: provider-mocked search → fetch → extract tests, output caps,
  provider errors and a documented credential/ToS policy.
- Deferred: no search backend is selected yet. Shipping a default scraper for a
  third-party search engine would be fragile and may violate its terms.

### M5 — Browser adapter — **superseded**
- QuillSift does not implement browser rendering. Use MudFish's opt-in
  `render_js` path, which owns headless-Chrome lifecycle, rendering and its
  documented SSRF caveat.
- QuillSift remains browser-free so the deterministic path stays small.

### M6 — API + MCP (Phase 6)
- Scope: `quillsift-api` (scrape/extract, health/ready, request ids, typed errors,
  API-key auth, quotas/rate limits, max job duration); `quillsift-mcp` (validated
  args, timeouts, output caps). Crawl remains a legacy CLI/Python fallback, not
  part of the new API surface.
- Acceptance: auth failure, quota, rate-limit, job-duration, and error-envelope
  tests; MCP argument-validation and output-cap tests.

### M7 — Reliability/security hardening + benchmarks (Phases 7–8)
- Scope: full fuzzing sweep, `cargo-deny`/audit in CI, benchmark harness and
  published results.
- Acceptance: documented fuzz runs, clean deny/audit, reproducible benchmark
  report with raw inputs.

### M8 — Docs + release quality (Phase 9)
- Scope: README/ARCHITECTURE/CONTRIBUTING/SECURITY/CODE_OF_CONDUCT/CHANGELOG,
  docs set, `NOTICE.md`, license decisions, naming/trademark check.
- Acceptance: docs review; working quick start executed on a clean checkout.

## 10. Open decisions

1. **QuillSift trademark clearance** (release blocker). Preliminary package,
   repository and domain availability checks were run on 2026-10-23; formal
   trademark clearance has not been performed.
2. **MSRV** and whether `async-trait` is needed or native async traits suffice.
3. Exact **MCP SDK** choice (`rmcp` vs alternative) after maturity check.

Resolved: license is Apache-2.0 (see `LICENSE`); PyO3 Python bindings are in
scope and implemented in `quillsift-python`.
