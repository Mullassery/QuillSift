# Security Policy

## Supported versions

QuillSift is alpha software. Only the latest release and the `main` branch
receive fixes; there is no backport policy.

## Reporting a vulnerability

Please do not open a public issue for a security vulnerability. Use GitHub's
[private vulnerability reporting](../QuillSift/security/advisories/new)
for this repository. If that is unavailable, open an issue titled
"Security contact request" with no vulnerability details.

This is a solo-maintained project with no guaranteed response time or SLA.

## Security model

- **SSRF protection is default-on.** Private, loopback, and cloud-metadata
  destinations are refused unless a caller explicitly opts in. The guard runs
  after DNS resolution and applies per redirect.
- **Redirects, response size, and time are bounded.** Fetched content is never
  executed.
- **LLM extraction is optional and caller-configured.** API keys are read from
  the environment or explicit configuration and are never logged by QuillSift.
  When enabled, the cleaned page text is sent to the endpoint you choose.
- **Untrusted input stays untrusted.** HTML, Markdown, and extracted JSON are
  data. Do not render scraped HTML directly into a privileged page or execute
  model output.
- **Dependency scanning runs in CI.** No audit proves the absence of
  vulnerabilities.

## Scope

The Rust crates, CLI, Python bindings, and repository workflows are in scope.
Third-party LLM providers, destination websites, and your deployment
environment are out of scope.
