# Final Fix Report

Status: **GREEN**

Base: `e42155929c7853fc561ff54759dccd2181e46d5f`.
No subagents used.

## RED

### Lifecycle restore race

Command:

```sh
cargo test -p colui-app --test lifecycle restore_between_request_and_lease_uses_profile_reread_under_lease -- --exact
```

Result: exit 101. Runtime received `/tmp/demo`; expected restored `/tmp/restored`.

### Runtime association

Command:

```sh
cargo test -p colui-app --test status
```

Result: exit 101, 6 passed and 2 failed. Same-name alternate tuple incorrectly projected one container without ambiguity; zero full-tuple matches incorrectly emitted `ambiguous_runtime_association`.

### Diagnostics UI

Command:

```sh
npx --yes pnpm@9.15.5 exec vitest run src/features/diagnostics/__tests__/DiagnosticsView.test.tsx
```

Result: exit 1, 9 passed and 1 failed. Accessible `Import` heading absent; retained import, definition, and journal projections were not rendered.

### Definition race

Initial integration compile after introducing canonical registry ownership failed because existing `DefinitionCache::new` call sites lacked the new `ProfileReader` owner. Behavioral regressions then cover restore between stale caller read and definition lease, plus equal revision/different SHA invalidation. No artificial pre-fix behavioral result was manufactured after production work began.

## GREEN

Focused command:

```sh
cargo test -p colui-adapters --test definitions && cargo test -p colui-app --test lifecycle && cargo test -p colui-app --test status && npx --yes pnpm@9.15.5 exec vitest run src/features/diagnostics/__tests__/DiagnosticsView.test.tsx
```

Result: exit 0. Definitions 13/13, lifecycle 9/9, status 8/8, DiagnosticsView 10/10.

Hermetic Rust:

```sh
cargo test --workspace
```

Result: exit 0; all workspace unit, integration, DTO/schema, and doc tests passed.

Strict Rust:

```sh
cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings
```

Result: exit 0, no warnings.

Frontend and contracts:

```sh
npx --yes pnpm@9.15.5 test && npx --yes pnpm@9.15.5 test:contracts && npx --yes pnpm@9.15.5 typecheck && npx --yes pnpm@9.15.5 lint && npx --yes pnpm@9.15.5 build && npx --yes pnpm@9.15.5 test:browser
```

Result: exit 0. Frontend 147/147; contracts 42/42; typecheck, lint, and build passed; browser 2/2.

Boundaries:

```sh
bash scripts/check-boundaries.sh --self-test
```

Result: exit 0. Structural, parser, and dependency checks passed.

Live Docker:

```sh
docker info --format '{{.ServerVersion}}'
cargo test -p colui-adapters --features docker-tests --test docker -- --nocapture
```

Result: Docker `29.5.2`; exit 0, 7/7 passed.

## Gate History

One parallel frontend run timed out 10 tests while strict Rust compilation competed for CPU. Same run also found one Clippy `cmp_owned` warning in new test code. Warning was fixed; strict Rust and complete frontend chains then passed sequentially. No timeout was raised and no test was skipped.

Fresh parallel final verification reproduced the previously documented intermittent `recovery_file_lease_excludes_another_manager` post-release reacquisition failure once. Five isolated exact reruns passed, then a sequential full `cargo test --workspace` plus strict Clippy rerun passed. No speculative lock change was made; intermittency remains a concern.
