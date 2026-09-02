# Task 5 Report

## Scope

Implemented isolated legacy v1 JSON import in `crates/colui-adapters`.
No Task 6 files, `.DS_Store`, or legacy source files were changed.

## Changes

- Added `registry/import_v1.rs` with strict known-field conversion from v1
  project entries to v2 `ProjectProfile` values.
- Preserved project order and Compose/environment file order.
- Mapped legacy names to display names and normalized lowercase Compose names
  by replacing invalid runs with `-` and trimming separators.
- Used `imported-<first-eight-profile-id-hex>` fallback for empty normalized
  names.
- Generated IDs through `IdGenerator`, retained revision 1, and set
  `RegistrationOrigin::Migrated`.
- Kept colliding Compose names as separate profiles.
- Added `import_v1_if_needed` first-start orchestration: skips existing v2,
  reads legacy bytes without modifying them, backs up bytes, persists imported
  profiles through existing atomic registry mutation, and creates empty v2
  state with `RegistryCorrupt` diagnostics for malformed input.
- Added locked empty-v2 initialization for malformed imports.

## TDD Evidence

1. Added importer tests before production importer code.
2. Ran:
   `cargo test -p colui-adapters --test registry import_preserves_entry_and_file_order_without_touching_source`
3. Red observed: compiler reported unresolved import
   `colui_adapters::import_v1`; importer did not exist.
4. Implemented minimal parser/converter.
5. Ran importer tests; focused ordering and malformed tests passed.
6. Added first-start orchestration test before orchestration implementation.
7. Red observed: compiler reported unresolved import
   `import_v1_if_needed`.
8. Implemented orchestration and malformed empty-state handling.
9. Focused orchestration tests passed.

## Verification

- `cargo fmt --all -- --check`: PASS.
- `cargo test -p colui-adapters --test registry`: PASS, 22 passed, 0 failed.
- `cargo test --workspace`: PASS; adapter 22, app 13, domain 12; all doc
  tests passed.
- `git diff --check`: PASS.
- Legacy source bytes are asserted unchanged for valid and malformed imports;
  backup bytes are asserted equal to source bytes.

## Concern

- No pre-existing application startup coordinator exists in this increment.
  `import_v1_if_needed` is therefore exposed as adapter orchestration for the
  eventual startup caller. Task 6 remains untouched.

## Review Fix Report

### Findings fixed

- Serialized canonical existence check, legacy read, backup, parse, and v2
  write under the registry's existing exclusive lock. Concurrent first-start
  callers now produce one import; later callers skip after seeing canonical
  v2 under the lock.
- Removed separate empty initialization path, so malformed import handling
  cannot overwrite a canonical registry created by another caller.
- Preserved `_` during Compose namespace normalization; invalid characters
  remain the only separator runs.
- Preserved `PermissionDenied` for legacy read, backup, and related filesystem
  failures through the existing typed filesystem error mapping.
- Replaced direct backup overwrite with durable sibling temp-file write,
  file sync, atomic rename, and parent-directory sync.

### TDD Evidence

1. Added failing underscore normalization regression test.
2. Ran focused test; observed `checkout-api` instead of expected
   `checkout_api`.
3. Added failing concurrent first-start test.
4. Ran focused test; observed two imports instead of one.
5. Added failing legacy permission test.
6. Ran focused test; observed `RegistryWriteFailed` instead of
   `PermissionDenied`.
7. Implemented lock-scoped orchestration, normalization, error mapping, and
   atomic backup.
8. Ran adapter tests; 25 passed.

### Verification Commands and Output

- `cargo fmt --all -- --check`: PASS.
- `cargo test -p colui-adapters --test registry`: PASS, 25 passed, 0 failed.
- `cargo test --workspace`: PASS; adapter 25, app 13, domain 12; all doc
  tests passed.
- `git diff --check`: PASS.

### Remaining concerns

- `import_v1_if_needed` currently invokes lock-scoped synchronous filesystem
  work directly from its async function. Existing registry mutation already
  uses `spawn_blocking`; future startup integration should likewise dispatch
  this orchestration from a blocking worker without changing importer scope.

## Review Fix Report: Async Blocking Boundary

### Finding fixed

- Moved complete lock-scoped first-start orchestration, including canonical
  claim, legacy read, durable backup, parse, and v2 atomic write, behind
  `tokio::task::spawn_blocking`.
- Preserved borrowed-call behavior by cloning the generator into the worker;
  callers still pass `&G`.
- Existing race safety, source immutability, malformed diagnostics, and
  atomic backup behavior remain covered by adapter tests.

### TDD Evidence

- Reproduced root cause by tracing `import_v1_if_needed` into direct
  `registry.run_import`, whose lock polling and filesystem operations executed
  on the async caller thread.
- Added no behavior changes outside worker placement; existing concurrency
  regression remains active and passed after the fix.
- Focused green run:
  `cargo test -p colui-adapters --test registry concurrent_first_start_imports_only_once`
  Result: PASS, 1 passed, 0 failed.

### Verification Commands and Output

- `cargo fmt --all -- --check`: PASS.
- `cargo test -p colui-adapters --test registry`: PASS, 25 passed, 0 failed.
- `cargo test --workspace`: PASS; adapter 25, app 13, domain 12; all doc
  tests passed.
- `git diff --check`: PASS.

### Remaining concerns

- `import_v1_if_needed` requires `Clone + Send + Sync + 'static` on `IdGenerator`
  so owned generator state can cross the blocking worker boundary. Existing
  deterministic and production-compatible generator implementations must meet
  these bounds.
