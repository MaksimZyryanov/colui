# Docker Smoke Fix Report

## Root Causes

- `docker_api.rs` normalized obsolete `com.docker.compose.working_dir` and `com.docker.compose.config-files` labels. Docker Compose 5.5 emits `com.docker.compose.project.working_dir` and `com.docker.compose.project.config_files`, so real containers lost working-directory and config-file metadata. Unit and inventory fixtures copied the obsolete keys and masked the defect.
- `containers_with_exact_project` listed every container and then inspected each one. Parallel smoke tests could remove an unrelated listed container before inspection, producing sanitized `RuntimeUnavailable`. List observations already contain canonical `compose.project`, so inspection was unnecessary.

## RED/GREEN

- RED: changed `normalization_extracts_only_the_four_official_compose_labels` to Docker Compose 5.5 label names, then ran `cargo test -p colui-adapters normalization_extracts_only_the_four_official_compose_labels -- --nocapture`. It failed at `docker_api.rs:267`: actual `working_directory` was `None`, expected `Some("/workspace")`.
- GREEN: changed only the two production label constants. Same focused test passed 1/1; ordered config-file assertion remained unchanged and passed.
- Full workspace initially exposed stale `tests/inventory.rs` fixture labels with the same `None` mismatch. Updating that fixture to official names made its focused test pass 1/1.
- Exact-project helper now filters `ContainerObservation.compose.project` and clones matching instances. No inspect calls, retries, or suite serialization were added. Existing real-Docker count, state, and service assertions exercise this helper; separate synthetic coverage would duplicate normalization/domain fixture setup without testing the removed Docker race.

## Files

- `crates/colui-adapters/src/runtime/docker_api.rs`
- `crates/colui-adapters/tests/inventory.rs`
- `crates/colui-adapters/tests/docker.rs`
- `.superpowers/sdd/2026-09-03-colui-increment-4-inventory/docker-smoke-fix-report.md`

## Verification

- `cargo test -p colui-adapters runtime::docker_api::tests:: -- --nocapture`: PASS, 8/8 focused tests.
- `cargo test -p colui-adapters --features docker-tests --test docker -- --test-threads=1 --nocapture`: PASS, 4/4 in 41.84s.
- `cargo test -p colui-adapters --features docker-tests --test docker -- --nocapture`: PASS twice consecutively, 4/4 in 20.78s and 4/4 in 20.81s.
- `cargo fmt --all -- --check`: PASS.
- `cargo test --workspace`: PASS.
- `cargo check -p colui-adapters --features docker-tests --tests`: PASS.
- `bash scripts/check-boundaries.sh`: PASS, `Dependency boundaries OK`.
- `COLUI_REAL_DOCKER_SMOKE=1 bash scripts/verify-increment-4.sh`: PASS, including frontend 87/87, contracts 34/34, production build, boundary checks, and parallel real-Docker smoke 4/4 in 20.89s.
- `git diff --check`: PASS.

## Concerns

- Increment script emits expected stderr from tests that intentionally exercise React error boundaries and Node's experimental local-storage warning; command exits successfully.
- No production behavior beyond the two Compose label constants changed.
