<!-- Link the issue this resolves. Wave contributors: one issue per PR. -->
Closes #

## What changed

## What breaks if this is wrong

<!-- The failure a reviewer should look for: lost funds, a wrong state, a leaked identifier, a broken page. -->

## How it was tested

## Checklist

- [ ] `cargo fmt --all` and `cargo clippy --all-targets -- -D warnings` are clean
- [ ] `cargo test` passes, with tests for the new behaviour or the bug
- [ ] `cargo build --target wasm32v1-none --release` succeeds
- [ ] Event fields unchanged, or the matching indexer change is linked
- [ ] Docs in `docs/` and the README contract reference updated if the interface changed
- [ ] No patient-identifying data added to storage or events
