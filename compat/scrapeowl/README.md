# scrapeowl

**Deprecated:** this package was renamed to
[`quillsift`](https://pypi.org/project/quillsift/).

`scrapeowl` 0.2.3 is a compatibility shim. It depends on `quillsift==0.3.0` and
re-exports the same API, but emits an import-time `DeprecationWarning`.

```python
# Preferred
import quillsift

result = quillsift.fetch("https://example.com")
```

```python
# Legacy; still works for now
import scrapeowl

result = scrapeowl.fetch("https://example.com")
```

The old name conflicted with an existing commercial scraping API. New work
belongs at https://github.com/Mullassery/QuillSift.

Apache-2.0.
