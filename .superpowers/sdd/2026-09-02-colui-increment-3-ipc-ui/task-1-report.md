# Task 1 Report: Application Lifecycle Use Cases

## Status

DONE_WITH_CONCERNS

## Files

- `crates/colui-app/src/lifecycle.rs`: Added application-only lifecycle operation, result, runtime port, shared lookup/execution path, and four ProfileId-only use-case wrappers.
- `crates/colui-app/src/lib.rs`: Exported lifecycle API.
- `crates/colui-app/tests/lifecycle.rs`: Added TDD coverage for profile lookup, operation distinction, context mismatch propagation, and missing-profile short circuit.
- `crates/colui-adapters/src/runtime/gateway.rs`: Added adapter implementation of `LifecycleRuntime`, mapping application operations to Compose verbs and mapping timeout/non-zero outcomes to typed errors. Refactored profile invocation to use one loaded-profile execution helper.

## TDD Evidence

1. Added lifecycle tests before production lifecycle API.
2. Ran `cargo test -p colui-app --test lifecycle`.
3. Expected RED observed: compile failed because lifecycle types and wrappers did not exist.
4. Implemented minimal application and adapter code.
5. Fixed compile-only issues: `Sync` bound required for `Send` lifecycle futures and test UUID construction unavailable without UUID v5 feature.
6. Re-ran focused tests successfully.

## Tests And Commands

- `cargo test -p colui-app --test lifecycle`: PASS, 4 passed, 0 failed.
- `cargo test --workspace`: PASS. colui-adapters unit tests 1 passed; registry 27 passed; colui-app lifecycle 4 passed; profiles 16 passed; app runtime 7 passed; domain profile 14 passed; domain runtime 7 passed; doc tests 0 failures.
- `bash scripts/check-boundaries.sh`: PASS, `Dependency boundaries OK`.
- Combined required command: `cargo test -p colui-app --test lifecycle && cargo test --workspace && bash scripts/check-boundaries.sh`: PASS after `cargo fmt --all`.

## Self-Review

- Lifecycle callers provide only `ProfileId`; profile paths, names, and commands come from stored `ProjectProfile` in the application reader/runtime contract.
- `colui-app` has no adapter, Bollard, or Tauri dependency.
- Application owns `LifecycleOperation`; adapter owns mapping to `ComposeOperation::{Up, Stop, Down, Restart}`.
- Missing profiles return `ProfileNotFound` before runtime access.
- Runtime mismatch is propagated before runner invocation by adapter readiness check.
- Existing adapter-only `RuntimeGateway::invoke_profile` remains available for current adapter tests and accepts adapter `ComposeOperation`; application lifecycle wrappers do not depend on it.

## Concerns

Existing adapter test/support API `RuntimeGateway::invoke_profile` still exposes `ComposeOperation` inside `colui-adapters`. It is not used by `colui-app` and does not create a second application lifecycle path, but later cleanup could route/remove this adapter test-facing API once downstream tests migrate to `LifecycleRuntime`. No functional or boundary test failures remain.

## Commit

Commit with plan-required message: `feat(app): add profile lifecycle use cases`.

## Review Fix Report

### Status

FIXED

### Changes

- Removed public `RuntimeGateway::invoke_profile(ComposeOperation)`, eliminating adapter-level profile lookup as a second lifecycle entry point.
- Migrated adapter lifecycle callers in `tests/runtime.rs` and `tests/docker.rs` to `LifecycleRuntime::run_profile`.
- Kept scaled Docker fixture coverage on explicit `invoke_backend_for_tests` because scaling is outside `LifecycleOperation::Apply`.
- Added adapter integration coverage for Apply, Stop, TearDown, and Restart Compose mappings.
- Added adapter coverage proving ContextMismatch blocks runner invocation.
- Added adapter coverage for timeout and nonzero lifecycle results.
- Fixed snapshot-lock reentrancy in non-ready lifecycle execution, which direct `run_profile` coverage exposed.

### Covering Test Files

- `crates/colui-adapters/tests/runtime.rs`
- `crates/colui-adapters/tests/docker.rs`

### Exact Commands And Outputs

- `cargo fmt --all && cargo test -p colui-adapters --test runtime --features test-support`: PASS, 25 passed, 0 failed.
- `cargo test -p colui-adapters --test docker --features docker-tests`: PASS, 3 passed, 0 failed.
- `cargo test -p colui-app --test lifecycle`: PASS, 4 passed, 0 failed.
- `cargo test --workspace`: PASS, all workspace tests passed; doc-tests 0 failures.
- `bash scripts/check-boundaries.sh`: PASS, `Dependency boundaries OK`.
- `grep invoke_profile` equivalent repository search: no Rust matches.

### Commit

Separate fix commit: `fix(adapters): enforce lifecycle runtime path`.
