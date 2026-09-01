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

## Review Follow-up

Review findings fixed without changing public application API.

- Removed Tokio from `[dependencies]`; kept Tokio under `[dev-dependencies]` with test runtime features.
- Confirmed production library builds without Tokio.
- Changed fake store mutation to keep one `MutexGuard` from snapshot read through closure execution and committed write.
- Added `failed_update_leaves_snapshot_revision_and_write_count_unchanged`, asserting failed validation preserves all three values.
- Added `fake_mutation_holds_snapshot_lock_through_closure` and `concurrent_mutation_cannot_enter_before_prior_write` to verify fixture atomicity.

TDD red evidence:

```text
cargo test -p colui-app --test profiles fake_mutation_holds_snapshot_lock_through_closure
```

Before adding required fixture lock-observation helper, compilation failed with:

```text
error[E0599]: no method named `mutation_lock_is_held` found for struct `Arc<FakeProfileStore>`
```

After the test existed, atomic fixture implementation was applied and verified green.

Green evidence:

```text
cargo check -p colui-app --lib
```

Passed.

```text
cargo test -p colui-app --test profiles
```

13 tests passed, 0 failed.

```text
cargo test --workspace
```

13 app tests and 12 domain tests passed; unit and doc tests passed.

```text
cargo fmt --all -- --check
```

Both passed.

Review-fix concerns: fake store remains test-only; production adapter must provide equivalent atomic lock-and-mutate semantics. Generated untracked `Cargo.lock` and `target/` remain excluded.

## Final Review-Fix Verification

```text
cargo fmt --all -- --check
```

Passed.

```text
cargo test -p colui-app --test profiles
```

13 tests passed, 0 failed.

```text
cargo test --workspace
```

13 app tests and 12 domain tests passed; unit and doc tests passed.

```text
git diff --check
```

Passed. Only intended tracked files were staged; pre-existing untracked `Cargo.lock` and `target/` remain excluded.

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

## Review Fixes

Moved Tokio from production dependencies to `dev-dependencies`. Production library compilation was verified without Tokio.

Changed fake `ProfileStore::mutate` to hold one mutex guard across closure execution and snapshot write. This models atomic mutation and serializes concurrent mutations.

Added tests:

- `failed_update_leaves_snapshot_revision_and_write_count_unchanged`
- `concurrent_mutations_are_serialized_by_fake_store`

Red command before fixture fix:

```text
cargo test -p colui-app --test profiles concurrent_mutations_are_serialized_by_fake_store
```

The test initially passed against the old fixture because its scheduling did not reliably overlap. The regression test was then strengthened to spawn concurrent tasks and fixture was changed to hold the lock through mutation. The final test suite demonstrates serialized behavior.

Green commands and results:

```text
cargo check -p colui-app --lib
```

Passed. Confirms production crate compiles with Tokio absent from normal dependencies.

```text
cargo test -p colui-app --test profiles
```

12 tests passed, 0 failed.

```text
cargo test --workspace
```

App 12 tests and domain 12 tests passed; unit and doc tests passed.

```text
cargo fmt --all -- --check
```

Both passed.
