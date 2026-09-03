# CoLUI Increment 4 Inventory and Definitions Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add one coordinated runtime inventory, separate Compose definition cache, per-profile lifecycle locking, generation-safe IPC, and one frontend polling query.

**Architecture:** `InventoryCoordinator`, `DefinitionCache`, and `OperationLockManager` are independent peer owners. `RuntimeGateway` retains session ownership and global Compose semaphore but does not own inventory, definitions, or per-profile locks. Lifecycle use cases acquire the shared operation guard, invoke the gateway, refresh inventory exactly once on Compose success, and return the resulting snapshot.

**Tech Stack:** Rust workspace (`colui-domain`, `colui-app`, `colui-adapters`, `src-tauri`), Tokio, Bollard, Tauri 2, Serde/Schemars, React 18, TypeScript strict, TanStack Query v5, Zod, Vitest, Testing Library, pnpm.

**Spec:** `docs/superpowers/specs/2026-09-03-colui-increment-4-inventory-design.md`

## Global Constraints

- Generation `0` is reserved for pre-observation state; first successful inventory generation is `1`.
- `hasSnapshot: false` is required for pre-observation generation `0`; first success cannot be rejected as equal generation.
- `InventoryCoordinator` is sole owner of fast refresh, publication, generation, coalescing, and automatic backoff.
- Every fast refresh uses one Docker `list all containers` call and zero inspect/config calls.
- `DockerApi::list_containers` returns `Vec<ContainerObservation>`; `DockerApiAdapter` extracts approved Compose labels.
- `RuntimeInventory` is immutable; runtime session generation remains private to `RuntimeGateway`.
- Lower generations are rejected; equal generations return existing object/reference without cache replacement or subscriber notification.
- `DefinitionCache` is independent, keyed by `(ProfileId, Revision)`, with 60-second stale expiry.
- Definition revision hashes canonical `docker compose config --format json` output: recursively sorted object keys, preserved array order/scalars, compact UTF-8 JSON bytes.
- `OperationLockManager` is the only per-profile lifecycle/definition lock owner; guards implement `Send + Sync` and release once through `Drop`.
- Lifecycle input remains `{ profileId }`; backend resolves all profile paths.
- Successful lifecycle returns its already-produced inventory and does not invalidate/refetch inventory queries.
- Existing global Compose semaphore remains one permit.
- Runtime and definition errors remain separate; profiles and retained snapshots remain visible during outage.
- No Discovery UI, Diagnostics, standalone lifecycle/log/port UI, Docker Events, remote/multi-context support, Images, SQLite, generic resource framework, or visual redesign.
- Ordinary verification requires no Docker/network. Existing feature-gated smoke reuses its fixture only for real Compose-label parsing and definition revision change.

---

## File Map

### Domain and application

- Modify `crates/colui-domain/src/definition.rs`: add immutable inventory values, `InventoryFreshness`, `ProjectRuntimeSnapshot`, `ContainerObservation`, and `ComposeContainerMetadata`; export from `src/lib.rs`.
- Modify `crates/colui-app/src/runtime.rs`: change `DockerApi::list_containers` to return `Vec<ContainerObservation>`; retain inspect and runtime ports.
- Create `crates/colui-app/src/inventory.rs`: `InventoryReader`, `InventoryRefresher`, future aliases, and coordinator-facing projection contracts.
- Create `crates/colui-app/src/definitions.rs`: `DefinitionReader`, `DefinitionRefresher`, future aliases, and definition projection contract.
- Create `crates/colui-app/src/operations.rs`: `OperationKind` alias/re-export, `OperationLockManager`, guard contracts, and operation projection state.
- Modify `crates/colui-app/src/lifecycle.rs`: inject lock manager and inventory refresher; extend `LifecycleResult` with returned inventory.
- Modify `crates/colui-app/src/status.rs` and `src/lib.rs`: compose status from shared inventory/definition readers and export new ports.

### Adapters

- Modify `crates/colui-adapters/src/runtime/docker_api.rs`: map Bollard summaries to `ContainerObservation` with official labels.
- Modify `crates/colui-adapters/src/runtime/gateway.rs`: remove per-profile lock/inventory responsibilities if introduced; retain session counter, global Compose semaphore, lifecycle execution, and `DockerApi` observation port.
- Create `crates/colui-adapters/src/inventory.rs`: `InventoryCoordinator` state machine, coalescing, publication, generations, normalization/grouping, retention, and backoff.
- Create `crates/colui-adapters/src/definitions.rs`: `DefinitionCache`, Compose config invocation/parser, canonical JSON hashing, TTL/revision invalidation.
- Create `crates/colui-adapters/src/operations.rs`: concrete atomic lock manager and RAII guards.
- Modify `crates/colui-adapters/src/lib.rs`: export new adapters.

