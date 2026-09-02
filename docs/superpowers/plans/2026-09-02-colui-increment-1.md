# CoLUI Increment 1 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build CoLUI 2.0's pure domain model, profile use cases, versioned JSON registry, and one-time legacy v1 importer as a tested headless foundation.

**Architecture:** Use a Cargo workspace with `colui-domain`, `colui-app`, and `colui-adapters`; `src-tauri` is reserved for future increments. Domain contains no filesystem/process/runtime dependencies. Application defines ports and profile use cases; adapters implement atomic persistence and legacy import.

**Tech Stack:** Rust stable, Cargo workspace, serde, schemars, uuid, thiserror, async-trait, Tokio, tempfile, serde_json.

**Spec:** `docs/superpowers/specs/2026-09-01-colui-2-design.md`

## Global Constraints

- Target macOS only; do not claim Linux or Windows support.
- `ProfileId` is immutable and independent of display name, Compose namespace, and paths.
- Duplicate display names are valid; duplicate Compose names remain separate and surface as conflicts.
- Profile file order is preserved; paths are not identity.
- Profiles contain no runtime state, container IDs, or transient Docker errors.
- Queries use `ProfileReader`; only mutations use `ProfileStore::mutate`.
- Registry writes use one advisory lock, bounded retry, temp file, fsync, atomic rename, directory fsync, and reread validation.
- Lock timeout defaults to 5 seconds and is configurable from 5 through 10 seconds.
- Corrupt canonical or legacy JSON is never overwritten silently.
- v1 import is read-only against `~/.colui/projects.json`; legacy file remains unchanged.
- No Bollard, Tauri, Docker, process, or shell code belongs in Increment 1.
- Every externally visible failure uses typed `AppError`; no `Result<_, String>`.

## File Map

- Create `Cargo.toml`: workspace members and shared dependency versions.
- Create `crates/colui-domain/Cargo.toml`: pure domain crate manifest.
- Create `crates/colui-domain/src/lib.rs`: public domain module exports.
- Create `crates/colui-domain/src/identity.rs`: `ProfileId`, `DisplayName`, `ComposeProjectName`, `Revision`.
- Create `crates/colui-domain/src/profile.rs`: `ProjectProfile`, `ProfileDraft`, origin, validation.
- Create `crates/colui-domain/src/error.rs`: `AppError`, stable error codes, retryability.
- Create `crates/colui-domain/src/definition.rs`: definition placeholders needed by profile projection boundaries, without runtime adapters.
- Create `crates/colui-domain/tests/profile.rs`: identity and validation tests.
- Create `crates/colui-app/Cargo.toml`: application crate manifest.
- Create `crates/colui-app/src/lib.rs`: ports and use-case exports.
- Create `crates/colui-app/src/profiles.rs`: profile CRUD use cases and reader/store traits.
- Create `crates/colui-app/tests/profiles.rs`: fake-store application tests.
- Create `crates/colui-adapters/Cargo.toml`: filesystem adapter manifest.
- Create `crates/colui-adapters/src/lib.rs`: adapter exports.
- Create `crates/colui-adapters/src/registry/mod.rs`: registry implementation and lock configuration.
- Create `crates/colui-adapters/src/registry/format.rs`: v2 serde format and conversion.
- Create `crates/colui-adapters/src/registry/import_v1.rs`: isolated legacy parser/converter.
- Create `crates/colui-adapters/tests/registry.rs`: atomic write, corruption, lock, revision, and import tests.
- Create `schemas/.gitkeep`: reserve committed generated-schema directory for Increment 3 DTOs; no IPC schema is generated yet.

### Task 1: Scaffold workspace and domain identity

**Files:**
- Create: `Cargo.toml`
- Create: `crates/colui-domain/Cargo.toml`
- Create: `crates/colui-domain/src/lib.rs`
- Create: `crates/colui-domain/src/identity.rs`
- Test: `crates/colui-domain/tests/profile.rs`

**Interfaces:**
- Produces `ProfileId`, `DisplayName`, `ComposeProjectName`, `Revision`, serde support, and domain module exports.

- [ ] **Step 1: Write failing identity tests**

```rust
#[test]
fn profile_id_is_not_derived_from_name_or_path() {
    let first = ProfileId::new(Uuid::from_u128(1));
    let renamed = ProfileId::new(Uuid::from_u128(1));
    assert_eq!(first, renamed);
}

#[test]
fn compose_name_rejects_invalid_namespace() {
    assert!(ComposeProjectName::try_from("Checkout API").is_err());
    assert!(ComposeProjectName::try_from("checkout_api-2").is_ok());
}
```

