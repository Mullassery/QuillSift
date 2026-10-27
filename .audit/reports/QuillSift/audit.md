# Audit Report: QuillSift

**Audit Date:** 2026-10-26  
**Repository Path:** `/Users/georgimullassery/Desktop/Projects/QuillSift`  
**Audit Type:** Read-only systems design audit  
**Version:** 0.3.0 (workspace and Python package)  
**Branch:** `main`  
**Source Size:** ~6,574 Rust LOC, ~311 Python LOC; 9 crates; 112 Rust tests

## Executive Summary

QuillSift is a deterministic web-fetching and structured-extraction toolkit with a Rust core, Python bindings, CLI, bounded legacy crawler, and local-first MCP server. The security core is the strongest part: scheme allowlisting, literal-IP and DNS-level destination classification, redirect validation, private-network denial by default, response-size/time limits, and schema-validated LLM output. The Python bindings release the GIL around blocking work and share a multi-thread Tokio runtime.

The main gaps are verification breadth and platform claims. The Python package declares `abi3-py39`, but CI only builds/tests one Python version and the only Python test is a smoke script; the MCP wrapper has no automated test coverage. The legacy crawler is appropriately labelled, but HTML/URL parsing has no fuzzing. Build artifacts exist in the working tree (`dist/`, compiled `.so`, `__pycache__`) but were not tracked at audit time; they should stay out of releases.

## Purpose and Positioning

- **What it is:** a Rust/Python library and CLI for guarded fetching, HTML-to-markdown/text extraction, bounded crawling, JSON-schema-validated LLM extraction, and MCP exposure.
- **Who it is for:** AI agents, RAG pipelines, and developers who need deterministic scraping with an optional LLM step.
- **What it is not:** a browser, a JavaScript renderer, a full crawler platform, a hosted scraping service, or a substitute for MudFish on JS-heavy sites.
- **Differentiator:** deterministic-by-default extraction with an unusually explicit SSRF guard and schema validation at the LLM boundary.

## Architecture

```text
CLI / Python / MCP
  quillsift-cli, quillsift-python, python/quillsift/mcp_server.py
        |
        v
quillsift-core
  config: FetchPolicy, CrawlPolicy, RetryPolicy
  net: URL normalization and address classification
  traits/types: Extractor, Document, Extracted
        |
        +--> quillsift-fetch     guarded HTTP client (rustls, redirects, DNS)
        +--> quillsift-html      structural HTML -> text/markdown/links/images
        +--> quillsift-extract   deterministic + LLM schema extraction
        +--> quillsift-crawl     bounded same-origin legacy crawler
        +--> quillsift-graph     link/relationship graph
        +--> quillsift-llm       provider abstraction
        |
        v
quillsift-python
  PyO3 module: fetch, parse_html, extract, crawl
  shared Tokio runtime; py.detach around blocking calls
```

### Fetch path

```text
URL string
  -> parse and normalize
  -> scheme allowlist (http/https only)
  -> literal-IP classification
  -> DNS resolution through guarded resolver
  -> reject metadata/private/multicast/unspecified unless explicitly allowed
  -> HTTP request with timeout and size cap
  -> manual redirect validation at every hop
  -> HTML parse and extract
```

## Repository Layout

```text
crates/quillsift-core/      policies, errors, address classification, traits
crates/quillsift-fetch/     HTTP client, DNS guard, redirect handling
crates/quillsift-html/      structural extraction
crates/quillsift-extract/   deterministic and LLM schema extraction
crates/quillsift-crawl/     bounded legacy crawler and robots support
crates/quillsift-graph/     link graph
crates/quillsift-llm/       provider configuration/abstraction
crates/quillsift-cli/       CLI entry point
crates/quillsift-python/    PyO3 module, Python package, MCP server, smoke test
.github/workflows/ci.yml    fmt, clippy, test, wheel, deny, audit
```

## Technology Stack and Dependencies

- **Rust:** Tokio, reqwest/rustls, scraper/html5ever, serde/serde_json, url, thiserror/anyhow.
- **Python:** PyO3 abi3, FastMCP (optional runtime script), standard JSON/schema handling.
- **Build:** maturin for the Python extension; Cargo for the CLI/core.
- **Testing:** Rust unit/integration tests; one Python smoke script; no pytest suite.
- **CI:** fmt, clippy `-D warnings`, workspace tests, Python wheel build + smoke, cargo-deny, cargo-audit.

## Public API

Python (`quillsift`):

- `fetch(url, *, format="markdown", allow_private=False)`.
- `parse_html(html, *, base_url=..., format="markdown")`.
- `extract(...)` for schema-validated extraction.
- `crawl(...)` for the bounded legacy crawler.
- `__version__`.