### Tauri and schemas

- Create `src-tauri/src/dto/inventory.rs`: inventory, observations, freshness, project snapshot, and container DTOs.
- Create `src-tauri/src/dto/definition.rs`: definition/service DTOs and mappings.
- Modify `src-tauri/src/dto/status.rs`: generation-bearing lifecycle mapping and compatibility status mapping.
- Modify `src-tauri/src/dto/mod.rs`: exports and schema registration.
- Create/modify `src-tauri/src/commands/inventory.rs`: `refresh_inventory`.
- Create/modify `src-tauri/src/commands/definitions.rs`: `refresh_project_definition` and detail integration.
- Modify `src-tauri/src/commands/runtime.rs`: shared inventory-backed status.
- Modify `src-tauri/src/commands/lifecycle.rs` and `src-tauri/src/lib.rs`: wire shared owners and command registration.
- Modify `src-tauri/src/schema_generation.rs`, `src-tauri/tests/dto_contracts.rs`, `schemas/*.json`, and `schemas/fixtures/*.json`.

### Frontend and verification

- Modify `src/ipc/{commands.ts,schemas.ts,types.ts}`: inventory/definition commands and lifecycle response.
- Create `src/features/runtime/hooks/useInventory.ts`: one visible-window polling query.
- Modify `src/features/projects/{query-keys.ts,ProjectsView.tsx}` and `src/features/projects/hooks/{useProjectStatus.ts,useLifecycleActions.ts,useProfileMutations.ts}`: inventory projections, guarded lifecycle publication, definition invalidation.
- Create `src/features/runtime/__tests__/useInventory.test.tsx` and extend project/query tests.
- Modify `src/ipc/__tests__/{contracts.test.ts,commands.test.ts,mock-backend.test.ts}` and `src/ipc/mock-backend.ts`.
- Modify `scripts/check-boundaries.sh`; create `scripts/verify-increment-4.sh`.

---

## Task 1: Add Domain Inventory Contracts

**Files:**
- Modify: `crates/colui-domain/src/definition.rs`, `crates/colui-domain/src/lib.rs`
- Test: `crates/colui-domain/src/definition.rs` unit tests or `crates/colui-domain/tests/inventory.rs`

**Interfaces:**
- Consumes: existing `ContainerInstance`, `RuntimeSessionId`, `DaemonFingerprint`, `AppError`.
- Produces: `RuntimeInventory`, `InventoryFreshness`, `ProjectRuntimeSnapshot`, `ContainerObservation`, `ComposeContainerMetadata`.

- [ ] **Step 1: Write failing value tests**

```rust
#[test]
fn pre_observation_is_distinct_from_published_generation_zero() {
    let value = RuntimeInventory::unavailable();
    assert!(!value.has_snapshot);
    assert_eq!(value.generation, 0);
    assert!(value.observed_at.is_none());
}

#[test]
fn observation_keeps_compose_metadata_separate_from_instance() {
    let observation = ContainerObservation::new(instance(), Some(ComposeContainerMetadata::project("checkout")));
    assert_eq!(observation.compose.unwrap().project, "checkout");
    assert!(observation.instance.service_name.is_none());
}
```

- [ ] **Step 2: Run focused test and verify failure**

Run: `cargo test -p colui-domain inventory`

Expected: FAIL because inventory contracts do not exist.

- [ ] **Step 3: Implement immutable domain values**

Use owned fields, derive `Clone, Debug, Eq, PartialEq, Serialize, Deserialize`, make pre-observation timestamps/session/fingerprint optional, and expose constructor/accessors without mutation methods. Keep `ContainerInstance` free of generic labels. Add `has_snapshot` to distinguish generation zero absence from a published generation.

- [ ] **Step 4: Run tests and boundary check**

Run: `cargo test -p colui-domain inventory && bash scripts/check-boundaries.sh`

Expected: PASS; domain imports no Tokio, Bollard, Tauri, filesystem, or process APIs.

- [ ] **Step 5: Commit**

```bash
git add crates/colui-domain
git commit -m "feat(domain): add inventory contracts"
```

## Task 2: Update Application Ports and Lifecycle Result

