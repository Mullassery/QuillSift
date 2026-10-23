# Feature Parity Matrix

Legend

- **Priority:** `P0` required for the deterministic core + first usable release;
  `P1` next; `P2` later/optional; `OOS` intentionally out of scope.
- **Parity goal:** the behavior ScrapeOwl aims to provide, expressed
  independently. It is a *goal*, not verified parity.
- Upstream reference column cites the audited v2.3.1 behavior
  (see [UPSTREAM_AUDIT.md](./UPSTREAM_AUDIT.md)).

> Rule: nothing in this table is "done" until a ScrapeOwl test exercises it and
> we report the real result. No unimplemented row may be described as
> equivalent.

## A. Fetching & input

| Feature | Upstream behavior | ScrapeOwl goal | Priority | Notes / risk |
|---|---|---|---|---|
| HTTP fetch (URL) | Playwright/Chromium by default, `requests` if `use_soup` | Async `reqwest` fetch, no JS by default | P0 | Deterministic core must not require a browser |
| Local HTML file/dir | `local_dir` passthrough | Read local file(s) | P0 | |
| JSON / XML / MD inputs | `FetchNode` handlers | Structured local input parsing | P0 | |
| CSV input | pandas read | `csv` crate parsing | P1 | |
| PDF input | PyPDFLoader (no first-class graph key) | Optional `pdf` extractor | P2 | Separate crate/feature flag |
| Request headers / user agent | `loader_kwargs` for browser; limited for `requests` | Typed config, allowlisted headers | P0 | |
| Timeouts (connect/read) | Partial; advisory in generation | Hard connect + total deadlines | P0 | |
| Redirects | Browser/requests default | Bounded redirects with SSRF re-check | P0 | Upstream has no SSRF/redirect policy |
| Response size / decompression caps | None observed | Byte + decompressed caps | P0 | |
| robots.txt | Only `RobotsNode` | Robots policy enforced by default in the legacy crawler | P1 | Implemented in `scrape-crawl` (M4a); MudFish is the supported crawler |
| Crawl delay | None | Configurable polite delay | P1 | Legacy per-origin delay in `scrape-crawl`; use MudFish for new crawls |
| Offline HTML→Markdown | `html2text` via `convert_to_md` | `scrape-html` markdown conversion | P0 | |
| Main-content extraction | `Html2TextTransformer` | Deterministic readability extraction | P0 | |
| Links / images extraction | `ParseNode` regex | DOM-based link/image extraction | P0 | |
| Title/description/metadata | Not first-class | Deterministic metadata extractor | P0 | ScrapeOwl differentiator |
| Canonical URL normalization | Basic `urljoin` | RFC-3986 canonicalization + dedup key | P0 | |

## B. Graph / pipeline engine

| Feature | Upstream behavior | ScrapeOwl goal | Priority | Notes |
|---|---|---|---|---|
| Typed node DAG | `BaseGraph` with state dict + boolean input exprs | Typed nodes with explicit typed state, checked at build time | P0 | Upstream uses dynamic dict state |
| Conditional branching | `ConditionalNode` + `simpleeval` on whole state | Typed conditional edges, no expression eval of state | P0 | Avoid simpleeval-with-state risk |
| Fan-out over URLs | `GraphIteratorNode`, semaphore default 16 | Bounded concurrent map node | P1 | |
| Concurrent independent nodes | Not supported (linear walk) | DAG scheduler with dependency-aware concurrency | P1 | Differentiator |
| Cancellation / deadlines | Not first-class | `CancellationToken` + deadline | P0 | |
| Retry policy by error class | None (only browser retry) | Bounded retry + backoff by category | P0 | |
| Per-node metrics/tracing | Callback token/cost + exec time | `tracing` spans + metrics | P0 | |
| Intermediate results | State dict | Typed intermediate results | P1 | |
| Failure isolation | First error aborts graph | Per-node error capture + typed result | P1 | |
| Unrestricted agent loop | Not present (graphs are fixed) | Not implemented | OOS | Keep graphs explicit |

## C. Graphs (pipeline presets)

| Upstream graph | Behavior | ScrapeOwl goal | Priority |
|---|---|---|---|
| `SmartScraperGraph` | fetch→parse→(reasoning)→generate, optional reattempt | `scrape` pipeline: fetch→extract→LLM structured output | P0 |
| `SmartScraperLiteGraph` | fetch→parse only (chunks) | Deterministic text/chunk output mode | P0 |
| `MarkdownifyGraph` | fetch→markdown (no LLM) | `markdown` pipeline (no LLM) | P0 |
| `JSONScraperGraph` | fetch JSON→LLM | Local structured input → LLM | P1 |
| `XMLScraperGraph` | fetch XML→LLM | Local structured input → LLM | P1 |
| `CSVScraperGraph` | CSV→LLM | Local CSV → LLM | P1 |
| `DocumentScraperGraph` | MD→LLM | Local text → LLM | P1 |
| `SmartScraperMultiGraph` | fan-out over URL list + merge | Bounded multi-page extraction | P1 |
| `SmartScraperMultiLiteGraph` | fan-out lite + merge | Multi-page parse/chunk | P2 |
| `SmartScraperMultiConcatGraph` | fan-out + conditional merge/concat | Concat/merge policy | P2 |
| `SmartScraperMultiBatchGraph` | OpenAI Batch API (paid) | Provider batch adapter (opt-in) | P2 |
| `SearchGraph` | search → fan-out scrape → merge | Pluggable search provider + fan-out; delegate crawl to MudFish | P1 |
| `OmniSearchGraph` | search + vision | Search + media (opt-in) | P2 |
| `SearchLinkGraph` | fetch → link list | Link discovery pipeline | P1 |
| `DepthSearchGraph` | k-level BFS + Qdrant RAG | Bounded depth + optional vector store | P2 |
| `OmniScraperGraph` | page + images → vision LLM | Multimodal extraction (opt-in) | P2 |
| `SpeechGraph` | answer → TTS (paid) | TTS adapter (opt-in) | OOS/P2 |
| `ScreenshotScraperGraph` | screenshots → vision (paid) | Browser screenshot + vision (opt-in) | P2 |
| `ScriptCreatorGraph` | LLM writes a Python scraper | Out of scope as-is | OOS |
| `ScriptCreatorMultiGraph` | LLM writes/merges scripts | Out of scope as-is | OOS |
| `CodeGeneratorGraph` | LLM writes + **executes** Python | Not implemented (no codegen execution) | OOS |
| `GraphBuilder` | LLM emits graph JSON | Deferred; may build typed graphs from a declarative spec | P2 |

