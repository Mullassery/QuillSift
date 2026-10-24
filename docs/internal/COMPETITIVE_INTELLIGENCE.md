# Competitive intelligence

## OSS gap analysis — 2026-10-24

**What matters most here:** settle the browser boundary and prove extraction quality. QuillSift deliberately delegates JavaScript rendering to MudFish, so it should either document that integration as the supported path or own a renderer, then publish a reproducible precision/recall and throughput comparison against Trafilatura. Without evidence, "deterministic and Rust-fast" is a positioning claim, not a buying reason.

Scope: QuillSift 0.3.0 (Rust workspace, Apache-2.0). Implemented today, per README, `docs/FEATURE_PARITY_MATRIX.md`, `docs/RUST_ARCHITECTURE.md` and `Cargo.toml`: guarded HTTP fetch (SSRF, redirect/size/time caps), deterministic HTML→text/Markdown/links/images/metadata, a typed bounded DAG executor, OpenAI-compatible and Ollama extraction with JSON Schema validation and bounded repair, a legacy bounded BFS crawler, a CLI, and PyO3 Python bindings. Not implemented: browser/JS rendering, REST API, MCP server, search providers, local CSV/PDF/JSON/XML pipeline inputs, multi-page fan-out, benchmarks. Alpha status is stated in the README.

### Comparable OSS projects
| Project | GitHub | License | Last activity | Why it is the comparison |
|---|---|---|---|---|
| Trafilatura | https://github.com/adbar/trafilatura | Apache-2.0 | Pushed 2026-10-23; actively released | Closest deterministic comparator: rule-based main-text/metadata extraction, crawling, Markdown/JSON/CSV/XML output, no LLM and no JS. Directly substitutes for our `fetch`/crawl path. |
| Crawl4AI | https://github.com/unclecode/crawl4ai | Apache-2.0 | Pushed 2026-10-22; created 2024, frequent releases | LLM-oriented crawler with Playwright rendering, Markdown, CSS/XPath and LLM schema extraction. Overlaps our crawl + schema-extraction positioning; the 2024-era entrant users compare against. |
| Jina Reader | https://github.com/jina-ai/reader | Apache-2.0 | Pushed 2026-05-22; open-source branch re-synced 2026-04 | URL→LLM-friendly Markdown service with headless-Chrome/curl engine choice, PDF/Office input, search and screenshots. The hosted baseline users weigh against self-hosting. |

### Capability comparison
| Capability | Ours | Trafilatura | Crawl4AI | Jina Reader | Gap |
|---|---|---|---|---|---|
| No-browser deterministic extraction | Yes | Yes | No | Partial | Parity with Trafilatura; ahead of browser-first tools on cost/no-JS paths |
| JavaScript rendering | No | No | Yes | Yes | Absent; delegated to MudFish, so users needing SPAs must add a second tool |
| Schema-validated LLM extraction | Yes | No | Yes | No | Parity with Crawl4AI; ahead of the deterministic-only tools |
| Main-content extraction | Yes | Yes | Yes | Yes | Parity; our quality is unbenchmarked against Trafilatura |
| Metadata (title/description/etc.) | Yes | Yes | Partial | Partial | Parity/slight lead |
| Markdown output | Yes | Yes | Yes | Yes | Parity |
| Bounded multi-page crawl | Partial | Yes | Yes | Partial | Weaker: legacy fallback only, no resumable/content dedup |
| SSRF/redirect/size hardening | Yes | No | Partial | Partial | Ahead: per-hop SSRF re-check and hard caps are differentiators |
| Search provider fan-out | No | Partial | Yes | Yes | Absent; blocks discovery workflows |
| PDF/Office input | No | No | Partial | Yes | Absent; limits document-heavy pipelines |
| REST API / server | No | No | Yes | Yes | Absent; blocks service integration |
| MCP server | No | No | Yes | No | Absent; agent integrations must be hand-built |
| Native binary (no interpreter) | Yes | No | No | No | Ahead: Rust single binary plus PyO3 bindings |
| Python package | Yes | Yes | Yes | No | Parity; Jina is HTTP-only |
| Apache-2.0 license | Yes | Yes | Yes | Yes | Parity |

