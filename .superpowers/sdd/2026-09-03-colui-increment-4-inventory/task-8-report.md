# Task 8 Report

## RED/GREEN

- RED: `cargo test -p colui-tauri --test dto_contracts` failed because `RuntimeInventoryDto` and lifecycle fixture were absent; frontend command was unavailable as bare `pnpm`.
- GREEN: focused Rust DTO contracts 17/17, IPC contracts 29/29, and TypeScript typecheck pass.

## Changes

- Added explicit `RuntimeInventoryDto`, container/project projections, freshness enum, definition DTOs, and project-details response.
- Lifecycle DTO now carries `inventoryGeneration` and embedded inventory; mapping asserts generation equality.
- Added shared `DefinitionCache` to `AppState`; profile update/removal invalidates it.
- Added `get_inventory`, `refresh_inventory`, `get_project_details`, and `refresh_project_definition`; registered all Tauri commands.
- Added IPC schemas, inferred types, wrappers, mock generation/lifecycle responses, and pre-observation validation.

## Contracts

- CamelCase fields, kebab-case enum values, RFC3339 timestamps, canonical UUID validation.
- Pre-observation inventory is generation `0`, unavailable, empty, and has no fabricated runtime identity.
- Runtime inventory retains full containers, grouped projects, and standalone containers without path/secret caller authority.
- Generated schemas and positive/negative fixtures synchronized.

## Verification

- `cargo test --workspace`: pass.
- `cargo fmt --all -- --check`: pass.
- `bash scripts/check-boundaries.sh`: pass.
- `cargo check -p colui-adapters --features docker-tests --tests`: pass.
- `npm run test:contracts`: pass.
- `npm run typecheck`: pass.

## Concerns

- Docker smoke not run per instruction.

## Fix Round 1

- Sanitized Compose failure mapping by omitting subprocess stderr from `AppError.details`; regression covers paths, project names, and secret-like values.
- Added typed command tests for inventory, definition, lifecycle generation, exact names/arguments, malformed responses, and invalid IDs.
- Added direct positive/negative pre-observation fixture checks and generation/`hasSnapshot` invariant assertions.
- Mock successful inventory now includes canonical session/fingerprint metadata plus profile-derived project/container data; lifecycle refresh changes generation and container state consistently.
- Runtime session UUID now uses canonical UUID serde validation, covered by Rust DTO contract test.

### Fix Verification

- `cargo test -p colui-adapters compose_failure_omits_raw_stderr_from_transport_error`: 1/1 pass.
- `cargo test -p colui-tauri --test dto_contracts`: 18/18 pass.
- Focused IPC tests: 33/33 pass.

## Fix Round 1 Verification

- `cargo test --workspace`: 128 tests passed, 0 failed.
- `npm run test:contracts`: 33 tests passed, 0 failed.
- `npm run typecheck`: pass.
- `cargo fmt --all -- --check`: pass.
- `bash scripts/check-boundaries.sh`: pass.
- `cargo check -p colui-adapters --features docker-tests --tests`: pass.
- Docker smoke skipped as instructed.
