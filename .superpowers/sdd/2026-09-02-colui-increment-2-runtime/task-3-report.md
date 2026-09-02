# Task 3 Report

## Status

Implemented endpoint resolution, CLI environment construction, Bollard fingerprint mapping, and strict CLI `docker info` fingerprint parsing.

## Changes

- Added `bollard = "0.18"` dependency and updated `Cargo.lock`.
- Exported `colui_adapters::runtime`.
- Added endpoint precedence: explicit preference, supplied `DOCKER_HOST`, then `unix:///var/run/docker.sock`.
- Added inherited CLI environment policy: force resolved `DOCKER_HOST`, clear context/TLS/API-version/experimental and Compose selector variables, preserve ordinary variables plus `DOCKER_CONFIG` and `DOCKER_BUILDKIT`.
- Added `SystemInfo` to `DaemonFingerprint` mapping for all four fields.
- Added CLI parser requiring exactly one non-empty `ID`, `Server Version`, `OSType`, and `Architecture`, preserving trimmed exact values.
- Added six integration tests covering required resolver/environment/parser behavior and Bollard mapping.

## TDD Evidence

1. Wrote `tests/runtime.rs` before runtime adapter modules.
2. Ran `cargo test -p colui-adapters --test runtime endpoint_resolution`.
3. Observed expected compile failure: unresolved import `colui_adapters::runtime` because runtime adapter modules did not exist.
4. Added minimal implementation.
5. Focused tests passed, followed by workspace and boundary verification.

## Commands And Output

- `cargo test -p colui-adapters --test runtime endpoint_resolution`: PASS, 1 passed.
- `cargo test -p colui-adapters --test runtime`: PASS, 6 passed.
- `cargo test -p colui-adapters --test runtime cli_info_parser`: PASS, 2 passed.
- `cargo fmt --all -- --check`: PASS.
- `cargo test --workspace`: PASS, workspace tests passed.
- `bash scripts/check-boundaries.sh`: PASS, `Dependency boundaries OK`.
- `git diff --check`: PASS.
- `cargo test -p colui-adapters --features docker-tests`: BLOCKED because package declares no `docker-tests` feature.

## Self-Review

- Adapter dependency direction remains domain <- app <- adapters.
- No forbidden dependency was added to domain or app.
- Environment construction never logs or serializes the full environment.
- Endpoint and fingerprint values use domain canonical trimming.
- CLI duplicate and missing fingerprint fields are rejected.
- No semver normalization or suffix stripping occurs.
- Scope limited to task 3 files and generated lockfile.

## Concerns

- Real-Docker verification cannot run from this checkout because `colui-adapters` has no declared `docker-tests` feature or real-Docker test target. No feature was added because task 3 file list and brief only request resolver/parser adapters; this remains follow-up work for the planned gateway tasks.

## Commit

Pending commit: `feat(runtime): resolve endpoint and fingerprints`
