# Task 6 Report

## Decision

Controller approved deferring error-bearing definition projection to Task 7. `CachedDefinition` remains adapter-private and stores `load_error` internally; public failures return retained stale definitions or unchecked definitions.

## RED/GREEN

- RED: `cargo test -p colui-adapters --test definitions` failed because `DefinitionCache` module did not exist.
- GREEN: focused definitions suite passed, 5 passed, 0 failed.

## Files

- Added `crates/colui-adapters/src/definitions.rs`.
- Added `crates/colui-adapters/tests/definitions.rs`.
- Modified `crates/colui-adapters/src/lib.rs` and adapter `Cargo.toml`.
- Reconciled `crates/colui-app/src/definitions.rs` to profile-valued reader/refresher/invalidation ports.

## Semantics

Revision-aware one-entry-per-profile cache, injected 60-second TTL clock, shared `OperationLockManager` definition guard, lifecycle-busy unchecked/stale fallback, invalidation epoch and profile-revision late-result rejection, canonical recursive JSON key ordering and compact SHA-256 revision, service/image/build/ports mapping, invalid-definition issues, and retained stale load failures.

## Verification

- `cargo test -p colui-adapters --test definitions`: 5 passed, 0 failed.
- `cargo test --workspace`: 98 passed, 0 failed.
- `cargo check -p colui-adapters --features docker-tests --tests`: passed.
- `cargo fmt --all -- --check`: passed.
- `bash scripts/check-boundaries.sh`: `Dependency boundaries OK`.

## Self-review / concerns

No Docker smoke run: requires daemon and is outside hermetic verification. Public error projection remains intentionally deferred to Task 7.
