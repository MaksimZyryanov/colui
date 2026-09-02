# Task 6 Report

## Status

Completed. Added missing hermetic acceptance coverage in `crates/colui-adapters/tests/runtime.rs` only. No Docker invocation, script modification, production-code modification, or `colui-app` test modification.

## Coverage Added

- `compose_gate_serializes_two_profiles`: starts two concurrent gateway Compose invocations against a deterministic runner and asserts maximum active Compose processes is one.
- `reconnect_invalidates_old_api_observation`: holds an API read across reconnect, verifies reconnect creates a different session ID, and asserts stale read returns `RuntimeUnavailable`.

Existing tests already covered endpoint precedence, environment policy, exact fingerprint parsing and mapping, profile Compose argv, missing-session gates, mismatch diagnostics and API access, runner argv/cwd/env handling, shell metacharacters, output ring bounds, invalid UTF-8 replacement, non-zero exit, timeout process-group termination, reap behavior, and disconnect serialization.

## TDD Evidence

- Baseline focused adapter runtime suite: 18 passed.
- Added tests initially failed to compile because test helper trait imports and shared test-double ownership needed correction.
- Corrected test-only setup; focused suite then passed with 20 tests.
- `cargo fmt --all` applied before final verification.

## Verification

Exact commands from brief all exited 0:

- `cargo fmt --all -- --check`
- `cargo test --workspace`: all workspace tests passed; 90 tests passed, 0 failed, 0 ignored.
- `bash scripts/check-boundaries.sh`: `Dependency boundaries OK`
- `git diff --check`

## Concerns

- Concurrency test uses deterministic 100 ms sleeps to overlap tasks; it verifies gate behavior without wall-clock ordering assumptions.
- Reconnect test uses one shared fake Docker control with controlled read release; it verifies generation invalidation, not network behavior.
- No concerns blocking Task 6 acceptance.

## Review Fixes

- Added `crates/colui-app/tests/runtime.rs` coverage for complete structured Compose invocation delivery and RuntimeConnector lifecycle calls.
- Changed Compose gate test to use distinct `profile-a` and `profile-b` arguments and assert both were observed while maximum active work stayed at one.
- Bounded Notify waits in gateway tests and fake blocking operations with one-second Tokio timeouts.
- Extended reconnect test to assert Ready state and successful API reads after stale operation completion.

## Task 6 Review Fix Verification

- `pnpm test -- src/ui/components/__tests__/primitives.test.tsx`
  - Output: `zsh:1: command not found: pnpm`
- `pnpm typecheck`
  - Output: `zsh:1: command not found: pnpm`
- `npm test -- --run src/ui/components/__tests__/primitives.test.tsx`
  - Output: `Test Files 1 passed (1); Tests 8 passed (8)`
- `npm run typecheck`
  - Output: exited 0
- `npm test`
  - Output: `Test Files 5 passed (5); Tests 35 passed (35)`
- `npm run build`
  - Output: `vite build`; exited 0
- `cargo fmt --all -- --check`
  - Output: exited 0
- `cargo test --workspace`
  - Output: all workspace suites passed; 92 tests passed, 0 failed, 0 ignored
- `bash scripts/check-boundaries.sh`
  - Output: `Dependency boundaries OK`
- `git diff --check`
  - Output: exited 0

Review fixes verified: `Button` requires `aria-label` or `aria-labelledby` for `iconOnly` at type and runtime; root duplicate `ErrorBoundary` mount removed; unused `DropdownMenu` `Button` import removed. `RuntimeInitializer.tsx` unchanged.
