# Task 5 Report

## Status

Implemented concrete runtime adapter modules, Compose argv policy, and `RuntimeGateway` ownership/gating.

## Changes

- Added `DockerApiAdapter` mapping Bollard list and inspect values into domain container values.
- Added backend-only ordered Compose argv generation for Compose files, project name, environment files, and `up -d`, `stop`, `down`, and `restart`.
- Added `RuntimeGateway` implementing Docker API, Compose runner, runtime connector, and state reader ports.
- Added one-permit asynchronous Compose semaphore.
- Added session transitions for ready, mismatch, failed, disconnected, and reconnect flows.
- Added hermetic fake Docker and Compose control planes covering readiness, API-read mismatch behavior, Compose mismatch gate, client reuse, and reconnect.

## TDD Evidence

Initial gateway command before production implementation:

```text
cargo test -p colui-adapters --test runtime gateway_
FAIL: unresolved imports ComposeOperation, DockerApiAdapter, RuntimeGateway
```

After implementation:

```text
cargo test -p colui-adapters --test runtime gateway_
running 4 tests
test gateway_matching_fingerprints_reuses_one_client_and_allows_reads ... ok
test gateway_mismatch_allows_api_reads_but_blocks_compose ... ok
test gateway_missing_session_blocks_api_and_compose ... ok
test gateway_reconnect_replaces_mismatch_without_restart ... ok
test result: ok. 4 passed; 0 failed
```

## Required Checks

- `cargo fmt --all -- --check`: passed.
- `cargo test --workspace`: passed; all workspace tests passed, including 16 adapter runtime tests.
- `bash scripts/check-boundaries.sh`: passed, `Dependency boundaries OK`.
- `git diff --check`: passed.

## Concerns

- Feature-gated Docker test currently provides daemon availability/skip coverage; disposable Compose lifecycle fixture remains future integration work.

## Review Fixes

- Replaced eager Unix-socket construction with endpoint-bound Bollard factory creation per connection attempt; Unix, TCP, and HTTP endpoints use same resolved endpoint as CLI environment.
- Replaced split blocking state/client locks with one async snapshot mutex and generation invalidation. Connecting clears old observations; terminal state and session-bound API handle publish together.
- Retained API handles for mismatch reads with internal session ID, endpoint, and API fingerprint binding; Compose remains Ready-only.
- Added inspect network port mapping for all host bindings, protocols, and container ports.
- Rejected timed-out and nonzero `docker info` results before fingerprint parsing.
- Recorded real epoch-millisecond connection timestamps using existing `Timestamp` representation.
- Resolved stored relative Compose and environment paths against profile working directory.
- Added `docker-tests` feature and explicit daemon-unavailable feature test hook.

## Review Fix Verification

- `cargo test -p colui-adapters --test runtime gateway_`: 4 passed.
- `cargo test --workspace`: passed.
- `cargo test -p colui-adapters --features docker-tests`: passed; real-Docker hook reported clear daemon-unavailable skip.
- `cargo fmt --all -- --check`: passed.
- `bash scripts/check-boundaries.sh`: passed.
- `git diff --check`: passed.
