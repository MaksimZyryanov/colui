# Task 3 Report

## Status

Implemented profile application ports and CRUD use cases for Task 3 only.

## Red Evidence

Wrote integration tests in `crates/colui-app/tests/profiles.rs` before production implementation, covering read-only list/get, create, update/remove revision checks, typed not-found errors, immutable IDs, and revision increments.

Command:

```text
cargo test -p colui-app --test profiles
```

Result after removing unimplemented production modules:

```text
error[E0432]: unresolved import `colui_app`
error[E0433]: cannot find module or crate `colui_app`
```

Failure matched missing Task 3 crate API. Initial mandatory two-test red run also failed for the same missing crate/API reason.

## Green Evidence

Focused command:

```text
cargo test -p colui-app --test profiles
```

Result: 10 tests passed, 0 failed.

Workspace command:

```text
cargo test --workspace
```

Result: domain 12 tests passed, app 10 tests passed, 0 failed; unit and doc tests passed.

Additional verification:

```text
cargo fmt --all -- --check
```

Both passed.

## Files Changed

- `Cargo.toml`: adds `crates/colui-app` workspace member.
- `crates/colui-app/Cargo.toml`: app crate metadata, domain dependency, Tokio async runtime, and UUID test dependency.
- `crates/colui-app/src/lib.rs`: public application exports.
- `crates/colui-app/src/profiles.rs`: `RegistrySnapshot`, reader/store ports, `IdGenerator`, patch type, and profile CRUD use cases.
- `crates/colui-app/tests/profiles.rs`: fake store and focused async behavior tests.

## Behavior

- `ProfileReader::load` exposes read-only snapshot loading.
- `ProfileStore::mutate` is sole mutation port.
- Create generates ID through `IdGenerator`, starts profile revision at 1, and increments registry revision.
- List/get never mutate store.
- Update/remove require expected profile revision.
- Update validates patched draft, preserves profile ID, advances profile revision, and increments registry revision.
- Missing profiles and revision conflicts return stable typed `AppErrorCode` values.

## Scope Review

- No registry filesystem adapter, locking, atomic writes, migration, runtime, IPC, UI, or Tasks 4-6 behavior added.
- No subagents or reviewers used.
- No `.DS_Store` added.
- Pre-existing generated `Cargo.lock` and `target/` remain untracked and excluded from commit.

## Concerns

- `ProfileStore` adapter implementations must ensure mutation closure failure leaves persisted snapshot unchanged; fake fixture naturally does so.
- Registry persistence and duplicate Compose-name policy remain later registry/application work outside Task 3 brief.