- [ ] **Step 2: Run tests and verify failure**

Run: `cargo test -p colui-domain --test profile`
Expected: FAIL because workspace, types, and crate do not exist.

- [ ] **Step 3: Implement minimal workspace and identity types**

Implement `ProfileId(Uuid)` with `new`, `as_uuid`, and `Display`; `DisplayName(String)` with non-empty validation; `ComposeProjectName(String)` with lowercase `[a-z0-9][a-z0-9_-]*` validation; and `Revision(u64)` with `initial()` and `next()`. Derive `Clone`, `Debug`, `Eq`, `Hash`, `Serialize`, and `Deserialize` where safe. Keep constructors fallible for validated newtypes.

- [ ] **Step 4: Run tests and verify pass**

Run: `cargo test -p colui-domain --test profile`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add Cargo.toml crates/colui-domain
git commit -m "feat(domain): add profile identity types"
```

### Task 2: Add profile model and typed errors

**Files:**
- Modify: `crates/colui-domain/src/lib.rs`
- Create: `crates/colui-domain/src/profile.rs`
- Create: `crates/colui-domain/src/error.rs`
- Modify: `crates/colui-domain/tests/profile.rs`

**Interfaces:**
- Consumes identity types from Task 1.
- Produces `ProjectProfile`, `ProfileDraft`, `RegistrationOrigin`, `validate_draft`, `AppError`, and `AppErrorCode`.

- [ ] **Step 1: Add failing model tests**

```rust
#[test]
fn rename_preserves_id_and_compose_namespace() {
    let draft = valid_draft("Checkout", "checkout");
    let profile = ProjectProfile::from_draft(ProfileId::new(Uuid::from_u128(1)), draft).unwrap();
    let renamed = profile.with_display_name(DisplayName::try_from("Payments").unwrap());
    assert_eq!(renamed.id, profile.id);
    assert_eq!(renamed.compose_project_name, profile.compose_project_name);
}

#[test]
fn duplicate_display_names_are_allowed() {
    let first = valid_draft("Local", "checkout");
    let second = valid_draft("Local", "payments");
    assert!(ProjectProfile::from_draft(ProfileId::new(Uuid::from_u128(1)), first).is_ok());
    assert!(ProjectProfile::from_draft(ProfileId::new(Uuid::from_u128(2)), second).is_ok());
}
```

- [ ] **Step 2: Run test and verify failure**

Run: `cargo test -p colui-domain --test profile rename_preserves_id_and_compose_namespace`
Expected: FAIL because profile types and constructors do not exist.

- [ ] **Step 3: Implement profile and error types**

Add `ProjectProfile` fields exactly as spec: id, revision, display name, Compose name, working directory, ordered Compose files, environment files, and registration origin. Add `ProfileDraft` without identity/revision. Implement `from_draft`, `with_display_name`, and `validate_draft`; require at least one Compose file, reject duplicate paths, and leave filesystem existence to adapters. Add all stable error codes, including `ProfileAlreadyRegistered`; implement `AppError::new` and one retryability mapping.

- [ ] **Step 4: Run full domain tests**

Run: `cargo test -p colui-domain`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/colui-domain
git commit -m "feat(domain): define profiles and app errors"
```

### Task 3: Define profile application ports and use cases

**Files:**
- Create: `crates/colui-app/Cargo.toml`
- Create: `crates/colui-app/src/lib.rs`
- Create: `crates/colui-app/src/profiles.rs`
- Create: `crates/colui-app/tests/profiles.rs`

**Interfaces:**
- Consumes `ProjectProfile`, `ProfileDraft`, and `AppError` from `colui-domain`.
- Produces `RegistrySnapshot`, `ProfileReader`, `ProfileStore`, `CreateProfile`, `UpdateProfile`, `RemoveProfile`, `GetProfile`, and `ListProfiles`.

- [ ] **Step 1: Write failing fake-store tests**