**Files:**
- Modify: `crates/colui-app/src/runtime.rs`, `crates/colui-app/src/lifecycle.rs`, `crates/colui-app/src/status.rs`, `crates/colui-app/src/lib.rs`
- Create: `crates/colui-app/src/inventory.rs`, `crates/colui-app/src/definitions.rs`, `crates/colui-app/src/operations.rs`
- Test: `crates/colui-app/tests/inventory_contracts.rs`, `crates/colui-app/tests/lifecycle.rs`

**Interfaces:**
- Consumes: Task 1 domain values and existing `ProfileReader`, `LifecycleRuntime`.
- Produces: `DockerApi::list_containers() -> RuntimeFuture<'_, Vec<ContainerObservation>>`; `InventoryReader`; `InventoryRefresher`; `DefinitionReader`; `DefinitionRefresher`; `OperationLockManager`; `LifecycleResult { profile_id, success, inventory }`.

- [ ] **Step 1: Write failing port tests**

```rust
#[tokio::test]
async fn concurrent_lifecycle_acquisition_conflicts_per_profile() {
    let locks = FakeOperationLocks::new();
    let first = locks.acquire_lifecycle(profile_id(), OperationKind::Apply).await.unwrap();
    let error = locks.acquire_lifecycle(profile_id(), OperationKind::Stop).await.unwrap_err();
    assert_eq!(error.code, AppErrorCode::OperationConflict);
    drop(first);
}

#[test]
fn lifecycle_result_exposes_inventory_generation() {
    let result = LifecycleResult { profile_id: profile_id(), success: true, inventory: RuntimeInventory::unavailable() };
    assert_eq!(result.inventory.generation, 0);
}
```

- [ ] **Step 2: Run focused tests and verify failure**

Run: `cargo test -p colui-app --test inventory_contracts`

Expected: FAIL because ports and result shape are absent.

- [ ] **Step 3: Implement exact application contracts**

Define `InventoryFuture`, `DefinitionFuture`, and `OperationFuture` as boxed `Send` futures. Define `OperationKind` in `operations.rs` as the existing four lifecycle values. Make `LifecycleOperationGuard` and `DefinitionLoadGuard` `Send + Sync` RAII contracts. Change lifecycle orchestration to accept shared lock/inventory ports while preserving `{ profileId }` lookup semantics.

- [ ] **Step 4: Migrate existing fakes and tests**

Update all `DockerApi` fake implementations to return `ContainerObservation`; update existing lifecycle fixtures to construct `RuntimeInventory`.

- [ ] **Step 5: Run tests**

Run: `cargo test -p colui-app --test inventory_contracts && cargo test -p colui-app --test lifecycle && cargo test --workspace`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/colui-app
git commit -m "feat(app): add inventory and operation ports"
```

## Task 3: Normalize Docker Observations

**Files:**
- Modify: `crates/colui-adapters/src/runtime/docker_api.rs`, `crates/colui-adapters/src/runtime/gateway.rs`, `crates/colui-adapters/src/runtime/mod.rs`
- Test: `crates/colui-adapters/tests/runtime.rs`, `crates/colui-adapters/tests/inventory.rs`

**Interfaces:**
- Consumes: Task 1 `ContainerObservation`; Bollard `ContainerSummary`.
- Produces: gateway `DockerApi` list returning observations; one-call label mapping with no inspect calls.

- [ ] **Step 1: Write failing normalization tests**

```rust
#[tokio::test]
async fn list_returns_compose_metadata_without_inspect() {
    let gateway = gateway_with_summary(compose_summary("checkout", "web"));
    let values = gateway.list_containers().await.unwrap();
    assert_eq!(values[0].compose.as_ref().unwrap().project, "checkout");
    assert_eq!(gateway.inspect_call_count(), 0);
}

