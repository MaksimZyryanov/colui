# Final Review Fix Report

## Status

All four final review findings addressed in one wave.

## Fixes

- Removed `RuntimeGateway`'s application-facing `ComposeRunner` implementation and raw invocation path from normal builds. Added `invoke_profile`, which loads `ProjectProfile` through `ProfileReader` and derives executable, argv, cwd, environment, and operation from backend state. Raw gateway invocation remains only under `test-support` for adapter tests.
- Acquired shared Compose operation gate for full `connect_runtime` duration. Compose, disconnect, and reconnect now coordinate; generation checks continue to reject stale API observations.
- Added bounded PID-file polling before parsing child PID in process-group timeout test.
- Scaled Docker fixture now stops and asserts every matching project container is stopped before teardown. Stopped assertion also rejects empty matches.

## Verification

- `cargo fmt --all -- --check`: passed.
- `cargo test --workspace`: passed, 90 tests.
- `cargo test -p colui-adapters --features docker-tests --test docker -- --nocapture`: passed; two Docker lifecycle tests explicitly skipped because local daemon unavailable.
- `bash scripts/check-boundaries.sh`: passed.
- `git diff --check`: passed.

## Concerns

- Docker Desktop and Colima live matrix unavailable in current environment; no live-Docker result claimed.
- `test-support` is intentionally test-only gateway escape hatch and is enabled by `docker-tests` solely for integration fixture execution.

## Final Gateway Follow-Up

- `invoke_profile` now acquires operation gate before profile lookup and holds it through readiness validation, endpoint capture, invocation construction, and runner start.
- Non-ready states return typed Compose errors; no `unreachable!` remains on application state paths.
- Added regressions for non-ready invocation and invocation waiting behind an in-flight connect operation.

## Follow-Up Verification

- `cargo fmt --all -- --check`: passed.
- `cargo test --workspace`: passed, 90 tests.
- `cargo test -p colui-adapters --features docker-tests --test docker -- --nocapture`: passed; two Docker tests explicitly skipped because local daemon unavailable.
- `bash scripts/check-boundaries.sh`: passed.
- `git diff --check`: passed.
