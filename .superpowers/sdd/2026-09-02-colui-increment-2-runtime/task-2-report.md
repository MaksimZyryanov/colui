# Task 2 Report: Application Runtime Ports

## Scope

Defined application-facing runtime contracts in `crates/colui-app` using only domain/application types. No Bollard, Tauri, Tokio runtime, OS process, or filesystem implementation dependency was added to the app crate.

## Changes

- Added `runtime.rs` with:
  - `RuntimeFuture<'a, T>` boxed `Future` alias.
  - `ComposeInvocation` structured executable, argv, working directory, environment, and absolute deadline contract.
  - `ComposeProcessResult` process status, output, independent truncation flags, timeout state, and duration.
  - `DockerApi` async-by-boxed-future list and inspect methods.
  - `ComposeRunner` async-by-boxed-future invocation method.
  - `RuntimeConnector` async-by-boxed-future connect/disconnect methods.
  - `RuntimeStateReader` session state method.
- Re-exported runtime contracts from `lib.rs`.
- Added fake-port integration tests covering shared borrowing, structured Compose invocation, and independent output/timeout state.
- Left `crates/colui-app/Cargo.toml` unchanged because existing dependency set already satisfies boundary requirements.

## TDD Evidence

1. Added fake-port tests before production runtime contracts.
2. Ran `cargo test -p colui-app --test runtime`; compile failed with unresolved runtime exports because contracts were absent.
3. Added minimal contracts and reran focused tests; all 3 tests passed.

## Verification

- `cargo test -p colui-app --test runtime`: 3 passed.
- `cargo test -p colui-app`: 19 tests passed, 0 failed; doc tests passed.
- `bash scripts/check-boundaries.sh`: `Dependency boundaries OK`.
- `cargo fmt --all -- --check`: passed.
- `git diff --check`: passed.

## Concern

Rust cannot define both an associated constructor and an instance accessor with the same name. The brief's example uses `ComposeProcessResult::timed_out(...)` and `result.timed_out()` simultaneously. Constructor is therefore named `from_timeout`; instance accessor retains required `timed_out()` name.

## Reviewer Fix

Changed `RuntimeStateReader::session_state` to return `RuntimeFuture<'_, RuntimeSessionState>`, matching all other async runtime ports and allowing state reads to cross an async boundary without a mutable borrow. Updated fake implementation and added focused tests for asynchronous state reads and typed inspection failure behavior.

### Fix Verification

Command: `cargo test -p colui-app --test runtime`

Output: `running 5 tests`; `test result: ok. 5 passed; 0 failed`.

Command: `bash scripts/check-boundaries.sh`

Output: `Dependency boundaries OK`.
