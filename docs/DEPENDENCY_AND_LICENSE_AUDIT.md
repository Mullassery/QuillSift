# Dependency & License Audit

- **Date:** 2026-10-20
- **Scope:** upstream ScrapeGraphAI `v2.3.1` dependencies, the reuse decision,
  and the policy for adopting Rust dependencies.

## 1. Headline decision

ScrapeOwl is a **clean-room reimplementation**. **No upstream Python source,
prompts, or assets are copied.** Therefore upstream's dependency licenses do
**not** propagate to ScrapeOwl by dependency, but we still audit them to
understand attribution obligations for faithfully reproduced *public API
concepts* and to document that no copyleft code was imported.

- Upstream project license: **MIT** (`LICENSE`, "Copyright 2024 Scrapgraph-ai
  team"). MIT is permissive and compatible with a permissive ScrapeOwl license.
- The managed service (`scrapegraph-py`) and its internals are **not** inspected
  or reused; it is a paid, separate product.
- Attribution for the upstream project (name, link, MIT notice) is recorded in
  [`../NOTICE.md`](../NOTICE.md).

> **Verification status:** no dependency was machine-verified in Phase 0. Tables
> below label each row's license as *unverified* unless it was read directly
> from a file in the audited clone. Rust crates are **candidates** and must pass
> the adoption gate (§4) before use. Do not treat the "expected license" column
> as legal advice.

## 2. Upstream direct dependencies (`pyproject.toml`)

Versions are as declared upstream. License column = *unverified in Phase 0*
unless noted.

| Package | Declared range | Expected license | Verification |
|---|---|---|---|
| langchain | >=1.2.0 | MIT | unverified |
| langchain-classic | >=1.0.0 | MIT | unverified |
| langchain-openai | >=1.1.6 | MIT | unverified |
| langchain-mistralai | >=1.1.1 | Apache-2.0 | unverified |
| langchain_community | >=0.4.0 | MIT | unverified |
| langchain-aws | >=1.1.0 | MIT | unverified |
| langchain-ollama | >=1.0.1 | MIT | unverified |
| html2text | >=2025.4.15 | GPL-3.0 | unverified — **copyleft; do not port code** |
| beautifulsoup4 | >=4.14.3 | MIT | unverified |
| python-dotenv | >=1.2.1 | BSD-3-Clause | unverified |
| tiktoken | >=0.12.0 | MIT | unverified |
| tqdm | >=4.67.1 | MIT/MPL-2.0 | unverified |
| minify-html | >=0.18.1 | MIT | unverified |
| free-proxy | >=1.1.3 | MIT | unverified |
| playwright | >=1.57.0 | Apache-2.0 | unverified |
| undetected-playwright | >=0.3.0 | (verify) | unverified; **not adopted** |
| semchunk | >=3.2.5 | MIT | unverified |
| async-timeout | >=4.0.0 | Apache-2.0 | unverified |
| simpleeval | >=1.0.3 | MIT | unverified |
| jsonschema | >=4.25.1 | MIT | unverified |
| ddgs | >=9.0.0 | MIT | unverified |
| pydantic | >=2.12.5 | MIT | unverified |
| scrapegraph-py | >=2.0.0 | MIT SDK / paid service | **not reused** |

Optional extras: `burr[start]` (Apache-2.0 expected), 
`langchain-nvidia-ai-endpoints` (verify), `surya-ocr` (GPL-3.0 expected —
copyleft), `matplotlib` (PSF-like), `ipywidgets` (BSD-3), `pillow` (MIT-CMU).

**Findings**

- `html2text` and `surya-ocr` are the notable **copyleft** candidates. Because
  ScrapeOwl copies no code, this does not bind us; it only reinforces the
  clean-room approach. ScrapeOwl's markdown conversion will use a
  permissively-licensed Rust crate, not a port of `html2text`.
- `undetected-playwright` (evasion) is explicitly **not adopted** (see
  [SECURITY_THREAT_MODEL.md](./SECURITY_THREAT_MODEL.md) and the parity matrix
  "out of scope" section).

## 3. Upstream LICENSE text (verified by reading the file)

```
Copyright 2024 Scrapgraph-ai team

Permission is hereby granted, free of charge, ... (MIT License, 7 lines)
```

Full text retained upstream at `LICENSE`. The notice above will be reproduced
in `/NOTICE.md` with a link, satisfying MIT attribution for conceptual reuse.

## 4. Rust dependency adoption gate

Before any crate is added to `Cargo.toml`:

1. **Maintenance:** recent release/commit within ~12 months; responsive repo.
2. **License:** OSI-approved; prefer `MIT OR Apache-2.0`. Reject unknown
   licenses. Reject GPL/AGPL/SSPL for linked code unless there is a documented,
   reviewed exception (there is no intent to ship copyleft).
3. **Safety/profile:** minimize `unsafe` and FFI; parsing crates must not open
   sockets.
4. **Record:** add row to the table below with version + resolved license.
5. **Enforce in CI:** `cargo-deny` (licenses, bans, advisories, sources) and
   `cargo-audit`; fail the build on new copyleft or vulnerable deps.

### Adopted crates (verified)

Resolved versions and licenses were read from crates.io metadata for the exact
locked versions; `cargo deny check` passes with the allowlist in
[`../deny.toml`](../deny.toml).

| Crate | Version | Purpose | Resolved license | Status |
|---|---|---|---|---|
| tokio | 1.54.0 | async runtime | MIT | adopted |
| reqwest | 0.12.28 | HTTP client (rustls backend) | MIT OR Apache-2.0 | adopted |
| webpki-roots | 1.0.10 | Mozilla root store | CDLA-Permissive-2.0 | adopted (transitive; permissive data license) |
| scraper | 0.27.0 | HTML parsing | ISC | adopted |
| ego-tree | 0.11.0 | DOM arena (scraper dependency) | ISC | adopted |
| htmd | 0.5.6 | HTML→Markdown | Apache-2.0 | adopted |
| url | 2.5.8 | URL handling | MIT OR Apache-2.0 | adopted |
| http | 1.5.0 | header map | MIT OR Apache-2.0 | adopted |
| thiserror | 2.0.21 | error enums | MIT OR Apache-2.0 | adopted |
| serde | 1.0.229 | serialization | MIT OR Apache-2.0 | adopted |
| serde_json | 1.0.154 | JSON (tests) | MIT OR Apache-2.0 | adopted |
| tracing | 0.1.44 | observability | MIT | adopted |
| futures-util | 0.3.34 | stream utilities | MIT OR Apache-2.0 | adopted |
| async-trait | 0.1.92 | async trait objects | MIT OR Apache-2.0 | adopted |
| tokio-util | 0.7.20 | cancellation token | MIT | adopted |
| flate2 | 1.1.10 | gzip fixture in tests | MIT OR Apache-2.0 | dev-only |

**Note on CDLA-Permissive-2.0:** it is a permissive *data* license used by the
bundled CA roots, not code under a copyleft term. It is allowlisted explicitly in
`deny.toml`.

### Still candidates (not yet adopted)

| Crate | Purpose | Expected license |
|---|---|---|
| lol_html | streaming HTML rewrite | BSD-3-Clause |
| schemars | JSON Schema generation | MIT |
| jsonschema | runtime validation | MIT |
| async-trait | async traits | MIT/Apache-2.0 (avoid if native suffices) |
| tracing-subscriber | log formatting | MIT |
| axum / tower / tower-http | API server | MIT |
| rmcp | MCP SDK | verify (maturity gate) |
| clap | CLI | MIT/Apache-2.0 |
| ipnet | CIDR policy | MIT/Apache-2.0 |
| sha2 | content hashing | MIT/Apache-2.0 |
| tiktoken-rs | token counting | MIT |
| cargo-deny / cargo-audit / cargo-fuzz | tooling | MIT/Apache-2.0 |

No candidate is adopted until it is verified and recorded here.

## 5. ScrapeOwl license decision

- **Decided: Apache-2.0.** Confirmed by the maintainer on 2026-10-20. Chosen for
  its explicit patent grant; compatible with the permissive upstream MIT code we
  do not copy. See [`../LICENSE`](../LICENSE).
- [`../NOTICE.md`](../NOTICE.md) credits ScrapeGraphAI (MIT) for the concepts
  and public API names it inspired, and states clearly that ScrapeOwl is
  independent and unaffiliated.

## 6. Outstanding work

- Machine-verify every upstream dependency license (e.g. `pip-licenses` / SBOM)
  if any conceptual reuse is ever in question.
- ~~Verify each adopted Rust crate's resolved license and record it.~~ Done for
  M0–M1 crates (§4); repeat for each new adoption.
- ~~Add `deny.toml` with an explicit allowlist and run it in CI from M0.~~ Done:
  `deny.toml` is committed and enforced by CI (`cargo deny check` passes).
