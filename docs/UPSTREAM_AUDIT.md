# Upstream Audit — ScrapeGraphAI

- **Audit date:** 2026-10-20
- **Upstream:** https://github.com/ScrapeGraphAI/Scrapegraph-ai
- **Audited commit:** `194055e203afce41ed4e70365dbc416bad756115`
- **Audited tag/version:** `v2.3.1` (commit date 2026-10-06)
- **Upstream LICENSE:** MIT — "Copyright 2024 Scrapgraph-ai team" (typo in the
  upstream file is preserved here as-is for accuracy). Verified by reading
  `LICENSE` directly.
- **Method:** read-only inspection of a shallow clone. No upstream files were
  modified. All line references point into the audited clone.

> This document records observed upstream behavior. It is not a claim of
> feature parity by QuillSift. Uncertainties are listed explicitly in
> [§10](#10-explicit-uncertainties).

---

## 1. What upstream is

ScrapeGraphAI is a Python (>=3.12) library that builds LLM-driven scraping
pipelines expressed as directed graphs of nodes ("graphs"). A user supplies a
natural-language prompt and a source (URL or local file/dir); nodes fetch,
parse, chunk and then ask an LLM to produce structured output.

- Package manager: `uv` (`pyproject.toml`), build backend `hatchling`.
- LLM integration: LangChain (`langchain>=1.2.0`, `langchain-classic`,
  `langchain-openai`, `langchain-mistralai`, `langchain-community`,
  `langchain-aws`, `langchain-ollama`).
- Browser fetching: Playwright + `undetected-playwright` (and an optional
  Selenium backend via `undetected_chromedriver`).
- HTML handling: `beautifulsoup4`, `html2text`, `minify-html`.
- Search: `ddgs` (DuckDuckGo) plus a hand-written multi-engine wrapper.
- Hosted API: `scrapegraph-py>=2.0.0` (managed cloud SDK — a *separate*,
  paid service, not open source).

### Repository layout

```
scrapegraphai/
├── graphs/        pipelines (SmartScraperGraph, SearchGraph, …)
├── nodes/         single graph steps (FetchNode, ParseNode, GenerateAnswerNode, …)
├── models/        LLM wrappers and token/model metadata
├── docloaders/    fetchers (ChromiumLoader, browserbase, scrape.do, plasmate)
├── prompts/       prompt templates
├── helpers/       constants, token tables, schemas, robots mapping
├── integrations/  Burr, Indexify, scrapegraph-py managed-API shim
└── utils/         HTML cleanup, tokenization, callbacks, code-gen helpers
examples/          runnable example per graph
tests/             pytest suite (mixed deterministic + credentialed/network)
docs/              docs + translations
```

### Open-source library vs managed API

The README explicitly separates the two: the OSS library is self-hosted with
bring-your-own LLM keys; the managed API (`scrapegraph-py` / `scrapegraph-js`)
is a paid hosted service. The OSS repo contains a compatibility shim
(`integrations/scrapegraph_py_compat.py`) that calls that managed service, but
it is effectively **unreachable** through the normal config path (§7.4).

---

## 2. Execution model

### 2.1 AbstractGraph (`graphs/abstract_graph.py`)

- Constructor: `(prompt, config, source=None, schema=None)`.
- Builds the LLM **first** (`_create_llm`, line 70) and the graph second
  (line 80). An unsupported model/provider therefore fails at construction.
- `_create_llm` (121-291): known-provider allowlist (158-178); a `"provider/model"`
  string is split (180-183); otherwise the provider is inferred from the token
  table (185-195); an unknown provider raises (204-208).
- Token window: looked up in `helpers.models_tokens`; on miss it silently falls
  back to **8192 tokens** and sets `model_tokens_defaulted = True` (215-229).
- `run()` (334-342) builds `{"user_prompt": prompt, input_key: source}`, runs the
  graph, returns `final_state.get("answer", "No answer found.")`.
- `run_safe_async()` (344-353) offloads `run` to a thread executor.

### 2.2 BaseGraph (`graphs/base_graph.py`)

- Constructor `(nodes, edges, entry_point, use_burr=False, burr_config=None,
  graph_name="Custom")`.
- `edges` is converted to an adjacency dict via `_create_edges(set(edges))`
  (line 67) — **set() loses edge order**, but conditional true/false branches are
  assigned from the ordered `raw_edges` list (100-124): first declared edge =
  true branch, second = false branch.
- Execution `_execute_standard` (236-342): walks nodes from `entry_point`
  following `edges`, or follows a node-name returned by a `ConditionalNode`.
  A returned `None` terminates the graph (222-234).
- Per-node LLM token/cost accounting via callback manager (198-220) and a
  synthetic `TOTAL RESULT` row (310-320).
- Telemetry is emitted on both failure (293-308) and success (326-340).
- **No per-node retry loop.** The first node exception aborts the graph and is
  re-raised.
- Optional Burr integration (`integrations/burr_bridge.py`), local tracking by
  default.

---

## 3. Nodes

33 node modules. Nodes are state transformers: `execute(state: dict) -> dict`.
State-key wiring lives in each graph, not in the node. `input` is a boolean
expression over state keys (`&`, `|`, parentheses) parsed by
`BaseNode._parse_input_keys` (`nodes/base_node.py:136-235`).

Key nodes:

| Node | Role | LLM | Notable behavior / risk |
|---|---|---|---|
| `FetchNode` | Fetch URL / local file / dir | no | Dispatches by input key (`url`, `local_dir`, `pdf`, `csv`, `json`, `xml`, `md`, `*_dir`). Optional HTML→MD. Real timeouts for `requests` and PDF; propagates timeout into `ChromiumLoader`. **No robots enforcement, no SSRF protection.** |
| `ParseNode` | HTML→text chunks; URL/image extraction | no | Uses `Html2TextTransformer`; deterministic warning if requested terms are absent. |
| `GenerateAnswerNode` | Map-reduce Q&A with optional schema | yes | Single-chunk fast path + parallel per-chunk + merge. `invoke_with_timeout` only measures elapsed time **after** the call returns, so it does not bound a hung request. |
| `GenerateAnswerNodeKLevel` | RAG answer over Qdrant | yes | Embedding-query string hard-coded (bug). |
| `BatchGenerateAnswerNode` | OpenAI Batch API jobs | yes | Uses raw `openai.OpenAI()`, not the LangChain model. |
| `FetchNodeLevelK` | BFS fetch to depth k | no | Link denylist for non-web schemes; per-URL errors skipped. |
| `GraphIteratorNode` | Fan out N sub-graphs over URLs | indirect | `asyncio.Semaphore` default batchsize 16; increments `graph_depth` to prevent recursion. |
| `ConditionalNode` | Branch selector | no | Evaluates `condition` via `simpleeval.EvalWithCompoundTypes` with the **entire graph state** injected as names. |
| `SearchInternetNode` | LLM query → web search | yes | Engines: duckduckgo, bing, searxng, serper (serper needs a key). |
| `SearchLinkNode` / `SearchLinksWithContext` | Link extraction / query generation | partial | Regex-first; `SearchLinksWithContext` currently appends an uninvoked Runnable (bug). |
| `RobotsNode` | robots.txt + LLM scrapeability check | yes | **The only robots-aware node**; `force_scraping` bypasses with a warning. |
| `MergeAnswersNode` / `ConcatAnswersNode` | Combine multi-page answers | Merge yes / Concat no | Merge assumes answers are dicts (adds `sources`); Concat wraps `>1` results as `{"products": {...}}`. |
| `RAGNode` / `DescriptionNode` | Qdrant store + doc summaries | yes | Qdrant required for `DepthSearchGraph`. |
| `GenerateCodeNode` | Generate + iteratively repair Python extraction code | yes | **Executes LLM-generated code with full builtins** (see §7.5). |
| `GenerateScraperNode` / `MergeGeneratedScriptsNode` | Generate/merge scraper scripts | yes | First chunk only for `GenerateScraperNode`. |
| `ImageToTextNode` / `GenerateAnswerFromImageNode` | Vision | yes | Omni/screenshot graphs; direct OpenAI HTTP in one path. |
| `TextToSpeechNode` | TTS | TTS model | Assumes answer is a dict, takes first value. |
| `MarkdownifyNode` | HTML→Markdown | no | Used by `MarkdownifyGraph`, which extends `BaseGraph` directly. |

---

## 4. Graph inventory

26 graph classes. Full behavior is in
[FEATURE_PARITY_MATRIX.md](./FEATURE_PARITY_MATRIX.md). Summary:

- **Single-page:** `SmartScraperGraph` (fetch→parse?→reasoning?→generate,
  optional reattempt loop), `SmartScraperLiteGraph` (fetch→parse only, returns
  chunks), `MarkdownifyGraph` (fetch→markdown, no LLM), `CSVScraperGraph`,
  `JSONScraperGraph`, `XMLScraperGraph`, `DocumentScraperGraph`,
  `SearchLinkGraph` (returns link list), `SpeechGraph` (answer→audio),
  `ScreenshotScraperGraph` (screenshots→vision), `OmniScraperGraph`
  (page + images→vision).
- **Multi-page (iterator fan-out):** `SmartScraperMultiGraph`,
  `SmartScraperMultiLiteGraph`, `SmartScraperMultiConcatGraph`,
  `SmartScraperMultiBatchGraph` (OpenAI Batch API),
  `CSVScraperMultiGraph`, `JSONScraperMultiGraph`, `XMLScraperMultiGraph`,
  `DocumentScraperMultiGraph`, `ScriptCreatorMultiGraph`.
- **Search-driven:** `SearchGraph`, `OmniSearchGraph`, `SearchLinkGraph`.
- **Depth / RAG:** `DepthSearchGraph` (k-level BFS + Qdrant).
- **Code generation:** `CodeGeneratorGraph` (requires schema; writes a file),
  `ScriptCreatorGraph`, `ScriptCreatorMultiGraph`.
- **Builder:** `builders/graph_builder.py` uses an LLM/extraction chain to emit
  a graph JSON, optionally rendered to Graphviz.

---

## 5. Input formats & sources

| Source key | Handled by | Notes |
|---|---|---|
| `url` | FetchNode web path | Playwright/Chromium by default; `use_soup` uses `requests`. |
| `local_dir` | FetchNode | Raw HTML string passed in. |
| `json` / `json_dir` | JSONScraperGraph | Dir passes a path through. |
| `xml` / `xml_dir` | XMLScraperGraph | Dir passes a path through. |
| `csv` / `csv_dir` | CSVScraperGraph | CSV read via pandas. |
| `md` / `md_dir` | DocumentScraperGraph | Markdown/plain text. |
| `pdf`, `pdf_dir` | FetchNode supports it | No first-class graph sets a `pdf` input key. |
| `html` | MarkdownifyGraph | Raw HTML string. |
| `user_prompt` → search → `urls` | Search/Omni graphs | Requires a search engine. |

---

## 6. LLM integrations

Provider dispatch is in `abstract_graph.py` (158-288). Most providers go through
LangChain `init_chat_model`. Custom wrappers in `models/`:

- OpenAI-compatible (subclass `ChatOpenAI`, hard-code a base URL): `AtlasCloud`,
  `CheaperInference`, `CLoD`, `DeepSeek`, `MiniMax`, `OneApi`, `XAI`,
  `OpenAIImageToText`.
- OpenAI SDK directly: `OpenAITextToSpeech`.
- Native: `Nvidia` (lazy `langchain-nvidia-ai-endpoints`).
- Known providers include: openai, azure_openai, google_genai, google_vertexai,
  ollama, oneapi, nvidia, groq, anthropic, bedrock, mistralai, hugging_face,
  deepseek, ernie, fireworks, clod, togetherai, xai, minimax.

**Note:** `AtlasCloud`/`CheaperInference` exist and are in the token table but
are **not** in the `known_providers` allowlist, so selecting them via graph
config raises. They are only reachable by direct instantiation / `model_instance`.

Token/cost metadata: `helpers/models_tokens.py` (~432 lines, provider→model→max
input tokens) and `utils/model_costs.py` (cost tables). Unknown models fall back
to 8192 tokens silently.

---

## 7. Cross-cutting behaviors, defects and risks

### 7.1 Error handling & retries
- No generic node retry / backoff. First failure aborts the graph and re-raises.
- `GenerateAnswerNode` catches a narrow exception set and writes an error dict.
- `ChromiumLoader` has a retry loop (`retry_limit=1` default).
- `TolerantJsonOutputParser` retries once by stripping one doubled-brace layer.

### 7.2 Timeouts
- Real: `FetchNode` `requests`, PDF parse (thread + `future.result(timeout=)`),
  `ChromiumLoader` (`async_timeout`, default 60s), batch API poll cap.
- **Advisory only:** `GenerateAnswerNode.invoke_with_timeout` (does not abort).
- Missing entirely for several generation/merge/RAG/reasoning nodes.

### 7.3 Robots & rate limits
- Only `RobotsNode` consults robots.txt. `FetchNode`, `FetchNodeLevelK` and
  `ChromiumLoader` fetch arbitrary URLs with no robots check and no crawl-delay.
- `research_web.py` applies a global 10-requests/60s rate limit and query
  sanitization, but that is search-only.

### 7.4 Managed-API shim
- `integrations/scrapegraph_py_compat.py` calls the hosted `scrapegraph-py`
  service with a caller-provided `api_key`.
- In `SmartScraperGraph` the branch that reaches it (lines 79-95) is effectively
  unreachable because `_create_llm` rejects the `"scrapegraphai"` provider
  before `_create_graph` runs. Documented as a defect, not a supported path.

### 7.5 Security-relevant patterns (factual, not exhaustively validated)
1. **Arbitrary code execution (critical):** `GenerateCodeNode` calls
   `exec(function_code, {"__builtins__": __builtins__, ...})` on LLM-generated
   code (`nodes/generate_code_node.py:456`). This is not a sandbox. Reachable
   from `CodeGeneratorGraph`, which also writes a file to disk.
2. **Dynamic module execution:** `utils/sys_dynamic_import.py` `exec_module` on
   an arbitrary path; `ChromiumLoader` selects a backend by import name.
3. **SSRF:** `FetchNode`, `FetchNodeLevelK`, `ChromiumLoader`, `FetchScreenNode`
   fetch caller-supplied URLs with no private-IP / loopback / metadata blocking.
4. **TLS verification disabled:** `docloaders/chromium.py` sets
   `ignore_https_errors=True` on one Playwright path; `docloaders/scrape_do.py`
   uses `verify=False` and globally suppresses `InsecureRequestWarning`.
5. **Telemetry exfiltration by default:** `telemetry/telemetry.py` posts
   `user_prompt`, `json_schema`, `website_content`, `llm_response`, `url` to
   `https://sgai-oss-tracing.onrender.com/v1/telemetry` for URL-based runs.
   Enabled by default; opt-out via env var / `~/.scrapegraphai.conf`.
6. **Prompt injection:** scraped content is interpolated directly into prompts
   with only ad-hoc instructions; no systemic delimiter/sanitization.
7. **Path traversal:** `save_code_to_file`, `save_audio_from_bytes`,
   `data_export` write caller-supplied paths without sanitization.
8. **Expression evaluation with state:** `ConditionalNode` injects the whole
   graph state as evaluation names into `simpleeval` with compound types.

### 7.6 Behavioral defects observed in source
These are recorded because they affect "parity" expectations:
- `XMLScraperMultiGraph` and `DocumentScraperMultiGraph` pass state key `xmls`
  while their iterator node requires `jsons`; `XMLScraperMultiGraph` also
  misspells `scaper_config`.
- `CodeGeneratorGraph` depends on state key `original_html`, which no node
  writes.
- `SearchLinksWithContext` appends an uninvoked Runnable and writes `urls`
  instead of the expected `parsed_doc`.
- `RAGNode` uses a non-existent `QdrantClient.add` in one branch and upserts only
  the last point in another.
- `GenerateAnswerNodeKLevel` hard-codes the embedding query string.
- Several `run() -> str` annotations actually return dict/list.
- `MarkdownifyGraph` is not exported and has a different calling convention.

---

## 8. Tests & CI

- `pytest.ini`: `testpaths = tests`, discovery `test_*.py` and `*_test.py`,
  coverage on by default, global 300s timeout, `asyncio_mode = auto`.
- `tests/conftest.py` provides a real local `HTTPServer` on `localhost:8888`
  (`tests/fixtures/mock_server/server.py`) with routes for HTML, JSON, XML, CSV,
  `/slow`, `/error/404`, `/error/500`, `/rate-limited`, `/dynamic`, `/pagination`.
  This is the strongest reusable test asset.
- **CI deliberately runs only a hand-picked deterministic subset**
  (`.github/workflows/test-suite.yml:38-52`). `tests/graphs/*`,
  `tests/integration/*` and `tests/test_chromium.py` are excluded because they
  need real keys, network or browsers.
- Many provider tests read real keys (`OPENAI_APIKEY`, `CLOD_API_KEY`,
  `FIREWORKS_APIKEY`, …) or hit live sites and are **not** consistently marked
  `integration`/`requires_api_key`, so a naive `pytest` run tries the network.
- Two test files are 0 bytes (`test_json_scraper_multi_graph.py`,
  `test_smart_scraper_multi_concat_graph.py`).
- Stale assertions exist (e.g. `test_models_tokens.py` expects a DeepSeek token
  count and a Groq model key that no longer match the source).

### Behavioral contracts encoded by tests (reusable as acceptance criteria)
- `FetchNode` timeout default 30s, `None` disables, propagates to browser loader
  unless overridden; PDF timeout raises `TimeoutError`.
- `GenerateAnswerNode` timeout yields `{"error": "Response timeout exceeded", ...}`.
- `ChromiumLoader` warns (does not raise) on HTTP >= 400; retry-limit semantics.
- `ParseNode` absent-term warning; schema field names treated as expected terms.
- `JSONScraperGraph` input-key selection (`json` vs `json_dir`) and schema
  forwarding.
- Multi-graph config deep-copy, node/edge counts, fallback strings.
- `AbstractGraph` 8192-token fallback + `model_tokens_defaulted` flag.
- Telemetry flag parsing precedence (env over config file).

---

## 9. Dependency snapshot (upstream, for license cross-check)

Direct deps: `langchain`, `langchain-classic`, `langchain-openai`,
`langchain-mistralai`, `langchain_community`, `langchain-aws`,
`langchain-ollama`, `html2text`, `beautifulsoup4`, `python-dotenv`, `tiktoken`,
`tqdm`, `minify-html`, `free-proxy`, `playwright`, `undetected-playwright`,
`semchunk`, `async-timeout`, `simpleeval`, `jsonschema`, `ddgs`, `pydantic`,
`scrapegraph-py`. Optional: `burr`, `langchain-nvidia-ai-endpoints`,
`surya-ocr`/`matplotlib`/`ipywidgets`/`pillow`.

See [DEPENDENCY_AND_LICENSE_AUDIT.md](./DEPENDENCY_AND_LICENSE_AUDIT.md) for the
reuse decision (clean-room reimplementation; no upstream code copied).

---

## 10. Explicit uncertainties

- **Runtime behavior was not executed.** This audit is a static read of the
  2.3.1 source and its tests. No upstream test suite was run (it needs API keys,
  network and browsers). "Parity" claims must be re-derived from our own tests.
- **Exact provider semantics** (e.g. `with_structured_output` differences across
  LangChain versions) were not exercised.
- **The `scrapegraph-py` managed service internals were not inspected** and are
  assumed proprietary; nothing is reused from it.
- **Dependency licenses** were not machine-verified in this phase (no lockfile
  license tool was run). The Rust side defers all adoption to a verified audit.
- **Trademark/naming:** The project was renamed from ScrapeOwl to QuillSift
  after preliminary checks found the former name conflicts with an existing
  commercial scraping API. QuillSift package, repository and domain candidates
  were checked for availability on 2026-10-23; formal trademark clearance has
  **not** been performed. This remains a release blocker in
  [RUST_ARCHITECTURE.md](./RUST_ARCHITECTURE.md) and
  [../README.md](../README.md).
- **Security patterns** listed in §7.5 are from static inspection only; they are
  not the result of an exploit test.

---

## 11. Clean-room position

QuillSift is an **independent, clean-room Rust reimplementation** of the
*concepts* found in ScrapeGraphAI's public API surface (typed graph pipelines,
prompt-driven extraction, multi-page/search/depth graphs). It does **not** copy
upstream source, prompts, or assets, and it is **not affiliated with or
endorsed by** ScrapeGraphAI. Upstream is MIT-licensed; attribution and the MIT
notice are preserved in [../NOTICE.md](../NOTICE.md) for any concepts or public
API names that are intentionally kept compatible.
