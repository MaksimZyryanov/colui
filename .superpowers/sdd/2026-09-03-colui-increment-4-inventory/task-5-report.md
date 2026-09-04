# Task 5 Report

## RED

`cargo test -p colui-adapters --test inventory` failed before implementation:
`unresolved import colui_adapters::InventoryCoordinator`.

## Implementation

- Added `InventoryCoordinator` with immutable retained projections, generation sequencing, one in-flight `OnceCell`, bounded failure backoff, explicit/automatic refresh paths, session/fingerprint validation, normalization/grouping, and broadcast subscribers.
- Updated application inventory ports to shared `current_inventory()` and `refresh()` contracts.
- Added `RuntimeInventorySource` composition port for Docker and runtime session identity.
- Added focused tests for coalescing, generation, unavailable state, grouping, publication, retention, stale freshness, and automatic backoff.

## Verification

- `cargo test -p colui-adapters --test inventory`: 8 passed, 0 failed.
- `cargo test --workspace`: 98 passed, 0 failed.
- `cargo check -p colui-adapters --features docker-tests --tests`: passed.
- `cargo fmt --all -- --check`: passed.
- `bash scripts/check-boundaries.sh`: `Dependency boundaries OK`.
- `git diff --check`: passed.

## Review

No Docker smoke run required. Fast refresh uses one list operation and no inspect/Compose calls. No domain, Tauri, frontend, DefinitionCache, lifecycle wiring, or second lock map touched.

## Fix Round 1

- Replaced post-await `OnceCell` cleanup with creator-owned watch completion. The creator removes in-flight state and sends the shared result while holding the coordinator mutex, preventing a new wave before completion publication.
- Added application `Clock` port and injected monotonic/timestamp time. Retry deadlines and snapshot timestamps are deterministic.
- Refresh failures now return typed `Err(AppError)` while `current_inventory()` retains stale/unavailable data and error state.
- Added blocking-source coalescing regression and controllable-clock coverage for 1s, 2s, 4s, 8s, 10s backoff, blocking automatic refresh, cap, and success reset.

Fix verification:

- `cargo test -p colui-adapters --test inventory -- --nocapture`: 9 passed, 0 failed.
- `cargo test --workspace`: 98 passed, 0 failed.
- `cargo check -p colui-adapters --features docker-tests --tests`: passed.
- `cargo fmt --all -- --check`: passed.
- `bash scripts/check-boundaries.sh`: `Dependency boundaries OK`.

Concern: Docker daemon smoke remains intentionally unrun per task scope.
