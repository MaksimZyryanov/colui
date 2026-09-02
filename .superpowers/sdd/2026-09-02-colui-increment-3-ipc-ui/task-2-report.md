# Task 2 Report: Tauri Crate and DTO Boundary

## Status

DONE_WITH_CONCERNS

## Files

- `Cargo.toml`: Added `src-tauri` workspace member; existing three workspace members preserved.
- `Cargo.lock`: Resolved Tauri 2, schemars, and related dependencies.
- `src-tauri/Cargo.toml`: Added Tauri 2 crate, typed DTO dependencies, library and binary targets.
- `src-tauri/build.rs`: Added Tauri build hook.
- `src-tauri/src/main.rs`: Registered typed commands through `generate_handler!` and starts Tauri.
- `src-tauri/src/lib.rs`: Added `AppState`, runtime port composition, and command modules.
- `src-tauri/src/dto/{mod.rs,error.rs,profile.rs,request.rs,runtime.rs,status.rs}`: Added serde/schemars transport DTOs, request wrappers, domain conversion, and typed error conversion.
- `src-tauri/src/commands/{mod.rs,profiles.rs,runtime.rs,lifecycle.rs}`: Added thin typed command boundary.
- `src-tauri/tests/dto_contracts.rs`: Added required serialization contract tests.
- `src-tauri/tauri.conf.json`, `src-tauri/icons/{icon.svg,icon.png}`: Added minimal Tauri runtime configuration and required icon asset.
- `crates/colui-app/src/{profiles.rs,lifecycle.rs}`: Added `Send + Sync` port bounds and `?Sized` use-case support for Tauri trait-object state.
- `crates/colui-domain/src/{identity.rs,profile.rs}`: Added boundary accessors and UUID parsing needed by DTO/command mapping.

## TDD Evidence

1. Added DTO contract tests before DTO implementation.
2. Ran `cargo test -p colui-tauri --test dto_contracts`.
3. Expected RED observed: compilation failed because `src-tauri/src/lib.rs` did not exist.
4. Implemented DTOs, state, commands, and Tauri registration.
5. Fixed compiler-discovered trait-object bounds, explicit error conversion ambiguity, and Tauri icon configuration.
6. Re-ran focused tests successfully.

## Exact Commands And Outputs

- `cargo test -p colui-tauri --test dto_contracts`: initial RED, missing `src-tauri/src/lib.rs`; final PASS, 2 passed, 0 failed.
- `cargo fmt --all`: PASS.
- `cargo fmt --all -- --check`: PASS, no output.
- `cargo check -p colui-tauri`: PASS, finished dev profile.
- `cargo test --workspace`: PASS; adapters 28 tests, app 27 tests, domain 21 tests, DTO contracts 2 tests, doc-tests 0 failures.
- `bash scripts/check-boundaries.sh`: PASS, `Dependency boundaries OK`.
- `git diff --check`: PASS, no output.
- Boundary search for `Result<...String`, Bollard, filesystem, process calls under `src-tauri`: no matches.

## Self-Review

- Workspace retains original `crates/colui-domain`, `crates/colui-app`, and `crates/colui-adapters` members.
- Tauri commands return `Result<_, AppErrorDto>`; no `Result<_, String>` exists in `src-tauri`.
- Lifecycle request type contains only `profileId`; no path-bearing lifecycle input exists.
- Lifecycle commands delegate to Task 1 `ApplyProject`, `StopProject`, `TearDownProject`, and `RestartProject`.
- Profile commands delegate to existing `colui-app` profile use cases.
- Runtime commands delegate to existing `RuntimeConnector` and `RuntimeStateReader` ports.
- DTO structs and enums derive `Serialize`, `Deserialize`, and `JsonSchema`, with explicit serde naming.
- No domain rules, Bollard, filesystem, or process calls exist in `colui-tauri`.

## Concerns