```rust
#[tokio::test]
async fn list_profile_reads_without_mutation() {
    let store = FakeProfileStore::with_profiles(vec![]);
    let before = store.write_count();
    let result = ListProfiles::new(&store).execute().await.unwrap();
    assert!(result.is_empty());
    assert_eq!(before, store.write_count());
}

#[tokio::test]
async fn update_requires_expected_revision() {
    let store = FakeProfileStore::with_profiles(vec![profile_revision(3)]);
    let error = UpdateProfile::new(&store).execute(profile_id(), 2, patch()).await.unwrap_err();
    assert_eq!(error.code, AppErrorCode::ProfileRevisionConflict);
}
```

- [ ] **Step 2: Run test and verify failure**

Run: `cargo test -p colui-app --test profiles`
Expected: FAIL because crate, ports, use cases, and fake fixture do not exist.

- [ ] **Step 3: Implement ports and use cases**

Define `RegistrySnapshot { registry_revision: u64, profiles: Vec<ProjectProfile> }`, async `ProfileReader::load()`, and `ProfileStore::mutate()` around that snapshot. Keep reader trait free of mutation methods. Implement create with generated `ProfileId` and revision 1, get/list as read-only, update/remove with expected revision checks, and explicit not-found/conflict errors. Put `IdGenerator` behind a port so tests can use deterministic UUIDs. Use a patch type that changes profile fields but never ID.

- [ ] **Step 4: Run tests and verify pass**

Run: `cargo test -p colui-app --test profiles`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add crates/colui-app
git commit -m "feat(app): add profile use cases"
```

### Task 4: Implement v2 registry format and atomic persistence

**Files:**
- Create: `crates/colui-adapters/Cargo.toml`
- Create: `crates/colui-adapters/src/lib.rs`
- Create: `crates/colui-adapters/src/registry/mod.rs`
- Create: `crates/colui-adapters/src/registry/format.rs`
- Create: `crates/colui-adapters/tests/registry.rs`

**Interfaces:**
- Consumes application profile ports and domain models, including `RegistrySnapshot` from Task 3.
- Produces `JsonProfileRegistry`, `RegistryConfig`, and lock-aware `ProfileReader`/`ProfileStore` implementations.

- [ ] **Step 1: Write failing registry tests**

```rust
#[tokio::test]
async fn mutation_writes_v2_and_preserves_order() {
    let registry = test_registry();
    let result = registry.mutate(|draft| {
        draft.add(profile_with_files(["compose.yml", "compose.local.yml"]));
        Ok(())
    }).await.unwrap();
    assert_eq!(result.profiles[0].compose_files.len(), 2);
    assert_eq!(read_json().schema_version, 2);
}

#[tokio::test]
async fn corrupt_registry_is_not_overwritten() {
    write_canonical_bytes(b"{broken");
    let registry = test_registry();
    let error = registry.load().await.unwrap_err();
    assert_eq!(error.code, AppErrorCode::RegistryCorrupt);
    assert_eq!(read_canonical_bytes(), b"{broken");
}
```

- [ ] **Step 2: Run test and verify failure**

Run: `cargo test -p colui-adapters --test registry mutation_writes_v2_and_preserves_order`
Expected: FAIL because adapter crate and registry do not exist.

- [ ] **Step 3: Implement v2 format and reader**

Create serde-only `RegistryFile { schema_version, registry_revision, profiles }`. Convert between file records and domain profiles without persisting runtime data. On missing canonical file, return an empty revision-0 snapshot without writing during `load`.

- [ ] **Step 4: Implement lock and atomic mutation**

Create parent directory, acquire exclusive lock with bounded exponential retry, default five-second timeout, and config range five to ten seconds. Read current bytes under lock, parse before mutation, apply one closure, increment registry revision only for changed content, serialize deterministically, write sibling temp file, fsync file, atomic rename, fsync directory, reread, validate, and release lock. Never overwrite malformed canonical bytes. Use unique temp names to avoid cross-process collisions.

- [ ] **Step 5: Add persistence failure tests**

Test unchanged mutation performs no write; expected revision conflict leaves bytes unchanged; invalid paths remain persisted; lock held beyond configured timeout returns retryable `RegistryLocked`; and reread confirms the committed bytes.

- [ ] **Step 6: Run adapter tests**

Run: `cargo test -p colui-adapters --test registry`
Expected: PASS.

- [ ] **Step 7: Commit**

```bash
git add crates/colui-adapters
git commit -m "feat(registry): add atomic v2 persistence"
```

### Task 5: Add one-time legacy v1 import

**Files:**
- Create: `crates/colui-adapters/src/registry/import_v1.rs`
- Modify: `crates/colui-adapters/src/registry/mod.rs`
- Modify: `crates/colui-adapters/tests/registry.rs`

**Interfaces:**
- Consumes legacy v1 JSON bytes and `IdGenerator`.
- Produces v2 profiles with `RegistrationOrigin::Migrated` and import diagnostics.

- [ ] **Step 1: Write failing import tests**

```rust
#[test]
fn import_preserves_entry_and_file_order_without_touching_source() {
    let source = br#"[{"name":"Checkout","working_dir":"/tmp/checkout","config_files":["compose.yml","local.yml"],"env_files":[".env"]}]"#;
    let imported = import_v1(source, deterministic_ids()).unwrap();
    assert_eq!(imported[0].display_name.as_str(), "Checkout");
    assert_eq!(imported[0].compose_files[1], PathBuf::from("local.yml"));
    assert_eq!(imported[0].registration_origin, RegistrationOrigin::Migrated);
}