Rust:

- `FetchPolicy`, `CrawlPolicy`, `RetryPolicy`.
- `Fetcher`, `StructuralExtractor`, `LlmExtractor`, `Crawler`.
- `AddressClass`, `block_reason`, `classify`, `is_cloud_metadata`, `normalize`.
- `Document`, `Extracted`, `Metadata`, `ScrapeError`/`ErrorKind`.

MCP:

- A FastMCP wrapper in `python/quillsift/mcp_server.py` exposing the same guarded operations to local agent runtimes.

## Design Patterns

- **Policy object:** fetch/crawl/retry behavior is explicit and configurable rather than scattered constants.
- **Guarded resolver:** DNS answers are classified before connection, closing the DNS-rebinding window.
- **Manual redirects:** each hop is revalidated instead of delegating policy to the HTTP client.
- **Deterministic-first extraction:** structural parser by default; LLM only when a schema is supplied.
- **Schema validation:** LLM output is validated against caller-provided JSON Schema.
- **Thin bindings:** Python exposes the same core operations as the CLI.
- **Legacy compatibility:** the crawler is retained but documented as a fallback to MudFish.

## Data and Storage

- No database or durable state.
- Fetch results are in-memory `Document`/`Extracted` values.
- Crawl state is in-memory and bounded by `CrawlPolicy`.
- Python wheel/sdist and compiled extension are build artifacts, not runtime state.
- No credential store; LLM keys come from environment/configuration.
- No cache was evident in the audited Python path.

## Concurrency and Performance

- Python bindings use a process-wide multi-thread Tokio runtime and `py.detach` for blocking fetch/extract/crawl work, so multiple Python threads can make progress concurrently.
- Fetching is async inside Rust; the Python API is synchronous by design.
- Crawl concurrency, depth, page count, and delay are policy-controlled; robots `Crawl-delay` is supported.
- Response-size and timeout limits bound individual fetches.
- No benchmark or load test was reviewed for large HTML, many links, or high-concurrency crawling.
- LLM extraction latency/cost depends on the configured provider; no batching or streaming evidence was reviewed.

## Security and Privacy

### Strengths

- **SSRF guard:** only `http`/`https`; literal IPs and DNS answers are classified; loopback/private/link-local/metadata/multicast/unspecified destinations are denied by default.
- **DNS rebinding resistance:** resolution goes through a guarded resolver rather than a separate check-then-connect step.
- **Redirect revalidation:** every redirect hop is checked, not just the initial URL.
- **Cloud metadata:** explicit metadata-address detection.
- **Resource limits:** timeouts and response-size caps.
- **Schema validation:** LLM output is validated before being returned as structured data.
- **Explicit opt-in:** `allow_private=True` is required for private destinations.
- **TLS:** rustls-based fetching.
- **No hardcoded credentials.**
- **Local-first MCP:** the wrapper is designed for local agent use, not a public multi-tenant endpoint.

### Gaps

- **MCP/remote exposure:** no authentication or authorization model is built into the MCP wrapper. If an HTTP transport is exposed beyond localhost, URL fetching and LLM calls become an open proxy/cost sink.
- **Prompt injection:** fetched pages are untrusted input to the LLM extractor. Schema validation constrains shape, not truthfulness or instruction-following attacks.
- **Private-network opt-in:** `allow_private=True` is intentionally powerful; callers must not expose it to untrusted prompts/agents without additional policy.
- **Parser attack surface:** HTML/URL parsing is not fuzzed in the audited CI.
- **No domain allowlist/denylist at the policy level** beyond private-address blocking; callers must add business-level restrictions.
- **No content-type allowlist** was evident; large/binary/non-HTML responses rely on size/time limits.
- **No audit trail** for fetched URLs or LLM calls.
- **No secret redaction** in errors/logs was verified.

## Reliability and Observability

- Structured `ScrapeError` with `ErrorKind` maps to Python exception types.
- Fetch policy includes timeouts, size limits, redirect limits, and retry policy.
- DNS/redirect failures are explicit rather than silently followed.
- Crawl policy bounds depth/pages/delay and honors robots.
- No metrics, tracing spans, health endpoint, or request ID propagation.
- No circuit breaker or global rate limiter for repeated fetches.
- LLM provider failures are surfaced, but no fallback/degradation policy was reviewed.
- No cache means repeated work is repeated.

## Testing, CI, and Quality

- 112 Rust tests; 9-crate workspace.
- Security tests appear strong for scheme/IP/DNS/redirect/metadata cases.
- Python coverage is one smoke script; there is no `pytest` suite and no direct MCP test.
- CI runs fmt, clippy, `cargo test --workspace`, a Python wheel build + smoke test, cargo-deny, and cargo-audit.
- Python CI uses one interpreter version (3.12), while the package declares `abi3-py39`; older/newer Python compatibility is not exercised.
- No fuzzing for URL normalization, HTML parsing, redirect handling, or schema validation.
- No coverage gate, load test, or live-web integration suite.
- No JS rendering tests because JS rendering is explicitly out of scope.

