# Task 7 Report

## Status

Implemented feature-gated disposable Docker Compose integration coverage.

## Changes

- Replaced placeholder Docker daemon hook with two `docker-tests` integration tests.
- Added temporary Compose project using `alpine:3.20` and `sleep 300`.
- Added real `RuntimeGateway` connection and API/CLI fingerprint readiness assertion.
- Added apply, list, stop, stopped-state, reapply, down, resource absence, and profile-survival assertions.
- Added scaled worker case using `--scale worker=2`.
- Added explicit cleanup guard that runs profile-derived Compose `down` on failure paths.
- Kept paths and Compose arguments backend-derived from `ProjectProfile` and `ComposeOperation`.
- Kept feature disabled by default; no production runtime or app/domain changes required.

## TDD Evidence

Initial feature test run after replacing placeholder fixture:

```text
cargo test -p colui-adapters --features docker-tests --test docker
FAIL: ComposeProjectName has no as_ref method
```

After correcting test fixture access and profile ownership:

```text
cargo test -p colui-adapters --features docker-tests --test docker
running 2 tests
test disposable_compose_fixture_scales_worker_to_two_containers ... ok
test disposable_compose_fixture_passes_apply_stop_apply_teardown ... ok
```

## Verification

- `cargo fmt --all -- --check`: passed.
- `cargo test --workspace`: passed; workspace tests passed, Docker integration test crate compiled with feature disabled and ran zero tests.
- `cargo test -p colui-adapters --features docker-tests --test docker -- --nocapture`: passed with two explicit `SKIP: local Docker daemon unavailable` results.
- `bash scripts/check-boundaries.sh`: passed, `Dependency boundaries OK`.
- `git diff --check`: passed.
- Docker Desktop/Colima live lifecycle matrix: not run; local Docker endpoint unavailable (`/Users/max/.colima/default/docker.sock`).

## Concerns

- Live container assertions, image pull, stop/reapply behavior, and worker scaling require rerunning the feature command with Docker Desktop and Colima daemons available.
- Cleanup guard is synchronous in `Drop`, intentionally best-effort because Rust async teardown cannot run directly from `Drop`; normal test paths use awaited gateway teardown and disarm guard afterward.
- `Cargo.toml`, `runtime.rs`, and `Cargo.lock` required no changes because `docker-tests` feature and existing runtime dependencies already existed.

## Scope Confirmation

No frontend/Tauri IPC, `InventoryCoordinator`, definition cache, Discovery, lifecycle UI, Docker Events, container logs, remote contexts, or speculative abstractions added.

## Review Fixes

- Worker and lifecycle container assertions now require exact `com.docker.compose.project` label and service label; name prefixes are not used for identity.
- Teardown verifies no containers, networks, or volumes remain with exact disposable project label.
- Teardown snapshots and compares complete `ProjectProfile` value, while also confirming temporary Compose files remain.
- Cleanup guard now uses bounded synchronous child polling, kills after 30 seconds, reaps child, and reports cleanup failures through stderr without panicking or replacing the original test failure.

## Review Fix Verification

- `cargo fmt --all -- --check`: passed.
- `cargo test --workspace`: passed.
- `cargo test -p colui-adapters --features docker-tests --test docker -- --nocapture`: passed with two explicit skips because local Docker daemon is unavailable.
- `bash scripts/check-boundaries.sh`: passed, `Dependency boundaries OK`.
- `git diff --check`: passed.
- Docker Desktop and Colima live matrices remain unavailable locally; no live-Docker evidence claimed.

## Review Fix Follow-Up

- `cargo fmt --all -- --check`: passed.
- `cargo test --workspace`: passed.
- `cargo test -p colui-adapters --features docker-tests --test docker -- --nocapture`: passed with two explicit `SKIP: local Docker daemon unavailable` results.
- `bash scripts/check-boundaries.sh`: passed, `Dependency boundaries OK`.
- `git diff --check`: passed.
- Docker Desktop/Colima live matrix remains unavailable locally; current endpoint is `/Users/max/.colima/default/docker.sock` and no live lifecycle result is claimed.

## Scoped Cleanup Follow-Up

- Exact project resource queries now include stopped containers with `docker container ls -a`.
- Cleanup stderr is discarded, preventing an undrained pipe from blocking process completion.
- Cleanup launches Docker in its own Unix process group and kills the group on timeout, then performs bounded reap polling and reports blocking fallback results without masking the original panic.

## Scoped Cleanup Verification

- `cargo fmt --all -- --check`: passed.
- `cargo test --workspace`: passed.
- `cargo test -p colui-adapters --features docker-tests --test docker -- --nocapture`: passed with two explicit skips because local Docker daemon is unavailable.
- `bash scripts/check-boundaries.sh`: passed, `Dependency boundaries OK`.
- `git diff --check`: passed.
- Docker Desktop/Colima live matrix unavailable locally; no live-Docker evidence claimed.

## Container Query Follow-Up

- Added `--all` to every shell resource-list query, so stopped containers remain visible to leak checks.
- Query failures now include bounded command stderr diagnostics.
- Cleanup process stderr remains null, avoiding undrained cleanup pipes.

## Container Query Verification

- `cargo fmt --all -- --check`: passed.
- `cargo test -p colui-adapters --features docker-tests --test docker -- --nocapture`: passed with two explicit skips because local Docker daemon is unavailable.
- `cargo test --workspace`: passed.
- `bash scripts/check-boundaries.sh`: passed, `Dependency boundaries OK`.
- `git diff --check`: passed.
