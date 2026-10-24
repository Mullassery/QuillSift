## What changed and why

<!-- Link the issue or design note this relates to, if any. -->

## Verification

<!-- Paste the commands you ran and their real result, not "tests pass". -->

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo clippy --workspace --all-targets --all-features -- -D warnings`
- [ ] `cargo test --workspace --all-features`
- [ ] `ruff check .`
- [ ] `python crates/quillsift-python/tests/smoke.py` (Python binding changes)

## Checklist

- [ ] No API keys, scraped personal data, or credentials committed
- [ ] README/docs updated if behavior or limitations changed
- [ ] New behavior has a test, or the gap is explained
