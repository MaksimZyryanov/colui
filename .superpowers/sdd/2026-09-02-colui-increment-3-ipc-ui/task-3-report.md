# Task 3 Report: Committed IPC JSON Schemas

## Status

DONE_WITH_CONCERNS

## Commit

- `7ef1b21 test(tauri): commit ipc json schemas`
- Follow-up stale-artifact fix commit recorded after final commit below.

## Changes

- Added `src-tauri/src/schema_generation.rs` with deterministic `schemars::schema_for!` generation for every public DTO and stable pretty JSON serialization through a `BTreeMap`.
- Added `src-tauri/src/bin/schema-generator.rs` as the explicit filesystem-writing entry point.
- Added `scripts/generate-schemas.sh`; it writes only the selected `schemas/` output directory.
- Added 26 committed DTO schemas under `schemas/`.
- Added positive fixtures for profile summary, runtime unavailable, runtime context mismatch, project status, lifecycle result, and profile validation.
- Added negative fixtures for missing ID, invalid enum, malformed timestamp, and missing required fields.
- Added `scripts/verify-increment-3.sh`; it generates into a temporary directory, overlays generated files for the required `git diff --exit-code schemas/` gate, then restores the original schema tree through an exit trap.
- Schema writing removes obsolete generated JSON files before writing current output.
- Extended `src-tauri/tests/dto_contracts.rs` with deterministic schema generation and required `LifecycleResultDto.profileId` type assertions.
- Exported schema generation from `src-tauri/src/lib.rs` for the contract test and generator binary.
- No frontend, registry, Docker, polling, coordinator, generation-number, or Increment 4 behavior was added.

## TDD Evidence

1. Added `generated_schema_files_are_deterministic` before production generation code.
2. Ran focused test and observed RED because `colui_tauri_lib::schema_generation` did not exist.
3. Added minimal schema generation module and explicit generator binary.
4. Re-ran focused test and observed GREEN: 1 passed, 0 failed.
5. Ran full DTO contract suite: 7 passed, 0 failed.
6. Temporarily changed `LifecycleResultDto.success` schema name to `outcome`; `scripts/verify-increment-3.sh` emitted the expected `LifecycleResultDto.json` diff and exited non-zero.
7. Removed temporary annotation and re-ran all checks successfully.
8. Self-review identified obsolete generated files were not removed. Added `schema_writer_removes_obsolete_generated_files`; initial run failed because `ObsoleteDto.json` remained.
9. Added JSON artifact reconciliation and re-ran focused test successfully.
10. Re-ran full DTO suite: 8 passed, 0 failed.

## Exact Commands And Outputs

### RED

Command:

```bash
cargo test -p colui-tauri --test dto_contracts generated_schema_files_are_deterministic
```

Output:

```text
error[E0432]: unresolved import `colui_tauri_lib::schema_generation`
 --> src-tauri/tests/dto_contracts.rs:8:22
  |
8 | use colui_tauri_lib::schema_generation::generate_all_schemas;
  |                      ^^^^^^^^^^^^^^^^^ could not find `schema_generation` in `colui_tauri_lib`
error: could not compile `colui-tauri` (test "dto_contracts") due to 1 previous error
```

### Focused GREEN

Command:

```bash
cargo fmt --all && cargo test -p colui-tauri --test dto_contracts generated_schema_files_are_deterministic
```

Output:

```text
running 1 test
test generated_schema_files_are_deterministic ... ok

test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 6 filtered out
```

### Required Generator

Command:

```bash
bash scripts/generate-schemas.sh
```

Output: none; exit 0. Generated 26 schema files and 10 fixture files under `schemas/`.

### Exact Brief Verification

Command:

```bash
cargo test -p colui-tauri --test dto_contracts && bash scripts/generate-schemas.sh && git diff --exit-code schemas/
```

Output:

