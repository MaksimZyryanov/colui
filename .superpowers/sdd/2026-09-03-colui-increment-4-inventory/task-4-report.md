# Task 4 Report

## Status

Completed. Added concrete atomic per-profile operation locks in `colui-adapters` and reconciled app contracts to spec §3.3.

## Implementation

- `OperationLockReader::is_busy` added; `OperationLockManager` now extends it.
- Lifecycle acquisition reserves pending state atomically, rejects duplicate lifecycle work, waits behind an existing definition lease, and promotes to active state.
- Definition acquisition is synchronous and returns `DefinitionBusy::LifecyclePending` or `DefinitionBusy::DefinitionActive` without awaiting or spawning work.
- Concrete `LifecycleOperationGuard` and `DefinitionLoadGuard` are `Send + Sync` RAII handles with one-shot `Drop` release callbacks.
- Adapter owns one mutex-protected `HashMap<ProfileId, ProfileState>`; no second lock map added to runtime, coordinator, or frontend.
- Migrated old app fake lock test to concrete guard contracts.

## TDD

- RED: `cargo test -p colui-adapters --test operations` failed because concrete manager and reconciled contracts were absent.
- GREEN: focused adapter operation suite passed 8/8.

## Verification

- `cargo test --workspace`: 110 passed, 0 failed.
- `cargo check -p colui-adapters --features docker-tests --tests`: passed; compile-only, Docker tests not run.
- `cargo fmt --all -- --check`: passed.
- `bash scripts/check-boundaries.sh`: `Dependency boundaries OK`.

## Fix Round 2

Review found `cancelled_active_lifecycle_releases_guard` used one `yield_now()` before aborting its spawned task. Scheduler timing could abort the task before it acquired its lifecycle guard, making the test pass without testing active-guard cleanup.

Fix:

- Added an explicit `tokio::sync::Notify` handshake after lifecycle guard acquisition. The test waits with a one-second timeout before asserting `is_busy`, aborting, and verifying the next lifecycle can acquire. No sleeps or unbounded polling remain.

Verification:

- `cargo test -p colui-adapters --test operations -- --nocapture`: 11 passed, 0 failed.
- `cargo test --workspace`: 121 passed, 0 failed.
- `cargo fmt --all -- --check`: passed.
- `bash scripts/check-boundaries.sh`: `Dependency boundaries OK`.
- `git diff --check`: passed.

## Port Reconciliation

Old boxed guard traits and async `acquire_definition_load` were replaced by concrete guards and synchronous `acquire_definition`, matching §3.3. Existing Docker/runtime, domain, Tauri, and frontend ports remain unchanged.

## Concerns

No blocking concerns. Docker daemon smoke execution remains separately gated and was not required for this lock-only task.

## Fix Round 1

Review of `b9a2cab` rejected three blocking findings:

- `operations.rs` used `std::thread::yield_now()` around a Tokio mutex for synchronous paths. This could starve saturated Tokio workers.
- Lifecycle state retained `OperationKind` but no `started_at`, violating spec §3.3 operation projection state.
- Tests lacked deterministic concurrent lifecycle winner, concurrent definition winner, and active-boundary cancellation coverage.

Fixes:

- Replaced Tokio mutex map with `std::sync::Mutex`; synchronous `acquire_definition` and `is_busy` now use bounded critical sections, while lifecycle waits only on `Notify` after releasing the mutex. No mutex is held across `await`.
- Added `Timestamp` start fields to active lifecycle and definition lease state, populated from the existing RFC3339 timestamp pattern.
- Added barrier-based concurrent acquisition tests and cancellation coverage for pending and active lifecycle states. Tests use barriers, task scheduling, and bounded timeout polling; no sleeps.

Fix round verification output:

- `cargo test -p colui-adapters --test operations -- --nocapture`: 11 passed, 0 failed.
- `cargo test --workspace`: 121 passed, 0 failed.
- `cargo check -p colui-adapters --features docker-tests --tests`: passed.
- `cargo fmt --all -- --check`: passed.
- `bash scripts/check-boundaries.sh`: `Dependency boundaries OK`.