## Documentation, Packaging, and Release

- Workspace/package version is consistently 0.3.0.
- README clearly distinguishes deterministic extraction, bounded legacy crawling, and MudFish for JS-heavy pages.
- Security posture and SSRF behavior are documented better than most projects of this size.
- Python wheel and sdist artifacts exist in `crates/quillsift-python/dist/`; compiled `.so` and `__pycache__` also exist in the working tree. They were not tracked at audit time and should stay untracked.
- `py.typed` and a `.pyi` stub are present, which is a strength.
- Declared `abi3-py39` but only Python 3.12 is CI-tested.
- MCP server packaging depends on the optional `fastmcp` dependency; installation/transport guidance deserves a focused docs check.

## Strengths

- Real, defense-in-depth SSRF protection rather than a string blacklist.
- Redirect and DNS handling are deliberate and testable.
- Deterministic extraction by default; LLM is optional and schema-validated.
- GIL release makes the synchronous Python API usable from threaded programs.
- Clear crate separation and a small, focused API.
- Strong CI hygiene: clippy `-D warnings`, cargo-deny, cargo-audit, wheel smoke.
- Honest scope: crawler is legacy, JS rendering is out of scope.
- Type stubs and `py.typed` improve the Python contract.

## Risks and Gaps

| Risk | Severity | Evidence | Impact |
|---|---|---|---|
| MCP wrapper has no auth/rate limits | Medium | `python/quillsift/mcp_server.py`; local-first docs | Remote exposure becomes an open fetch/LLM proxy. |
| Prompt injection through fetched content | Medium | LLM extraction consumes untrusted HTML | Schema-valid but attacker-influenced structured data. |
| Python compatibility is under-tested | Medium | `abi3-py39` vs CI Python 3.12 only | Untested interpreters may fail despite the compatibility claim. |
| Python/MCP coverage is thin | Medium | Only `tests/smoke.py`; no pytest/MCP test | Regressions in bindings or MCP wiring can ship. |
| No parser fuzzing | Medium | CI has no fuzz targets | URL/HTML edge cases and panics are unproven. |
| `allow_private` is a powerful escape hatch | Low/Medium | `FetchPolicy` and Python kwarg | Misuse enables internal-network access. |
| No domain/content-type policy layer | Low | Policy review | Callers must add business restrictions themselves. |
| No fetch/LLM audit trail | Low | No logging/metrics evidence | Hard to investigate abuse or cost spikes. |
| Build artifacts in working tree | Low | `dist/`, `.so`, `__pycache__` present but untracked | Accidental commits/bloat if ignore rules regress. |
| Legacy crawler remains a supported surface | Low | `quillsift-crawl`, README | More code to maintain than the MudFish-preferred path. |

## Prioritized Recommendations

### P0 — exposure and trust boundaries

1. Document and enforce MCP transport defaults: stdio/localhost only, explicit remote opt-in, auth, request limits, and per-client rate limiting.
2. Treat fetched content as hostile in the LLM prompt: delimit content, instruct the model to ignore embedded instructions, and add output provenance/confidence metadata.
3. Keep `allow_private` disabled for any agent-facing surface and require an explicit, separate capability to enable it.

### P1 — verification breadth

4. Add a real Python pytest suite covering `fetch`, `parse_html`, `extract`, `crawl`, error mapping, and the MCP tool wrapper.
5. Build/test the wheel on Python 3.9–3.13, or raise the declared minimum to match CI.
6. Add fuzz targets for URL normalization, redirect handling, HTML parsing, and JSON-schema validation.
7. Add a live-web, opt-in integration suite with local fixtures for robots, redirects, and malformed HTML.

### P2 — operations and hygiene

8. Add domain/content-type allowlists and an optional fetch audit log with redaction.
9. Add metrics/tracing for fetch latency, blocked destinations, redirects, LLM calls, and schema failures.
10. Ensure `dist/`, `.so`, and `__pycache__` remain ignored; document the release artifact path.
11. Add a load test for concurrent fetches/crawls and large documents.

## Audit Limitations

- No live web fetching, LLM calls, or MCP transport tests were performed.
- SSRF protections were reviewed statically and through existing tests, not adversarially fuzzed.
- Python compatibility on 3.9–3.11 was not executed.
- MCP authentication/transport behavior was not exercised.
- Test counts are static definitions, not a verified pass count.
- Dependency advisories were not independently re-scanned.