#[test]
fn missing_project_label_is_standalone_observation() {
    let value = normalize_summary(standalone_summary());
    assert!(value.compose.is_none());
}
```

- [ ] **Step 2: Run focused tests and verify failure**

Run: `cargo test -p colui-adapters --test inventory normalization`

Expected: FAIL because list returns `ContainerInstance` and discards labels.

- [ ] **Step 3: Implement adapter-owned mapping**

Extract only `com.docker.compose.project`, `service`, `working_dir`, and `config-files`; parse config files in Docker-provided order; retain existing state/image/name/port mapping. Change `DockerControl::list` and application `DockerApi` consistently. Keep inspect mapping for later on-demand operations.

- [ ] **Step 4: Run adapter tests**

Run: `cargo test -p colui-adapters --test runtime && cargo test -p colui-adapters --test inventory`

Expected: PASS; existing runtime connection and list tests remain valid after fixture migration.

- [ ] **Step 5: Commit**

```bash
git add crates/colui-adapters crates/colui-app
git commit -m "feat(runtime): preserve compose container labels"
```

## Task 4: Implement Atomic OperationLockManager

**Files:**
- Create: `crates/colui-adapters/src/operations.rs`
- Modify: `crates/colui-adapters/src/lib.rs`, `crates/colui-app/src/operations.rs`
- Test: `crates/colui-adapters/tests/operations.rs`

**Interfaces:**
- Consumes: Task 2 lock contracts and existing `AppErrorCode::OperationConflict`.
- Produces: concrete `OperationLockManager` with atomic lifecycle pending priority and definition lease.

- [ ] **Step 1: Write failing lock tests**

```rust
#[tokio::test]
async fn lifecycle_pending_blocks_definition_and_duplicate_lifecycle() {
    let locks = TestOperationLockManager::new();
    let lifecycle = locks.acquire_lifecycle(id(), OperationKind::Apply).await.unwrap();
    assert!(matches!(locks.acquire_definition(id()), Err(DefinitionBusy::LifecyclePending)));
    assert_eq!(locks.acquire_lifecycle(id(), OperationKind::Stop).await.unwrap_err().code, AppErrorCode::OperationConflict);
    drop(lifecycle);
}

#[tokio::test]
async fn definition_lease_releases_after_guard_drop() {
    let locks = TestOperationLockManager::new();
    let definition = locks.acquire_definition(id()).unwrap();
    drop(definition);
    assert!(locks.acquire_definition(id()).is_ok());
}
```

- [ ] **Step 2: Run and verify failure**

Run: `cargo test -p colui-adapters --test operations`

Expected: FAIL because concrete lock manager does not exist.

- [ ] **Step 3: Implement atomic state machine**

Use one async mutex-protected map keyed by `ProfileId`. Lifecycle acquisition marks `pending_lifecycle`, returns conflict for another lifecycle, waits for a definition guard to release, then changes state to active lifecycle. Definition acquisition succeeds only when no lifecycle is pending/active and no definition load is active. Guards own release and notify waiters. Do not add lock maps to `RuntimeGateway`, coordinator, or frontend.

- [ ] **Step 4: Verify lock races**

Run: `cargo test -p colui-adapters --test operations -- --nocapture`

Expected: PASS for duplicate, release, pending-priority, cancellation, and different-profile cases.

- [ ] **Step 5: Commit**

```bash
git add crates/colui-app/src/operations.rs crates/colui-adapters
git commit -m "feat(app): add profile operation locks"
```

## Task 5: Implement InventoryCoordinator Core

**Files:**
- Create: `crates/colui-adapters/src/inventory.rs`
- Modify: `crates/colui-adapters/src/lib.rs`, `crates/colui-adapters/src/runtime/gateway.rs`
- Test: `crates/colui-adapters/tests/inventory.rs`

**Interfaces:**
- Consumes: Task 2 inventory ports, Task 3 observations, RuntimeGateway session identity/fingerprint.
- Produces: `InventoryCoordinator::current_inventory()`, `refresh()`, subscriber publication, one-list normalization/grouping.

- [ ] **Step 1: Write failing coalescing/generation tests**

```rust
#[tokio::test]
async fn concurrent_refreshes_share_one_list_and_generation() {
    let docker = BlockingFakeDocker::with_one_observation(compose_observation("checkout"));
    let coordinator = coordinator(docker.clone());
    let (a, b) = tokio::join!(coordinator.refresh(), coordinator.refresh());
    assert_eq!(docker.list_calls(), 1);
    assert_eq!(a.unwrap().generation, b.unwrap().generation);
    assert_eq!(a.unwrap().generation, 1);
}

