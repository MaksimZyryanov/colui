# Increment 4 Final Blockers Fix Report

## Status

Both Important whole-branch findings are fixed. Required hermetic verification passes; real-Docker smoke remains gated and was not run.

## Root Causes And Fixes

1. `DockerApiAdapter` converted Bollard errors with `error.to_string()`, and `BollardFactory` did the same for connection construction failures. List and inspect shared the first mapper, so arbitrary daemon responses could reach inventory IPC. Both paths now discard backend text, preserve stable typed codes, and emit only fixed safe messages. The adapter operation is now backend-neutral `runtime_request` rather than `docker_api`.
2. `get_project_details` loaded a `DefinitionProjection`, then called compatibility `GetProjectStatus` through `RuntimeFacade`, which could load the same definition again. Details now reads current inventory directly and builds status through `project_status_from_inventory_and_definition_projection`, reusing the exact loaded projection. Independent `get_project_status` behavior remains unchanged.

## Security Sanitization Contract

- Runtime list and inspect failures preserve `runtime_unavailable` and fixed message `Runtime request failed`.
- Runtime client construction failures preserve `runtime_connection_failed` and fixed message `Runtime connection failed`.
- Raw socket paths, endpoints, credentials, environment values, daemon response text, backend implementation names, and unbounded strings are omitted from message/details.
- Existing Compose stderr sanitization and definition projection behavior are unchanged.

## TDD Evidence

- Docker RED: `docker_errors_are_sanitized_at_the_adapter_boundary` failed because serialized `AppError` contained `token=super-secret` from a 10,000-byte daemon response containing a socket path and credential-bearing endpoint.
- Docker GREEN: focused test passed, 1 passed and 0 failed; typed `runtime_unavailable` remained and all raw markers were absent.
- Definition RED: focused test failed to compile because `project_status_from_inventory_and_definition_projection` did not exist.
- Definition GREEN: focused test passed, 1 passed and 0 failed.
- One-call proof: failed `DefinitionCache::definition` used a recording Compose runner; status consumed that same projection and current unavailable inventory; assertion observed exactly one invocation, `Unchecked`, and retained typed `definition_failed` error.

## Changed Files

- `crates/colui-adapters/src/runtime/docker_api.rs`
- `crates/colui-adapters/src/runtime/gateway.rs`
- `crates/colui-adapters/tests/definitions.rs`
- `crates/colui-app/src/status.rs`
- `src-tauri/src/commands/definitions.rs`
- `.superpowers/sdd/2026-09-03-colui-increment-4-inventory/final-blockers-fix-report.md`

## Verification

- `cargo fmt --all -- --check`: PASS.
- `cargo test --workspace`: PASS, 156 passed and 0 failed.
- `cargo test -p colui-tauri --test dto_contracts`: PASS, 20 passed and 0 failed.
- `cargo check -p colui-adapters --features docker-tests --tests`: PASS.
- `npx pnpm@9.15.5 lint`: PASS.
- `npx pnpm@9.15.5 typecheck`: PASS.
- `npx pnpm@9.15.5 test`: PASS, 87 passed and 0 failed.
- `npx pnpm@9.15.5 test:contracts`: PASS, 34 passed and 0 failed.
- `npx pnpm@9.15.5 build`: PASS, 191 modules transformed.
- `bash scripts/check-boundaries.sh`: PASS.
- `bash scripts/verify-increment-4.sh`: PASS; real-Docker smoke skipped by gate.
- `git diff --check`: PASS.

## Self-Review

- Scope stays within runtime adapter mapping, application projection, details command, focused tests, and report.
- List and inspect use one sanitized mapper; connection construction separately discards raw client errors.
- Details definition DTO and status derive from one immutable projection, eliminating second-load races.
- Compatibility command, inventory refresh, polling, lifecycle locking, DTO schema, and frontend remain unchanged.

## Concerns

- Real Docker was not exercised because `COLUI_REAL_DOCKER_SMOKE=1` was not enabled.
- Vitest emits expected stderr from deliberate error-boundary tests while exiting successfully.

## Fix Round 1

Scoped review found that adapter-level projection coverage could not detect a regression in Tauri command orchestration. `get_project_details` now delegates to private `get_project_details_inner`, parameterized only by existing `ProfileReader`, `DefinitionReader`, and `InventoryReader` ports. Command visibility and IPC schema are unchanged.

- RED: focused `colui-tauri` test failed to compile because `get_project_details_inner` did not exist.
- GREEN: `project_details_reads_failed_definition_once` passed, 1 passed and 0 failed.
- Command proof: recording failed `DefinitionReader` observed exactly one call; response retained `unchecked`, typed `definition_failed`, and unavailable runtime projection with zero containers.
- `cargo fmt --all -- --check`: PASS after formatter applied one test-only line wrap.
- `cargo test --workspace`: PASS, 157 passed and 0 failed.
- `cargo test -p colui-tauri --test dto_contracts`: PASS, 20 passed and 0 failed.
- `cargo check -p colui-adapters --features docker-tests --tests`: PASS.
- Frontend lint and typecheck: PASS.
- Frontend tests: PASS, 87 passed and 0 failed; contracts 34 passed and 0 failed.
- Frontend build: PASS, 191 modules transformed.
- `bash scripts/check-boundaries.sh`: PASS.
- `bash scripts/verify-increment-4.sh`: PASS; real-Docker smoke skipped by gate.
- `git diff --check`: PASS.