```text
running 7 tests
test status_enums_use_approved_wire_values ... ok
test lifecycle_result_preserves_application_success ... ok
test update_request_serializes_only_camel_case_metadata_and_patch ... ok
test lifecycle_result_contains_profile_id_and_success ... ok
test every_stable_error_code_uses_snake_case ... ok
test dto_boundary_rejects_non_rfc3339_timestamps ... ok
test generated_schema_files_are_deterministic ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Generator and schema diff emitted no output; exit 0.

### Stale-Schema Negative Proof

Command after temporary schema-only rename from `success` to `outcome`:

```bash
bash scripts/verify-increment-3.sh
```

Output included:

```diff
diff --git a/schemas/LifecycleResultDto.json b/schemas/LifecycleResultDto.json
-    "success": {
+    "outcome": {
```

Exit status: 1, as required. Exit trap restored original committed schemas. Temporary source annotation was then removed.

### Final Verification

Command:

```bash
cargo fmt --all -- --check && cargo test -p colui-tauri --test dto_contracts && bash scripts/generate-schemas.sh && git diff --exit-code schemas/ && bash scripts/verify-increment-3.sh && git diff --check && git status --short
```

Output:

```text
running 7 tests
test status_enums_use_approved_wire_values ... ok
test lifecycle_result_preserves_application_success ... ok
test update_request_serializes_only_camel_case_metadata_and_patch ... ok
test every_stable_error_code_uses_snake_case ... ok
test lifecycle_result_contains_profile_id_and_success ... ok
test dto_boundary_rejects_non_rfc3339_timestamps ... ok
test generated_schema_files_are_deterministic ... ok

test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Formatting, generator, both schema-diff gates, diff check, and status emitted no output; exit 0.

### Commit

Command:

```bash
git add src-tauri schemas scripts/generate-schemas.sh scripts/verify-increment-3.sh && git commit -m "test(tauri): commit ipc json schemas"
```

Output:

```text
[colui-increment-3 7ef1b21] test(tauri): commit ipc json schemas
42 files changed, 1376 insertions(+)
```

## Self-Review

- `generate_all_schemas` uses `schema_for!` directly for all exported DTO structs and enums from Task 2.
- `BTreeMap` fixes top-level schema write order; `serde_json::to_vec_pretty` produces stable formatting and files end with one newline.
- Writer removes only JSON files in schema output directories, preserving non-generated files while preventing stale generated artifacts.
- Generator binary requires an explicit output directory and performs no registry, profile, runtime, Docker, or application-state access.
- Fixture values use stable UUIDs, timestamps, paths, and enum values; no wall clock, random source, environment-specific registry data, or network data is read.
- Required fixture categories exist: six positive scenarios and four negative structural scenarios.
- Required stale check terminates non-zero on generated/committed drift and restores the working schema tree after comparison.
- Scope remains Task 3 only. No frontend files or Increment 4 contracts changed.

## Concerns

- Schemars 0.8 cannot infer RFC 3339 format from custom serde timestamp functions, so generated timestamp fields are plain JSON Schema strings. Malformed timestamp rejection remains enforced by Rust serde and represented by the negative fixture; Task 4 manual Zod schemas must apply runtime timestamp validation.
- JSON Schema expresses ID fields as strings, not UUID format, matching current Task 2 DTO annotations. Invalid UUID semantics remain decoder/backend validation responsibility unless future DTO schema annotations add explicit formats.
- `scripts/verify-increment-3.sh` temporarily overlays `schemas/` to satisfy the plan-required `git diff --exit-code schemas/` command, then restores the prior tree with a trap. Concurrent edits to `schemas/` during that brief verification window could be overwritten.

## Fix Round 1 Evidence

### Findings Addressed

- Replaced working-tree overlay verification with recursive `diff -ruN` between committed `schemas/` and separately generated temporary tree. Added, removed, and changed schema or fixture paths now produce non-zero status; verifier never mutates working `schemas/`.
- Restricted schema writer cleanup to explicit current schema and fixture artifact names. Unrelated files such as `keep.json` remain untouched.
- Added schemars `uuid` formats to UUID-bearing DTO fields and `date-time` formats to RFC3339 DTO fields, including nested and nullable fields. Runtime serde validators remain authoritative.
- Corrected `runtime_unavailable.json` to `RuntimeStateDto::Failed` with `runtime_unavailable` error. Corrected context mismatch fixture to contain distinct API and CLI fingerprints.
- Added `profile_summary_invalid_uuid.json` and `project_status_invalid_runtime_combination.json` negative fixtures.
- Strengthened contract tests with exact schema count, deterministic property manifest, byte-for-byte repeated writer output, explicit format assertions, and exact fixture manifest assertions.

### Exact Fix Verification Commands And Outputs

```bash
cargo test -p colui-tauri --test dto_contracts
```

```text
running 10 tests
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

```bash
cargo test --workspace && git diff --check && bash scripts/generate-schemas.sh && bash scripts/verify-increment-3.sh
```

```text
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 27 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 4 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 17 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 1 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 7 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
test result: ok. 10 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

Generator and verifier emitted no output and exited 0; `git diff --check` emitted no output.

Changed-schema negative proof:

```bash
cp schemas/LifecycleResultDto.json /tmp/colui-lifecycle-schema.json
perl -0pi -e 's/"success"/"outcome"/' schemas/LifecycleResultDto.json
bash scripts/verify-increment-3.sh
```

Result: exit 1 with recursive diff showing committed `outcome` versus generated `success`; original schema restored afterward. Temporary comparison tree was cleaned by verifier trap.