#[tokio::test]
async fn refresh_failure_retains_last_snapshot_and_backoff_is_bounded() {
    let docker = FakeDocker::success_then_error();
    let coordinator = coordinator_with_clock(docker);
    assert_eq!(coordinator.refresh().await.unwrap().generation, 1);
    let retained = coordinator.refresh().await.unwrap();
    assert!(retained.has_snapshot);
    assert_eq!(retained.generation, 1);
    assert_eq!(retained.freshness, InventoryFreshness::Stale);
}
```

- [ ] **Step 2: Run and verify failure**

Run: `cargo test -p colui-adapters --test inventory`

Expected: FAIL because coordinator does not exist.

- [ ] **Step 3: Implement refresh state and coalescing**

Store `current_snapshot: Option<Arc<RuntimeInventory>>`, `next_generation: u64`, one in-flight shared result, failure count, next automatic retry deadline, and subscriber list. Install in-flight work under a short mutex before awaiting Docker. Capture runtime session ID/fingerprint before list and reject response if either changes. Explicit refresh bypasses automatic backoff; automatic refresh returns retained state while blocked.

- [ ] **Step 4: Implement normalization/publication**

Group observations with Compose metadata into `ProjectRuntimeSnapshot`; route observations without project metadata to standalone. Build success with generation `next_generation + 1`, `has_snapshot: true`, timestamp, current session metadata, and normalized full `containers`. Publish only strictly newer generation, notify once, and never mutate a published snapshot.

- [ ] **Step 5: Implement failure/backoff**

Use delays `1s, 2s, 4s, 8s, 10s cap`; reset after success. Preserve last success and expose typed error/freshness through retained projections. Before first success return generation `0`, `has_snapshot: false`, empty collections, unavailable freshness, and absent timestamp/session/fingerprint.

- [ ] **Step 6: Run focused tests**

Run: `cargo test -p colui-adapters --test inventory && cargo test --workspace`

Expected: PASS; no fast refresh invokes inspect or Compose.

- [ ] **Step 7: Commit**

```bash
git add crates/colui-adapters crates/colui-app
git commit -m "feat(inventory): coordinate runtime snapshots"
```

## Task 6: Implement DefinitionCache

**Files:**
- Create: `crates/colui-adapters/src/definitions.rs`
- Modify: `crates/colui-adapters/src/lib.rs`
- Test: `crates/colui-adapters/tests/definitions.rs`

**Interfaces:**
- Consumes: Task 2 definition/lock ports, profile values, existing `ComposeRunner`.
- Produces: `DefinitionCache::definition`, `refresh_definition`, `invalidate`, canonical config parser/hash.

- [ ] **Step 1: Write failing cache tests**

```rust
#[tokio::test]
async fn matching_revision_within_ttl_avoids_second_config_call() {
    let runner = FakeComposeRunner::config_json(service_json("web"));
    let cache = cache(runner.clone());
    let profile = profile_with_revision(1);
    cache.refresh_definition(profile.clone()).await.unwrap();
    cache.definition(profile).await.unwrap();
    assert_eq!(runner.config_calls(), 1);
}

#[tokio::test]
async fn revision_change_discards_old_inflight_result() {
    let runner = BlockingConfigRunner::new();
    let cache = cache(runner.clone());
    let first = tokio::spawn(cache.refresh_definition(profile_with_revision(1)));
    runner.release_with_json(service_json("old"));
    first.await.unwrap().unwrap();
    assert!(cache.contains(profile_id(), Revision::new(1)));
    cache.invalidate(profile_id());
    assert!(!cache.contains(profile_id(), Revision::new(1)));
}
```

- [ ] **Step 2: Run and verify failure**

Run: `cargo test -p colui-adapters --test definitions`

Expected: FAIL because DefinitionCache does not exist.

- [ ] **Step 3: Implement cache and canonical hash**

Invoke `docker compose config --format json` through the existing runner. Parse JSON, recursively sort object keys, preserve arrays/scalars, compact-serialize bytes, hash SHA-256, and map services/issues. Store one entry per profile revision, loaded timestamp, state, and services. Add SHA-256 dependency only to `colui-adapters`.

- [ ] **Step 4: Implement TTL and invalidation**

Treat `< 60s` matching entries as valid. Refresh stale/missing entries on foreground/manual access. Invalidate after update and remove. Reject completed loads whose captured revision no longer matches. Failed refresh returns previous services marked stale and retains definition error separately.

- [ ] **Step 5: Implement lifecycle priority**

Acquire `DefinitionLoadGuard` atomically before spawning Compose. On `DefinitionBusy`, return retained stale definition or unchecked without invoking runner. Lifecycle pending priority prevents a new definition lease.

- [ ] **Step 6: Run tests and commit**

Run: `cargo test -p colui-adapters --test definitions && cargo test --workspace`

```bash
git add crates/colui-adapters Cargo.toml Cargo.lock
git commit -m "feat(definitions): add compose config cache"
```

## Task 7: Integrate Lifecycle and Shared Status Projection

**Files:**
- Modify: `crates/colui-app/src/lifecycle.rs`, `crates/colui-app/src/status.rs`, `crates/colui-adapters/src/runtime/gateway.rs`, `src-tauri/src/lib.rs`, `src-tauri/src/commands/runtime.rs`
- Test: `crates/colui-app/tests/lifecycle.rs`, `crates/colui-adapters/tests/integration.rs`

**Interfaces:**
- Consumes: Tasks 4-6 concrete owners.
- Produces: lifecycle lock → gateway → coordinator flow; shared status projection; compatibility fallback.

- [ ] **Step 1: Write failing integration tests**

```rust
#[tokio::test]
async fn successful_lifecycle_refreshes_inventory_once_before_unlock() {
    let fixture = application_fixture();
    let result = fixture.apply(profile_id()).await.unwrap();
    assert!(result.success);
    assert_eq!(fixture.inventory_refresh_calls(), 1);
    assert_eq!(result.inventory.generation, 1);
}

