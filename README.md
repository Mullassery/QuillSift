# ScrapeOwl

> **Status: Phase 4 / M2 (typed graph engine).** The Rust workspace contains
> `scrape-core` (types, errors, policies, SSRF address classes), `scrape-html`
> (metadata, text, markdown, links, images), `scrape-fetch` (SSRF-guarded HTTP
> with redirect, size and timeout limits) and `scrape-graph` (bounded DAG
> execution with deadlines, cancellation, retries and failure isolation). CI
> (fmt, clippy, test, `cargo-deny`, `cargo-audit`) passes. There is **no LLM,
> crawl, browser, API, CLI or MCP functionality yet.** Nothing here is a claim
> of feature parity or production readiness.

ScrapeOwl is a planned **Rust-native, self-hosted AI scraping platform**:
a typed, secure-by-default engine for deterministic HTML extraction and
optional LLM-powered structured extraction, with a REST API, CLI and MCP
server. It is an independent, clean-room reimplementation inspired by the
public API concepts of [ScrapeGraphAI](https://github.com/ScrapeGraphAI/Scrapegraph-ai).

ScrapeOwl is **not affiliated with, endorsed by, or derived from** the
ScrapeGraphAI source code. See [NOTICE.md](./NOTICE.md).

## Goals

- Deterministic fetch → parse → extract that works with **no LLM, no browser
  and no paid credentials**.
- Typed graph pipelines, bounded concurrency, deadlines and cancellation.
- SSRF-safe fetching, response limits, and no execution of downloaded content.
- Provider-neutral structured LLM extraction (OpenAI-compatible + Ollama).
- Clear separation between this self-hosted engine and any managed service.

## Repository contents (current)

```
crates/
├── scrape-core/                      # domain types, config policies, errors, SSRF classes
├── scrape-fetch/                     # SSRF-guarded HTTP client (redirect/size/time limits)
├── scrape-html/                      # metadata, text, markdown, links, images
└── scrape-graph/                     # bounded DAG executor (deadline/cancel/retry)
docs/
├── UPSTREAM_AUDIT.md                 # read-only audit of ScrapeGraphAI v2.3.1
├── FEATURE_PARITY_MATRIX.md          # per-feature goals, priorities, out-of-scope
├── RUST_ARCHITECTURE.md              # proposed workspace, deps, milestones
├── DEPENDENCY_AND_LICENSE_AUDIT.md   # upstream deps + Rust adoption gate
└── SECURITY_THREAT_MODEL.md          # threats and required mitigations
NOTICE.md                             # upstream attribution (MIT)
deny.toml                             # cargo-deny license/advisory policy
```

`scrape-core` provides `ScrapeError`/`ErrorKind` (with retry classification),
`FetchPolicy`, `RetryPolicy`, `CrawlPolicy` (validated, secure defaults),
`Document`/`Metadata`/`Extracted`/`Extractor`, and SSRF address classification.
The other crates build on it: `scrape-fetch` (guarded HTTP), `scrape-html`
(deterministic extraction) and `scrape-graph` (typed DAG execution).

## Development

```bash
cargo test --workspace      # unit tests
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check
cargo deny check            # licenses + advisories
cargo audit
```

## Roadmap (see RUST_ARCHITECTURE.md for acceptance criteria)

M0 workspace (done) · M1 deterministic fetch/parse (done) · M2 typed graph
engine (done) · M3 LLM extraction + CLI · M4 crawl/search · M5 optional browser ·
M6 API/MCP · M7 hardening/benchmarks · M8 docs/release.

## Open items

- Confirm the project name **ScrapeOwl** passes a trademark/naming check.
- Confirm MSRV and the MCP SDK choice.

## License

Apache-2.0 — see [LICENSE](./LICENSE). Upstream ScrapeGraphAI is MIT-licensed;
attribution is recorded in [NOTICE.md](./NOTICE.md).