### Gaps that matter (ranked)
1. **JavaScript rendering is absent, not merely weaker.** README lines 139-141 and FEATURE_PARITY_MATRIX §E delegate `render_js` to MudFish; Crawl4AI and Jina Reader render pages in-process. Users scraping SPAs, infinite scroll or client-hydrated content must install and wire a second project. Hurts AI/RAG users and anyone replacing a hosted reader. Effort: L if built here, S to document/integrate MudFish.
2. **No published extraction-quality or throughput evidence.** Trafilatura cites ACL-2021 and Bevendorff-2023 benchmarks and an article-extraction benchmark; our RUST_ARCHITECTURE §8 only records 122 passing tests and un-wired fuzz targets. Buyers cannot compare precision/recall or pages/sec, so "Rust is faster" is an unproven claim. Hurts adoption against Trafilatura. Effort: M (port a public benchmark corpus, publish method/hardware).
3. **REST API and MCP server are roadmap only.** FEATURE_PARITY_MATRIX §G marks `quillsift-api`/`quillsift-mcp` P1 and RUST_ARCHITECTURE M6 as planned; Crawl4AI ships a server and MCP. Agent and backend users cannot call QuillSift as a service without writing their own wrapper. Effort: M.
4. **Structured-input and fan-out parity gaps.** §C lists CSV/JSON/XML/PDF graphs and multi-page merge at P1/P2, unimplemented; Trafilatura already emits/consumes CSV/JSON/XML and Jina Reader ingests PDF/Office. This excludes document-batch and multi-URL extraction jobs. Effort: M.
5. **Search-provider fan-out is absent (M4b deferred, no backend selected).** Trafilatura has sitemap/feed discovery, and both Crawl4AI and Reader expose search; QuillSift cannot turn a query into scrape candidates. Hurts research/monitoring use cases. Effort: M.
6. **Crawl maturity is weaker, and explicitly frozen.** The README marks the crawler "legacy compatibility fallback"; feature matrix §F omits content-hash dedup and resumable/incremental crawl (delegated to MudFish). Compared with Trafilatura's URL management and Crawl4AI's caching, long or interrupted crawls are fragile. Effort: S to harden, or defer to MudFish.

### Where we are differentiated
- **Security-first fetch.** SSRF address classification with per-hop redirect re-checks, rebind defence, decompressed-size caps and always-on TLS are defaults; Trafilatura and Crawl4AI do not advertise equivalent guarantees. This is the strongest reason to choose QuillSift for untrusted URLs.
- **Typed, bounded DAG execution.** Deadlines, cancellation, retries by error class and failure isolation are first-class (RUST_ARCHITECTURE §7), unlike linear scrape pipelines.
- **Deterministic-by-default plus optional LLM, one engine.** No browser/key for the core path, with OpenAI-compatible/Ollama schema extraction and bounded repair behind the same fetcher; neither Trafilatura (no LLM) nor Reader (hosted) matches this split.
- **Rust binary + GIL-releasing PyO3 bindings.** Deployment without a Python runtime and Python ergonomics from one core; unique among the three.

### Disposition
- **Build:** benchmark harness against the public Trafilatura/article-extraction corpora; REST API + MCP server (M6); multi-page fan-out and local JSON/XML/CSV inputs; search-provider abstraction with a documented ToS/credential policy.
- **Integrate/reuse:** keep JS and advanced crawling in MudFish and document it as the supported path; reuse public extraction corpora and evaluation methodology (Apache-2.0) rather than reimplementing benchmarks; reuse `jsonschema`/`schemars` validation already in place.
- **Defer/reject:** in-tree headless browser, stealth/anti-bot, screenshots and LLM code generation (all out of scope per §E/§H); PDF/Office parsing can stay deferred behind an optional crate/feature.

### Sources
- https://github.com/adbar/trafilatura (accessed 2026-10-24) — license, push date, README feature/benchmark claims.
- https://github.com/unclecode/crawl4ai (accessed 2026-10-24) — license, push/creation dates, README JS/schema/MCP claims.
- https://github.com/jina-ai/reader (accessed 2026-10-24) — license, push date, README headless-Chrome/PDF/Office/search claims.
- Repo files inspected: `README.md`, `Cargo.toml`, `docs/FEATURE_PARITY_MATRIX.md`, `docs/RUST_ARCHITECTURE.md`, `USE_CASES.md`.