#[tokio::test]
async fn status_before_first_snapshot_returns_unavailable_compatibility_shape() {
    let status = application_without_inventory().status(profile_id()).await.unwrap();
    assert_eq!(status.runtime.presence, RuntimePresence::Unavailable);
    assert_eq!(status.runtime.container_count, 0);
    assert!(status.runtime.observed_at.is_none());
}
```

- [ ] **Step 2: Run focused tests and verify failure**

Run: `cargo test -p colui-app --test lifecycle && cargo test -p colui-adapters --test integration`

Expected: FAIL because lifecycle result and wiring are not coordinator-backed.

- [ ] **Step 3: Implement orchestration**

Inject lock manager and inventory refresher into lifecycle use cases. Acquire lock before gateway, hold through coordinator refresh, return inventory, and release guard on every path. Compose failure produces no refresh. Successful Compose plus failed observation returns `success: true` with retained inventory/error state.

- [ ] **Step 4: Replace per-profile status path**

Build project runtime projection by matching profile Compose namespace against shared project snapshots. Use compatibility fallback before first snapshot. Remove gateway list/inspect loop from `ProjectStatusReader`; retain profile lookup and separate definition projection.

- [ ] **Step 5: Run tests and commit**

Run: `cargo test --workspace && bash scripts/check-boundaries.sh`

```bash
git add crates/colui-app crates/colui-adapters src-tauri
git commit -m "feat(app): wire shared inventory lifecycle"
```

## Task 8: Add Tauri DTOs, Commands, Schemas, and Mock Protocol

**Files:**
- Create: `src-tauri/src/dto/inventory.rs`, `src-tauri/src/dto/definition.rs`, `src-tauri/src/commands/inventory.rs`, `src-tauri/src/commands/definitions.rs`
- Modify: `src-tauri/src/{dto/mod.rs,dto/status.rs,commands/mod.rs,commands/runtime.rs,commands/lifecycle.rs,lib.rs,schema_generation.rs}`
- Modify: `src-tauri/tests/dto_contracts.rs`, `schemas/*.json`, `schemas/fixtures/*.json`
- Modify: `src/ipc/{commands.ts,schemas.ts,types.ts,mock-backend.ts}` and `src/ipc/__tests__/*`

**Interfaces:**
- Consumes: Task 7 application façade and DTO mapping conventions from Increment 3.
- Produces: `refresh_inventory()`, `refresh_project_definition({ profileId })`, updated lifecycle response with `inventoryGeneration` and `inventory`.

- [ ] **Step 1: Write failing contract tests**

```rust
#[test]
fn inventory_dto_contains_snapshot_marker_and_full_container_list() {
    let json = serde_json::to_value(RuntimeInventoryDto::fixture()).unwrap();
    assert_eq!(json["hasSnapshot"], true);
    assert!(json["containers"].is_array());
}

#[test]
fn lifecycle_generation_matches_embedded_inventory() {
    let dto = LifecycleResultDto::fixture();
    assert_eq!(dto.inventory_generation, dto.inventory.generation);
}
```

```ts
it('decodes pre-observation generation zero only with hasSnapshot false', () => {
  expect(inventorySchema.parse(preObservationFixture).hasSnapshot).toBe(false);
});
```

- [ ] **Step 2: Run focused tests and verify failure**

Run: `cargo test -p colui-tauri --test dto_contracts && pnpm test -- src/ipc/__tests__`

Expected: FAIL because new DTOs and commands are absent.

- [ ] **Step 3: Implement explicit DTOs and schemas**

Use camelCase serde attributes, nullable pre-observation fields, `containers` at top level, nested project snapshots, bounded `AppErrorDto`, and exact enum values. Map `inventoryGeneration` and assert it equals embedded `inventory.generation`. Keep compatibility `get_project_status` fallback shape.

- [ ] **Step 4: Register commands and update mock**

Wire shared owners in `AppState`, add commands to `generate_handler!`, implement mock inventory generations/coalescing fixtures and definition responses. All responses pass existing decoder. Lifecycle mock returns embedded inventory and generation without triggering a second mock refresh call.

- [ ] **Step 5: Generate schemas and fixtures**

Run: `bash scripts/generate-schemas.sh`

Expected: committed schemas include inventory/definition/lifecycle fields and deterministic positive/negative fixtures.

- [ ] **Step 6: Run tests and commit**

Run: `cargo test -p colui-tauri --test dto_contracts && pnpm test -- src/ipc/__tests__ && pnpm test:contracts`

```bash
git add src-tauri schemas src/ipc
git commit -m "feat(ipc): expose inventory contracts"
```

## Task 9: Integrate TanStack Query Generation Guard and Polling

**Files:**
- Create: `src/features/runtime/hooks/useInventory.ts`
- Modify: `src/app/query-client.ts`, `src/features/runtime/query-keys.ts`, `src/features/projects/query-keys.ts`, `src/features/projects/hooks/useProjectStatus.ts`, `src/features/projects/hooks/useLifecycleActions.ts`, `src/features/projects/hooks/useProfileMutations.ts`, `src/features/projects/ProjectsView.tsx`
- Test: `src/features/runtime/__tests__/useInventory.test.tsx`, `src/features/projects/__tests__/projects.test.ts`, `src/features/projects/components/__tests__/ActionMenu.test.tsx`

**Interfaces:**
- Consumes: Task 8 IPC types; TanStack Query v5.
- Produces: `inventoryKeys.snapshot()`, `useInventory()`, `inventoryStructuralSharing`, generation-safe lifecycle cache publication.

- [ ] **Step 1: Write failing frontend tests**

```tsx
it('keeps cache reference for equal or lower generation', () => {
  const oldData = publishedInventory(4);
  expect(inventoryStructuralSharing(oldData, publishedInventory(4))).toBe(oldData);
  expect(inventoryStructuralSharing(oldData, publishedInventory(3))).toBe(oldData);
});

it('accepts first snapshot despite pre-observation generation zero', () => {
  expect(inventoryStructuralSharing(preObservation(), publishedInventory(1))).toEqual(publishedInventory(1));
});

it('lifecycle success does not invoke refresh_inventory again', async () => {
  await applyMutation.mutateAsync(profileId);
  expect(mockBackend.calls('refresh_inventory')).toHaveLength(0);
  expect(queryClient.getQueryData(inventoryKeys.snapshot())?.generation).toBe(1);
});
```

- [ ] **Step 2: Run focused tests and verify failure**

Run: `pnpm test -- src/features/runtime/__tests__/useInventory.test.tsx`

Expected: FAIL because inventory query and structural sharing function do not exist.

- [ ] **Step 3: Implement exact TanStack Query integration**

Set `structuralSharing` on `useInventory` query options, not in `queryFn` or a post-write interceptor. Return old object for lower/equal published generations; accept first `hasSnapshot` response. Use `refetchInterval: 3000` only while visible, pause hidden, retain previous data on failure, and preserve existing retry policy.

- [ ] **Step 4: Update lifecycle mutation**

On success, call `queryClient.setQueryData(inventoryKeys.snapshot(), result.inventory)` with the same structural sharing function. Do not invalidate/refetch/call `refreshInventory`. Project status components read inventory-derived projections; no per-profile runtime polling remains.

- [ ] **Step 5: Add definition query invalidation**

Profile update calls backend cache invalidation through its command/use-case boundary and invalidates `projectKeys.definition(profileId)`. Profile removal evicts definition and inventory-derived query data. Foreground details loads through the definition command.

- [ ] **Step 6: Run tests and commit**

Run: `pnpm test -- src/features/runtime src/features/projects && pnpm typecheck && pnpm lint`

```bash
git add src/app src/features src/ipc
git commit -m "feat(ui): poll coordinated inventory"
```

## Task 10: Add Complete Hermetic Coverage and Verification Gates

**Files:**
- Modify: `crates/colui-adapters/tests/{inventory.rs,definitions.rs,operations.rs}`, `crates/colui-app/tests/{lifecycle.rs,status.rs}`
- Modify: `src/ipc/__tests__/*`, `src/features/**/__tests__/*`
- Modify: `scripts/check-boundaries.sh`
- Create: `scripts/verify-increment-4.sh`
- Modify: `package.json` only if script registration is required

**Interfaces:**
- Consumes: Tasks 1-9.
- Produces: hermetic acceptance gates and existing-fixture real-Docker label smoke.

- [ ] **Step 1: Add missing race and retention tests**

Cover all normative races: concurrent refresh, lifecycle during background refresh, lifecycle during definition load, reconnect/disconnect during list, old response after newer generation, profile revision during config load, outage after success, duplicate lifecycle, lifecycle observation failure. Assert Docker call counts, no config on fast path, no subscriber notification for equal generation, and guard release.

- [ ] **Step 2: Add fake protocols**

Fake Docker exposes queued observations, list count, in-flight barrier, session identity, and errors. Fake Compose records argv/cwd/env, distinguishes lifecycle/config, supports blocking and deterministic JSON. Tests use no Docker daemon or network.

- [ ] **Step 3: Add boundary gate**

`check-boundaries.sh` rejects second refresh API, second generation counter, second lock map, fast-path Compose config, direct frontend Tauri imports, path-bearing lifecycle payloads, name-based React keys, hidden registry writes, and forbidden dependency edges.

- [ ] **Step 4: Extend existing real-Docker smoke**

Reuse Increment 2 fixture. Assert at least one live Compose container has official labels and that refresh produces the expected `ContainerObservation` project snapshot. Assert changing profile definition produces a different definition revision. Do not use this test for coalescing or call-count guarantees.

- [ ] **Step 5: Run full verification**

Run:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo test -p colui-tauri --test dto_contracts
pnpm lint
pnpm typecheck
pnpm test
pnpm test:contracts
pnpm build
bash scripts/check-boundaries.sh
bash scripts/verify-increment-4.sh
git diff --check
```

Expected: all ordinary commands exit zero without Docker/network; real-Docker smoke remains separately gated.

- [ ] **Step 6: Commit verification gates**

```bash
git add crates src-tauri src scripts package.json Cargo.toml Cargo.lock schemas
git commit -m "test: verify increment four inventory"
```

## Acceptance Checklist

- [ ] One `InventoryCoordinator` owns fast refresh/coalescing/publication/backoff.
- [ ] `RuntimeInventory` is immutable and includes `generation`, `hasSnapshot`, timestamps, session/fingerprint, full containers, projects, standalone data, freshness, and error.
- [ ] Generation `0` exists only before first success; first successful generation is `1`; reconnect does not reset it.
- [ ] Backend and frontend reject lower generations; equal generation preserves object reference and sends no notification.
- [ ] One Docker list-all call per fast refresh; no inspect/config call in fast path.
- [ ] Docker adapter owns canonical `ContainerObservation` extraction and official label parsing.
- [ ] Project and standalone projections derive from one observation list without registry writes.
- [ ] Last successful snapshot survives outage with stale/unavailable state, timestamp, and bounded backoff.
- [ ] One `DefinitionCache` owns `(ProfileId, Revision)` entries, 60-second TTL, invalidation, canonical hash, and stale retention.
- [ ] Definition loading uses `docker compose config --format json` and never runs on three-second polling.
- [ ] One `OperationLockManager` owns lifecycle and definition leases; duplicate lifecycle returns `operation_conflict`.
- [ ] Lifecycle lock remains held through successful inventory refresh; failed Compose does not refresh.
- [ ] Successful lifecycle returns embedded inventory plus matching `inventoryGeneration`; frontend performs no follow-up refresh.
- [ ] Compatibility `get_project_status` returns explicit unavailable fallback before first inventory snapshot.
- [ ] TanStack Query has exactly one visible-window inventory poller and uses `structuralSharing` for generation guard.
- [ ] Runtime and definition errors remain separate; invalid profiles remain visible.
- [ ] Hermetic fakes cover coalescing, generations, failures/backoff, no-config fast poll, TTL, revision invalidation, and operation conflicts.
- [ ] Existing real-Docker fixture verifies official label parsing and definition revision change only.
- [ ] No Increment 5 or generic infrastructure scope appears.