- Existing Increment 1/2 application APIs expose no profile-draft inspection or project-status projection use case. `inspect_profile_draft` and `get_project_status` therefore return typed `protocol_mismatch` placeholders instead of inventing domain logic or direct adapter calls. They require a later application-port task before becoming functional.
- Tauri requires an icon during `generate_context!`; `src-tauri/icons/icon.png` is included as a minimal build asset.
- Schema generation and committed schemas/fixtures are outside this Task 2 brief and remain for subsequent contract work.

## Commit

Commit with plan-required message: `feat(tauri): add typed ipc dto boundary`.

---

## Review Fix Report

### Status

FIXED

### Changes

- Wired `AppState` into Tauri's `Builder::setup` with the JSON profile registry, UUID generator, shared runtime gateway, and all command registrations. `main` now calls the single wired `run` path.
- Added `InspectProfileDraft`, which uses existing DTO-to-domain conversion and `validate_draft`; invalid drafts return `ProfileValidationDto` issues without writes or filesystem access.
- Added minimal `ProjectStatusReader` and `GetProjectStatus` application contracts. The use case resolves `profileId` through `ProfileReader` before one single-shot runtime read.
- Implemented the runtime status port on `RuntimeGateway` using its verified session and existing Docker API port. Disconnected/non-ready sessions map to runtime `unavailable`; ready sessions return profile-filtered container presence/activity/counts. Definition remains `unchecked`; operation remains absent. No coordinator, polling, generation, cache, or Increment 4 behavior was added.
- Corrected approved status wire values to kebab-case, including `all-running`, `none-running`, and lifecycle operation `tear-down`. Stable errors remain snake_case; struct fields and tagged runtime states remain camelCase.
- Added RFC3339 validation on serialization and deserialization for `connectedAt`, `observedAt`, and `startedAt`. Runtime-generated timestamps now use RFC3339.
- Replaced forced lifecycle success mapping with `From<LifecycleResult>`, preserving application `success` exactly.
- Kept `colui-tauri` limited to state composition, input conversion, use-case calls, DTO mapping, and typed errors. No direct Bollard, filesystem, process, or domain policy was added to command/DTO modules.

### Focused Tests

- `crates/colui-app/tests/profiles.rs`: invalid draft reports existing domain validation issue.
- `crates/colui-app/tests/status.rs`: status use case resolves stored profile before calling runtime status port.
- `src-tauri/tests/dto_contracts.rs`: approved enum values, all stable error codes, malformed RFC3339 rejection, and lifecycle `success: false` preservation.

### TDD Evidence

- Initial `cargo test -p colui-app --test profiles --test status` failed with unresolved `InspectProfileDraft`, `GetProjectStatus`, `ProjectStatus`, `ProjectStatusReader`, and related contracts.
- Initial `cargo test -p colui-tauri --test dto_contracts` failed because `LifecycleResultDto: From<LifecycleResult>` was missing.
- After minimal implementations, focused and workspace suites passed.

### Exact Commands And Outputs

- `cargo fmt --all -- --check`: PASS, no output.
- `cargo test --workspace`: PASS; adapters unit 1 and registry 27, app lifecycle 4/profile 17/runtime 7/status 1, domain profile 14/runtime 7, Tauri DTO 6, all crate/doc tests 0 failures.
- `cargo test -p colui-tauri --test dto_contracts`: PASS, 6 passed, 0 failed.
- `cargo check -p colui-tauri`: PASS, finished `dev` profile.
- `bash scripts/check-boundaries.sh`: PASS, `Dependency boundaries OK`.
- `git diff --check`: PASS, no output.

### Concerns

- Single-shot ready-runtime status inspects listed containers to read exact Compose project labels because existing `ContainerInstance` omits project labels. This stays in the adapter/runtime port and adds no polling, but cost is one inspect request per listed container per explicit status call.
- Definition projection intentionally remains `unchecked` with no revision/service count because definition inspection/cache is outside existing contracts and Increment 3 Task 2 scope.