#[test]
fn invalid_names_are_normalized_and_collisions_are_not_merged() {
    let source = legacy_entries_with_names(["Checkout API", "checkout-api"]);
    let imported = import_v1(source, deterministic_ids()).unwrap();
    assert_eq!(imported.len(), 2);
    assert_eq!(imported[0].compose_project_name, imported[1].compose_project_name);
}
```

- [ ] **Step 2: Run test and verify failure**

Run: `cargo test -p colui-adapters --test registry import_preserves_entry_and_file_order_without_touching_source`
Expected: FAIL because importer does not exist.

- [ ] **Step 3: Implement isolated parser and conversion**

Parse only the known v1 fields. Preserve entry and file order. Map name to display name, normalize a lowercase Compose namespace by replacing invalid runs with `-` and trimming separators, and use `imported-<first-eight-profile-id-hex>` when normalization is empty. Generate new IDs, revision 1, and Migrated origin. Keep collisions as separate profiles; downstream summary/discovery marks them as conflicts.

- [ ] **Step 4: Implement first-start orchestration**

When v2 canonical registry is absent and legacy source exists, read source without writing it, create a source backup, import profiles, then write v2 through the normal atomic writer. On malformed legacy JSON, create usable empty v2 state and return import diagnostics; never silently discard or overwrite source bytes. Do not import if v2 already exists.

- [ ] **Step 5: Run import and full workspace tests**

Run: `cargo test --workspace`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/colui-adapters
git commit -m "feat(registry): import legacy project catalog"
```

### Task 6: Add Increment 1 verification gates

**Files:**
- Create: `schemas/.gitkeep`
- Modify: `Cargo.toml`
- Create: `scripts/check-boundaries.sh`

**Interfaces:**
- Consumes all workspace crates from Tasks 1-5.
- Produces repeatable local checks for dependency boundaries and acceptance criteria available before runtime work.

- [ ] **Step 1: Add boundary checks**

Implement a shell check that fails if `colui-domain` manifests mention Bollard, Tauri, Tokio, filesystem, or process dependencies, if `colui-app` mentions Bollard/Tauri, or if adapters expose direct Tauri dependencies. Keep this check limited to dependency manifests; Rust compilation remains the primary boundary enforcement.

- [ ] **Step 2: Run final Increment 1 verification**

Run:

```bash
cargo fmt --all -- --check
cargo test --workspace
bash scripts/check-boundaries.sh
git diff --check
```

Expected: all commands exit 0; tests cover identity, domain validation, read-only query behavior, revision conflicts, corruption safety, lock timeout, atomic persistence, and lossless v1 import.

- [ ] **Step 3: Commit**

```bash
git add Cargo.toml schemas/.gitkeep scripts/check-boundaries.sh
git commit -m "test: add increment one verification gates"
```

## Self-Review Checklist

- [ ] Every spec requirement for domain, profile registry, typed errors, v1 import, and Increment 1 tests maps to a task above.
- [ ] No task depends on RuntimeGateway, Bollard, Tauri, React, or generated IPC DTOs.
- [ ] `ProfileAlreadyRegistered` is reserved for discovery race handling in Increment 5, not incorrectly implemented as a profile CRUD duplicate error here.
- [ ] Atomic write sequence and five-to-ten-second lock range match approved spec.
- [ ] All path order, identity, corruption, backup, revision, and offline-safe profile requirements have explicit tests.
- [ ] No placeholders or vague test instructions remain.
