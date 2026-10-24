# Contributing to QuillSift

Thanks for contributing. This is a solo-maintained alpha project; review times
vary and there is no merge SLA.

## Before you start

- Read [`README.md`](README.md) and the docs under [`docs/`](docs/) for what is
  implemented today.
- For a large feature, open an issue first so scope can be agreed before code.
- Security issues go through [`SECURITY.md`](SECURITY.md), not public issues.

## Development setup

Requires Rust stable and CPython 3.9+.

```bash
cargo build --workspace
cargo test --workspace
```

Python bindings and CLI:

```bash
python3 -m venv .venv
source .venv/bin/activate
pip install maturin pytest ruff
maturin develop -m crates/quillsift-python/Cargo.toml
pytest -q
ruff check .
```

## Checks before a pull request

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace --all-features
ruff check .
```

CI runs the same checks. A change that adds behavior should add a test, and a
change that alters behavior or limitations should update the README/docs.

## Pull requests

- Keep each pull request focused on one change.
- Include the command and result you used to verify it.
- Do not describe untested behavior as production-ready.
- Never commit API keys, scraped personal data, or other secrets.

## License

Contributions are accepted under the Apache License 2.0.
