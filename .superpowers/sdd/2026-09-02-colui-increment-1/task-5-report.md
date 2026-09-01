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
