# ScrapeOwl — Rust Architecture (proposed)

Status: **proposal for review (end of Phase 0)**. No crate is implemented yet.
This document states intended boundaries, dependencies and acceptance criteria.
Dependency names are *candidates*; adoption is gated on the checks in
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
ScrapeOwl/
├── Cargo.toml                # workspace
├── crates/
│   ├── scrape-core/          # domain types, errors, config, traits
│   ├── scrape-fetch/         # HTTP, timeouts, redirects, SSRF, limits
│   ├── scrape-html/          # parse, metadata, links, content, markdown
│   ├── scrape-llm/           # provider abstraction + structured generation
│   ├── scrape-graph/         # typed DAG + scheduler/executor
│   ├── scrape-extract/       # schema-driven extraction + validation
│   ├── scrape-crawl/         # bounded crawler + dedup
│   ├── scrape-browser/       # OPTIONAL JS rendering adapter (feature-gated)
│   ├── scrape-api/           # REST service + middleware (binary)
│   ├── scrape-cli/           # CLI (binary)
│   ├── scrape-mcp/           # MCP server (binary)
│   └── scrape-python/        # OPTIONAL PyO3 bindings (later)
├── docs/
└── tests/                    # cross-crate integration (local fixture server)
```

Crate boundaries (rationale):

- `scrape-core` has no network/async-heavy deps; everything else depends on it.
- `scrape-fetch` owns all HTTP and the SSRF/limit policy; nothing else opens
  sockets except `scrape-browser` and `scrape-llm`, which reuse the same
  address-policy types from `scrape-core`.
- `scrape-html` is pure (bytes/str in → typed structure out); easily fuzzed.
- `scrape-graph` depends on `scrape-core` only and drives nodes via traits, so
  pipelines are testable with mock nodes.
- `scrape-browser` is feature-gated (`browser`) and never a default dependency.
- Binaries (`scrape-api`, `scrape-cli`, `scrape-mcp`) are thin; logic lives in
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
// scrape-core
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
// scrape-graph
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

## 7. Execution engine (scrape-graph)

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
- Fuzz targets for `scrape-html` parsing and URL/canonicalization.
- Reproducible benchmarks with published inputs/method/hardware.

## 9. Milestones & acceptance criteria

Each milestone is a separate PR-sized change. Tests must pass and results
reported before the next milestone.

### M0 — Workspace skeleton (Phase 1) — **done**
- Scope: workspace + `scrape-core` types/errors/config; CI (fmt, clippy, test,
  deny, audit); README/ARCHITECTURE stubs.
- Acceptance: `cargo test` green; `cargo clippy -- -D warnings` clean;
  `cargo deny check` clean.
- Result (2026-10-20): 14 unit tests pass; clippy clean; `cargo deny check` ok;
  `cargo audit` exit 0. License set to Apache-2.0.

### M1 — Deterministic fetch + parse (Phase 2)
- Scope: `scrape-fetch` (reqwest + SSRF + limits), `scrape-html` (parse,
  metadata, links, images, markdown, canonicalization), local inputs, typed
  errors, `tracing`.
- Acceptance: fixture-server integration tests (success, timeout, redirect,
  oversized, gzip, malformed); SSRF tests (loopback/private/metadata/IPv4-mapped
  IPv6/redirect-chain/rebind); no panics on malformed input; fuzz smoke.

### M2 — Typed graph engine (Phase 4 subset)
- Scope: `scrape-graph` DAG, scheduler, cancellation/deadlines, retry policy,
  per-node tracing, mock nodes.
- Acceptance: concurrency-limit, cancellation, deadline and retry tests with a
  deterministic scheduler test harness.

### M3 — LLM extraction (Phase 3) + CLI (Phase 6 subset)
- Scope: `scrape-llm` (OpenAI-compatible + Ollama adapters), `scrape-extract`
  (schemars→jsonschema validation, bounded repair), `scrape-cli`.
- Acceptance: mocked-provider tests for structured output, retries, rate limit,
  cancellation; injection tests proving page text cannot change policy;
  schema-validation tests; deterministic no-LLM path tested without a provider.

### M4 — Crawl + search (Phase 5)
- Scope: `scrape-crawl` (bounded, same-origin default, robots opt-in, dedup,
  content dedup), pluggable search provider.
- Acceptance: crawl-depth/page-limit/same-origin/dedup/robots tests against the
  fixture server; polite-rate test.

### M5 — Browser adapter (optional) (Phase 5)
- Scope: `scrape-browser` feature-gated JS rendering, SSRF applied to
  navigation/iframes/subresources, TLS always verified, no stealth.
- Acceptance: rendered-page test behind the feature flag; SSRF navigation
  tests; test proving core builds/tests without the feature.

### M6 — API + MCP (Phase 6)
- Scope: `scrape-api` (scrape/extract/crawl, health/ready, request ids, typed
  errors, API-key auth, quotas/rate limits, max job duration); `scrape-mcp`
  (validated args, timeouts, output caps).
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

## 10. Open decisions to confirm before M0

1. **Project name & trademark check** for "ScrapeOwl" (release blocker).
2. ~~License~~ — **decided: Apache-2.0** (see `LICENSE`).
3. **MSRV** and whether `async-trait` is needed or native async traits suffice.
4. Exact **MCP SDK** choice (`rmcp` vs alternative) after maturity check.
5. Whether `scrape-python` (PyO3) is in scope for the first year.
