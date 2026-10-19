# ScrapeOwl

> **Status: Phase 0 (audit) complete. No engine code exists yet.** This
> repository currently contains the audit and design documents only. Nothing
> here should be read as a claim of feature parity or production readiness.

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
docs/
├── UPSTREAM_AUDIT.md                 # read-only audit of ScrapeGraphAI v2.3.1
├── FEATURE_PARITY_MATRIX.md          # per-feature goals, priorities, out-of-scope
├── RUST_ARCHITECTURE.md              # proposed workspace, deps, milestones
├── DEPENDENCY_AND_LICENSE_AUDIT.md   # upstream deps + Rust adoption gate
└── SECURITY_THREAT_MODEL.md          # threats and required mitigations
NOTICE.md                             # upstream attribution (MIT)
```

## Roadmap (see RUST_ARCHITECTURE.md for acceptance criteria)

M0 workspace · M1 deterministic fetch/parse · M2 typed graph engine ·
M3 LLM extraction + CLI · M4 crawl/search · M5 optional browser ·
M6 API/MCP · M7 hardening/benchmarks · M8 docs/release.

## Open items before coding

- Confirm the project name **ScrapeOwl** passes a trademark/naming check.
- Confirm the ScrapeOwl license (proposed: MIT).
- Confirm MSRV and the MCP SDK choice.

## License

Proposed MIT — see [LICENSE](./LICENSE). Upstream ScrapeGraphAI is MIT-licensed;
attribution is recorded in [NOTICE.md](./NOTICE.md).
