# Audit Findings: QuillSift

**Audit Date:** 2026-10-26  
**Version:** 0.3.0  
**Branch:** `main`  
**Full report:** [`.audit/reports/QuillSift/audit.md`](../.audit/reports/QuillSift/audit.md)

## Scope

Read-only review of the 9-crate Rust workspace, Python bindings, MCP wrapper, CLI, crawler, tests, CI, and packaging. No live web, LLM, or MCP calls were made.

## Findings

| ID | Severity | Finding | Evidence | Recommendation |
|---|---|---|---|---|
| QS-01 | Medium | MCP wrapper has no auth, rate limits, or remote-exposure guidance. | `crates/quillsift-python/python/quillsift/mcp_server.py`. | Enforce stdio/localhost defaults and add auth/limits for remote transport. |
| QS-02 | Medium | Fetched content is untrusted input to the LLM extractor. | LLM extraction path; schema validation only constrains shape. | Delimit content, instruct against embedded instructions, and add provenance/confidence. |
| QS-03 | Medium | Declared `abi3-py39` but CI tests only Python 3.12. | `pyproject.toml` vs `.github/workflows/ci.yml`. | Test 3.9–3.13 or raise the minimum. |
| QS-04 | Medium | Python/MCP coverage is a single smoke script. | `crates/quillsift-python/tests/smoke.py`; no pytest suite. | Add pytest coverage for all bindings and MCP tools. |
| QS-05 | Medium | No fuzzing for URL/HTML/redirect/schema parsing. | CI has no fuzz targets. | Add fuzz targets for parsers and policy edges. |
| QS-06 | Low/Medium | `allow_private` is a powerful escape hatch. | `FetchPolicy.allow_private_networks`; Python kwarg. | Require explicit separate capability and keep it off in agent-facing surfaces. |
| QS-07 | Low | No domain/content-type allowlist layer. | Policy review. | Add optional business-level allowlists and content-type checks. |
| QS-08 | Low | No fetch/LLM audit trail or metrics. | No logging/metrics evidence. | Add redacted audit logging and metrics. |
| QS-09 | Low | Build artifacts exist in the working tree. | `dist/`, `.so`, `__pycache__`; untracked at audit time. | Keep ignored and verify release packaging. |
| QS-10 | Low | Legacy crawler remains a maintained surface. | `quillsift-crawl`; README fallback framing. | Keep it scoped and test robots/limits, or deprecate. |

## Positive Controls

- Defense-in-depth SSRF guard: scheme, literal IP, DNS, redirects, metadata.
- Private destinations denied by default; `allow_private` is explicit.
- Guarded DNS resolver reduces rebinding risk.
- Manual redirect validation at every hop.
- Timeouts and response-size limits.
- Schema-validated LLM output.
- Python GIL released around blocking work with a shared Tokio runtime.
- Strong CI: fmt, clippy `-D warnings`, workspace tests, wheel smoke, cargo-deny, cargo-audit.
- Type stubs and `py.typed`.

## Audit Limitations

- No live web, LLM, or MCP calls.
- SSRF behavior was not adversarially fuzzed.
- Python 3.9–3.11 and non-Linux platforms were not executed.
- Test counts are definitions, not a verified pass count.
- Dependency advisories were not independently re-scanned.