## D. LLM / providers

| Feature | Upstream behavior | ScrapeOwl goal | Priority | Notes |
|---|---|---|---|---|
| OpenAI-compatible chat/completions | via LangChain | Native provider adapter | P0 | |
| Ollama | via LangChain | Native/adapter | P0 | |
| Anthropic / Gemini / Mistral / Bedrock / Groq / xAI / DeepSeek / OneApi / MiniMax / etc. | LangChain + wrappers | Adapters added on demand | P1/P2 | Only maintained, verified endpoints |
| Structured JSON output | LangChain structured output + tolerant JSON parser | Schema-constrained generation + deterministic validation | P0 | |
| Schema definition | Pydantic models / JSON schema | `schemars` JSON Schema + `jsonschema` validation | P0 | |
| Repair of malformed output | One brace-normalization attempt | Bounded repair with strict attempt cap | P0 | |
| Retries + backoff + jitter | LLM `max_retries` if configured | Explicit bounded policy | P0 | |
| Rate limiting | In-memory option | Token-bucket limiter | P1 | |
| Token/cost metrics | Callback + cost tables | Usage metrics; cost only from provider data | P1 | No fabricated pricing |
| Deterministic no-LLM mode | Lite/Markdownify graphs | First-class deterministic extraction | P0 | Differentiator |
| Silent provider fallback | Not present | Forbidden | P0 | Never fall back silently |
| Send secrets/context to LLM | User prompt + page content only | Page content + schema only; secrets redacted from logs | P0 | |
| Prompt-injection resistance | Ad-hoc instructions only | Treat page as untrusted data; policy cannot be overridden | P0 | |

## E. Browser / JS rendering

| Feature | Upstream behavior | ScrapeOwl goal | Priority | Notes |
|---|---|---|---|---|
| JS rendering | Playwright (+ optional Selenium) | Use MudFish `render_js`; no browser in ScrapeOwl | OOS | Delegated, not required for core |
| Stealth / anti-bot | `undetected-playwright` | **Not implemented** | OOS | Explicitly excluded |
| Downloads / file access | Not restricted | Disabled by default | P0 (policy) | |
| SSRF for browser nav/iframes/subresources | Not addressed | MudFish owns browser SSRF handling | OOS | ScrapeOwl has no browser path |
| TLS verification | Disabled on some paths | Always verified on HTTP fetches | P0 | |
| Screenshots | `FetchScreenNode` | Not planned in ScrapeOwl | OOS | Use MudFish/browser tooling if needed |

## F. Crawling

| Feature | Upstream behavior | ScrapeOwl goal | Priority | Notes / risk |
|---|---|---|---|---|
| Multi-page crawl with depth | `FetchNodeLevelK` (BFS) | Legacy bounded crawler; MudFish preferred | P1 | Implemented in `scrape-crawl` (M4a), fixes only |
| Same-origin restriction | `only_inside_links` option | Legacy default same-origin, configurable | P1 | Redirects checked; MudFish for new crawls |
| URL dedup / canonicalization | Membership scans | Canonical key set | P0 | Fragment-normalized URL set |
| Content dedup | None | Not planned in ScrapeOwl | OOS | Crawl processing is delegated to MudFish |
| Robots + crawl delay | Not enforced | Legacy enforcement with opt-out | P1 | MudFish is the supported politeness layer |
| Resumable / incremental crawl | None | MudFish SQLite frontier | OOS | Not planned in ScrapeOwl |

## G. Interfaces

| Feature | Upstream behavior | ScrapeOwl goal | Priority |
|---|---|---|---|
| CLI | None first-class | `scrape-cli` (url/prompt/schema/output) | P0 |
| REST API | None in OSS | `scrape-api` (scrape/extract) + health/ready | P1 |
| Auth for non-local | N/A | API-key auth, per-user quotas/rate limits | P1 |
| Request IDs + structured errors | N/A | Typed error envelope + request id | P1 |
| MCP server | Separate `scrapegraph-mcp` repo | `scrape-mcp` with validated args + output cap | P1 |
| Python bindings | N/A (upstream *is* Python) | Optional PyO3 after API stabilizes | P2 |
| Local/self-hosted by default | Yes | Yes — no paid credentials for deterministic scraping | P0 |

## H. Explicitly out of scope

- Reimplementing the proprietary ScrapeGraphAI managed API or `scrapegraph-py`
  internals.
- Anti-bot evasion, CAPTCHA solving, stealth fingerprinting.
- Executing LLM-generated code or scripts (`CodeGeneratorGraph` behavior).
- Any feature requiring a paid third-party key to perform basic local scraping.
