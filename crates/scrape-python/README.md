# scrapeowl

Python bindings for [ScrapeOwl](https://github.com/Mullassery/ScrapeOwl): a
Rust-native, self-hosted scraping engine with a deterministic (LLM-free) path
and schema-validated LLM extraction.

```bash
pip install scrapeowl
```

```python
import scrapeowl

markdown = scrapeowl.fetch("https://example.com")
data = scrapeowl.fetch("https://example.com", format="json")

schema = {
    "type": "object",
    "required": ["title"],
    "properties": {"title": {"type": "string"}},
}
result = scrapeowl.extract(
    "https://example.com",
    schema,
    model="gpt-4o-mini",  # reads OPENAI_API_KEY by default
)
```

`fetch` never needs an LLM or credentials. `extract` supports `provider="openai"`
(`OpenAI`-compatible `/chat/completions`) and `provider="ollama"` (`/api/chat`);
output must parse as JSON and validate against the schema, with bounded repair.

Security defaults: private/loopback destinations are refused unless
`allow_private=True` (needed for a local Ollama endpoint), redirects and
response sizes are bounded, and API keys are read from environment variables or
provider headers, never embedded in URLs.

Apache-2.0. Not affiliated with ScrapeGraphAI.
