# quillsift

Python bindings for [QuillSift](https://github.com/Mullassery/QuillSift): a
Rust-native, self-hosted scraping engine with a deterministic (LLM-free) path,
a legacy bounded crawler and schema-validated LLM extraction.

> QuillSift was previously published as `scrapeowl`. That package is superseded
> by `quillsift`; the API is otherwise the same.

```bash
pip install quillsift
```

```python
import quillsift

markdown = quillsift.fetch("https://example.com")
data = quillsift.fetch("https://example.com", format="json")

schema = {
    "type": "object",
    "required": ["title"],
    "properties": {"title": {"type": "string"}},
}
result = quillsift.extract(
    "https://example.com",
    schema,
    model="gpt-4o-mini",  # reads OPENAI_API_KEY by default
)

report = quillsift.crawl(
    "https://example.com",
    max_depth=2,
    max_pages=25,
    respect_robots=True,
)
for page in report["pages"]:
    print(page["url"], page["content"]["metadata"]["title"])
```

`fetch` never needs an LLM or credentials. `crawl` is a legacy fallback: it is
breadth-first, same-origin by default, deduplicates URLs and returns per-page
failures without aborting the job, but new crawling and JavaScript-rendering
work should use [MudFish](https://pypi.org/project/mudfish/). `extract` supports
`provider="openai"`
(`OpenAI`-compatible `/chat/completions`) and `provider="ollama"` (`/api/chat`);
output must parse as JSON and validate against the schema, with bounded repair.

Security defaults: private/loopback destinations are refused unless
`allow_private=True` (needed for a local Ollama endpoint), redirects and
response sizes are bounded, and API keys are read from environment variables or
provider headers, never embedded in URLs.

Apache-2.0. Not affiliated with ScrapeGraphAI.
