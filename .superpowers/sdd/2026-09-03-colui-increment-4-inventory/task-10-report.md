# Task 10 Report

## Status

Complete. Hermetic acceptance gates pass without Docker or network. Required-feature Docker test targets compile. Real-Docker smoke remains opt-in and was not executed.

## Coverage Matrix

| Requirement | Coverage |
| --- | --- |
| Concurrent refresh; one list; shared generation/publication | `concurrent_refreshes_share_one_list_and_generation`, `successful_refresh_notifies_subscriber_once` |
| Lifecycle during background refresh | `lifecycle_refresh_joins_background_refresh` proves coordinator join and one Docker list |
| Lifecycle during definition load; no overtaking | `pending_lifecycle_overtakes_definition_lease`, `lifecycle_busy_returns_unchecked_without_running_compose` |
| Reconnect/disconnect during list | `session_change_during_list_rejects_obsolete_response_and_retains_snapshot` uses mutable session and semaphore barrier |
| Old/lower and equal generation | Backend obsolete-session rejection plus single publication; frontend `keeps cache reference for equal or lower generation` and lifecycle cache test |
| Profile revision during config load | `profile_revision_change_during_load_discards_old_result` uses blocked Compose runner and reload count |
| Outage after success and bounded backoff | `refresh_failure_retains_snapshot_and_automatic_backoff`, `automatic_backoff_uses_injected_clock_and_caps_then_resets` |
| Duplicate lifecycle and guard release | Lifecycle and operation tests cover conflict, success, failure, cancellation, pending cancellation, and callback-once release |
| Lifecycle observation failure | `observation_failure_keeps_compose_success_and_retained_inventory` proves retained generation, stale error, and released lock |
| Fast path call boundaries | Inventory adapter tests prove one list/no inspect; boundary gate rejects Compose config and registry writes from inventory path |
| Fake Docker protocol | Queued errors/observations, call counters, mutable session identity, deterministic clock, and semaphore/notify barriers |
| Fake Compose protocol | Existing success/failure/blocking fakes plus `compose_config_invocation_records_exact_boundary_inputs` for executable/argv/cwd/env |
| Frontend acceptance | Existing 85 tests cover schemas, generation structural sharing, no redundant lifecycle refresh, one visible poller, hidden pause, retention, and separate errors |
| Forbidden duplicate owners/APIs/dependencies | Boundary gate counts coordinator/cache/lock/API/generation/lock-map owners and rejects direct Tauri imports, path lifecycle payloads, unstable keys, fast config, registry writes, and forbidden Cargo edges |
| Real Docker label/revision smoke | `fixture_labels_feed_inventory_and_profile_change_updates_definition_revision`, feature-gated and reusing existing fixture |

## Verification

`bash scripts/verify-increment-4.sh` exited 0 after formatting:

- `bash scripts/verify-increment-3.sh`: pass; generated schemas unchanged.
- `cargo fmt --all -- --check`: pass.
- `cargo test --workspace`: pass, 147 Rust tests, 0 failed.
- `cargo test -p colui-tauri --test dto_contracts`: pass, 18 tests, 0 failed.
- `cargo check -p colui-adapters --features docker-tests --tests`: pass.
- `npx pnpm@9.15.5 lint`: pass.
- `npx pnpm@9.15.5 typecheck`: pass.
- `npx pnpm@9.15.5 test`: pass, 85 tests, 0 failed.
- `npx pnpm@9.15.5 test:contracts`: pass, 33 tests, 0 failed.
- `npx pnpm@9.15.5 build`: pass, 191 modules transformed.
- `bash scripts/check-boundaries.sh`: pass, `Dependency boundaries OK`.
- `git diff --check`: pass.
- Real-Docker smoke: skipped by default; run with `COLUI_REAL_DOCKER_SMOKE=1 bash scripts/verify-increment-4.sh`.

Additional: `bash scripts/check-boundaries.sh --self-test` passed.

## Changed Files

- `crates/colui-adapters/tests/inventory.rs`
- `crates/colui-adapters/tests/definitions.rs`
- `crates/colui-adapters/tests/docker.rs`
- `scripts/check-boundaries.sh`
- `scripts/check-boundaries-node.mjs`
- `scripts/verify-increment-4.sh`
- `.superpowers/sdd/2026-09-03-colui-increment-4-inventory/task-10-report.md`

## Production Fixes

None. Added tests and gates only.

## Self-Review And Concerns

- No sleeps, network, or Docker in ordinary acceptance tests.
- New race tests use deterministic notifications and semaphores; clock-driven backoff remains deterministic.
- Real-Docker smoke compiles but was not run because it is intentionally separately gated.
- Vitest emits expected React/jsdom stderr from tests asserting thrown errors; suite exits 0 and raw backend stderr is not exposed.

## Fix Round 1

Review blockers from commit `1618838` were addressed without production changes:

- Added distinct deterministic `disconnect_during_list_rejects_obsolete_response_and_retains_snapshot`; fake runtime transitions from ready to `Disconnected` behind list barrier and proves stale retention at unchanged generation.
- Lifecycle payload analysis now resolves local variable initializers and accepts only one-field `profileId` objects; inline, spread, unknown, and path-bearing variable payloads fail.
- React key analysis now traces local aliases and rejects `container.name`, `profile.name`, bare `name`, `displayName`, and `composeProjectName` origins while allowing stable ID expressions and aliases.
- Replaced spelling-sensitive Rust counts with tokenized structural checks for owner structs, refresh function, struct generation fields, and aliased lock-map fields.
- Added hermetic fixture matrix covering every architecture violation and harmless visibility, formatting, type-alias, stable-ID, and safe-payload variants.
- `verify-increment-4.sh` now runs structural boundary self-tests as an ordinary Docker-independent gate.

Fix-round verification:

- Focused disconnect test: 1 passed, 0 failed.
- `bash scripts/check-boundaries.sh --self-test`: pass; `Boundary structural self-tests OK`, `Boundary parser self-test OK`, `Dependency boundaries OK`.
- `cargo fmt --all -- --check`: pass.
- `cargo test --workspace`: pass, 148 Rust tests, 0 failed.
- `cargo test -p colui-tauri --test dto_contracts`: pass, 18 tests, 0 failed.
- `cargo check -p colui-adapters --features docker-tests --tests`: pass.
- `npx pnpm@9.15.5 lint`: pass.
- `npx pnpm@9.15.5 typecheck`: pass.
- `npx pnpm@9.15.5 test`: pass, 85 tests, 0 failed.
- `npx pnpm@9.15.5 test:contracts`: pass, 33 tests, 0 failed.
- `npx pnpm@9.15.5 build`: pass, 191 modules transformed.
- `bash scripts/check-boundaries.sh`: pass.
- `git diff --check`: pass.
- Real-Docker smoke: separately gated and not run.
