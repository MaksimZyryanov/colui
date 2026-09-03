# CoLUI Increment 4: InventoryCoordinator and Definitions

**Status:** design approved in conversation; written specification pending user review
**Date:** 2026-09-03
**Depends on:** `docs/superpowers/specs/2026-09-01-colui-2-design.md`, `docs/superpowers/specs/2026-09-02-colui-increment-2-runtime-design.md`, `docs/superpowers/specs/2026-09-02-colui-increment-3-ipc-ui-design.md`

## 1. Goal and scope

Increment 4 replaces Increment 3's single-shot runtime status path with one coordinated backend inventory and adds the separate slow Compose definition path. It also adds generation-aware transport/cache handling, per-profile lifecycle operation locks, and TanStack Query polling over the existing typed IPC foundation.

The target is a local macOS Docker workflow. Runtime inventory is read-only observation. Profile identity, revisions, and paths remain owned by `ProfileRegistry`; desired Compose configuration remains represented by `ProjectDefinition`; Docker runtime remains represented by immutable `RuntimeInventory` snapshots. No query or refresh operation writes durable profiles.

This increment includes:

- one backend `InventoryCoordinator` owner for fast refresh;
- at most one in-flight inventory refresh, with concurrent callers sharing its result;
- one Docker API `list all containers` call per fast refresh;
- normalization into `ContainerInstance`, official Compose-label association, project runtime snapshots, and standalone-container projections;
- monotonic inventory generations and backend/frontend stale-generation rejection;
- retention of the last successful snapshot after refresh failure, freshness/error state, timestamps, and bounded automatic backoff;
- a separate `DefinitionCache` for `docker compose config`;
- profile revision and 60-second definition expiry invalidation;
- definition refresh priority yielding to lifecycle operations;
- one `OperationLockManager` owner for per-profile lifecycle conflicts;
- exactly one coordinator refresh after each successful lifecycle operation;
- generation-bearing lifecycle results;
- coordinator-backed TanStack Query polling that pauses for hidden windows and uses existing retry defaults.

This increment does not include Discovery UI, auto-registration, Diagnostics, standalone-container lifecycle/log/port UI, Docker Events, remote or multi-context Docker, live-follow logs, Images, a new lifecycle implementation, SQLite, generic resource abstractions, or visual redesign beyond freshness/error presentation.

## 2. Increment 3 boundary and migration

### 2.1 Facts from the actual worktree

Increment 3 is present on `main` through commit `e1b0677` and prior commits. The actual implementation has these relevant surfaces:

- `crates/colui-domain/src/definition.rs` already defines `ContainerInstance`, `ProjectDefinition`, `ServiceDefinition`, `DefinitionState`, `RuntimePresence`, `RuntimeActivity`, `Issue`, and `Timestamp`.
- `crates/colui-domain/src/runtime.rs` already defines `RuntimeSessionId`, `DaemonFingerprint`, and `RuntimeSessionState`.
- `crates/colui-app/src/runtime.rs` defines `DockerApi`, `ComposeRunner`, runtime session ports, and `RuntimeFuture`.
- `crates/colui-app/src/status.rs` defines `ProjectStatusReader`, `ProjectStatus`, and the single-profile status use case.
- `crates/colui-app/src/lifecycle.rs` defines the four ID-based lifecycle use cases and `LifecycleRuntime`; `LifecycleResult` currently has only `profile_id` and `success`.
- `crates/colui-adapters/src/runtime/gateway.rs` owns runtime session state, the existing global Compose semaphore, and the current single-shot `ProjectStatusReader` implementation. Its internal `Snapshot::generation` is a runtime-session mutation counter.
- `crates/colui-adapters/src/runtime/docker_api.rs` already performs `list_containers` with `all: true`, but currently drops most labels after mapping. Increment 4 extends its raw mapping contract so inventory association does not require per-container inspect calls.
- `src-tauri/src/dto/status.rs` defines the current `ProjectStatusDto` and `LifecycleResultDto` without inventory generation.
- `src-tauri/src/lib.rs` wires one `RuntimeGateway` into `AppState`; it currently exposes `ProjectStatusReader` directly.
- `src/ipc/commands.ts`, `src/ipc/schemas.ts`, and `src/ipc/types.ts` are the only frontend transport boundary. `useProjectStatus` currently calls `get_project_status` once per profile.
- `src/app/query-client.ts` already restricts retries to retryable `AppErrorException`, disables focus refetch and polling, and disables mutation retries.
- `src/features/projects/hooks/useLifecycleActions.ts` currently invalidates `projectKeys.status(profileId)` after lifecycle success.

The existing runtime session generation is not reused as inventory generation. It protects a `RuntimeGateway` client from session changes; inventory generation describes ordering of inventory observations. `RuntimeInventory` therefore carries both independent concepts: `generation` and `runtime_session_id`.

### 2.2 Migration path

Reuse:

- the existing `RuntimeGateway`, `DockerApi`, `ComposeRunner`, endpoint/fingerprint gate, global Compose semaphore, and ID-based lifecycle command signatures;
- existing profile reader/store and revision semantics;
- the existing Zod decoder, typed errors, QueryClient retry defaults, and immutable profile query keys.

Replace:

- `RuntimeGateway`'s `ProjectStatusReader` implementation as the source for project runtime projections. It must no longer list and inspect containers once per profile.
- `GetProjectStatus` wiring with an application façade that composes profile data, `InventoryReader`, and `DefinitionReader` projections.
- lifecycle execution wiring so successful lifecycle operations call the shared coordinator exactly once before returning.
- `LifecycleResultDto` and its Zod schema to include `inventoryGeneration`.
- frontend per-profile status polling/queries with one inventory query, while retaining profile list/detail queries.
- lifecycle status invalidation with generation-aware inventory acceptance and one inventory refetch.

Do not add a second refresh API, generation counter, definition cache, or lifecycle lock owner. The existing runtime session counter remains private to `RuntimeGateway` and is not exposed as inventory freshness.

## 3. Ownership and architecture

The selected architecture is independent peer modules, not a coordinator god object and not a unified gateway façade.

```text
ProfileRegistry ───────────────┐
                               ├─ Project projection use case ── typed IPC
InventoryCoordinator ──────────┤
DefinitionCache ───────────────┘

Lifecycle use case ── OperationLockManager
                  ├─ RuntimeGateway / ComposeRunner
                  └─ InventoryCoordinator.refresh()

RuntimeGateway owns RuntimeSession, verified Docker client, global Compose gate,
and Compose execution. It does not depend on InventoryCoordinator or DefinitionCache.
```

### 3.1 InventoryCoordinator

`InventoryCoordinator` is the sole owner of fast runtime inventory refresh, current immutable snapshot, inventory generation sequencing, publication, refresh coalescing, and automatic backoff state. It depends on the application `DockerApi` port and a `Clock` port. It may read the current verified runtime session context through an application port, but it does not own or mutate the session.

### 3.2 DefinitionCache

`DefinitionCache` is the sole owner of slow `docker compose config` results and definition cache entries. It depends on a profile reader or receives a backend-resolved `ProjectProfile`, a Compose definition runner port, a `Clock`, and the shared operation lock read interface. It does not depend on `InventoryCoordinator` and does not share its generation counter.

### 3.3 OperationLockManager

`OperationLockManager` is the sole owner of lifecycle operation locks. It is application policy, not Docker transport. Lifecycle use cases acquire it before invoking `RuntimeGateway`; `DefinitionCache` reads the same owner to yield background definition loads. `RuntimeGateway` continues to own the global Compose semaphore, but does not own per-profile operation locks.

The acquire operation is atomic. A separate `contains` followed by `insert` is forbidden because it permits duplicate operations under a race.

### 3.4 Lifecycle orchestration

The lifecycle use case performs this sequence:

1. Resolve current profile by immutable `ProfileId`.
2. Acquire the shared per-profile operation lock.
3. Invoke `RuntimeGateway` through the existing `LifecycleRuntime` port.
4. If Compose succeeds, call `InventoryCoordinator.refresh()` exactly once while the operation lock remains held. The refresh returns either a fresh published snapshot or a retained snapshot carrying the prior generation plus stale/unavailable state.
5. Return `LifecycleResult { profile_id, success, inventory_generation }`.
6. Release the operation guard on every success or error path.

`RuntimeGateway` does not call the coordinator. A failed Compose operation does not trigger an inventory refresh. If the successful operation's inventory refresh fails, the lifecycle result still reports the successful Compose operation and carries the last published generation, while the inventory projection exposes stale/unavailable state and the refresh error. It must not invent a generation or hide that observation failed. A coalesced refresh still counts as the one refresh call for each successful lifecycle invocation, while only one Docker listing is in flight.

## 4. Domain and application contracts

### 4.1 Runtime inventory values

Add or export these runtime-free domain values in `crates/colui-domain/src/definition.rs` or a focused inventory module:

```rust
pub struct RuntimeInventory {
    pub generation: u64,
    pub observed_at: Timestamp,
    pub runtime_session_id: RuntimeSessionId,
    pub daemon_fingerprint: DaemonFingerprint,
    pub freshness: InventoryFreshness,
    pub last_successful_observed_at: Option<Timestamp>,
    pub containers: Vec<ContainerInstance>,
    pub project_snapshots: Vec<ProjectRuntimeSnapshot>,
    pub standalone_containers: Vec<ContainerInstance>,
    pub error: Option<AppError>,
}

pub enum InventoryFreshness { Fresh, Stale, Unavailable }

pub struct ProjectRuntimeSnapshot {
    pub compose_project_name: String,
    pub working_directory: Option<String>,
    pub config_files: Vec<String>,
    pub containers: Vec<ContainerInstance>,
}
```

`RuntimeInventory` is immutable after construction. Every collection is owned by the snapshot; callers receive clones or read-only views and cannot mutate coordinator state. `containers` is the normalized result of the one list call. `project_snapshots` and `standalone_containers` are derived from it. `error` is a bounded typed error describing current unavailability; it does not erase successful data.

Compose association uses official labels from the Docker list response:

- `com.docker.compose.project` identifies the Compose namespace;
- `com.docker.compose.service` identifies the service;
- `com.docker.compose.working_dir` supplies optional working-directory metadata;
- `com.docker.compose.config-files` supplies optional config-file metadata.

A container with a valid Compose project label belongs to that project snapshot. A container without that label belongs to `standalone_containers`. Missing optional labels do not cause an inspect call or profile mutation. Association is by Compose namespace for projection only; it never merges profiles or changes registry data.

The existing `ContainerInstance` must be extended internally with the labels needed during normalization, or the Docker adapter must return a separate raw observation that the coordinator converts into `ContainerInstance`. Labels must not be discarded before association. The public normalized `ContainerInstance` remains free of an unapproved generic metadata bag unless an exact later consumer requires it.

### 4.2 Application ports

Add focused ports to `crates/colui-app/src/inventory.rs`, `definitions.rs`, and `operations.rs`:

```rust
pub trait InventoryReader: Send + Sync {
    fn current_inventory(&self) -> InventoryFuture<'_, RuntimeInventory>;
}

pub trait InventoryRefresher: InventoryReader {
    fn refresh(&self) -> InventoryFuture<'_, RuntimeInventory>;
}

pub trait DefinitionReader: Send + Sync {
    fn definition(&self, profile: ProjectProfile) -> DefinitionFuture<'_, ProjectDefinition>;
}

pub trait DefinitionRefresher: DefinitionReader {
    fn refresh_definition(&self, profile: ProjectProfile) -> DefinitionFuture<'_, ProjectDefinition>;
    fn invalidate(&self, profile_id: ProfileId);
}

pub trait OperationLockReader: Send + Sync {
    fn is_busy(&self, profile_id: &ProfileId) -> bool;
}

pub trait OperationLockManager: OperationLockReader {
    fn try_acquire(
        &self,
        profile_id: ProfileId,
        kind: OperationKind,
    ) -> Result<OperationGuard, AppError>;
}
```

Exact future aliases follow existing `RuntimeFuture`/`LifecycleFuture` style and remain free of Bollard, Tauri, filesystem, and process types. `OperationGuard` is the RAII release handle; it must be `Send` when required by the async lifecycle future and release exactly once.

`InventoryReader::current_inventory` returns a retained snapshot when one exists. Before the first successful observation, it returns an unavailable `RuntimeInventory` projection with empty data; it never fabricates containers.

### 4.3 Definition cache contract

```rust
pub struct CachedDefinition {
    pub profile_revision: Revision,
    pub definition_revision: DefinitionRevision,
    pub loaded_at: Timestamp,
    pub state: DefinitionState,
    pub services: Vec<ServiceDefinition>,
    pub issues: Vec<Issue>,
}
```

Cache key is `ProfileId`; `profile_revision` is part of validity. `definition_revision` is a deterministic SHA-256 digest of normalized successful `docker compose config` output, not a profile identity or runtime generation. Invalid definitions are cached with `DefinitionState::Invalid` and issues. A failed refresh retains the previous definition as `Stale` with its previous services and issues, while the load error remains a definition error in the returned projection.

## 5. Inventory refresh state machine

### 5.1 States and transitions

```text
NoSnapshot / Idle
  ├─ explicit refresh ───────────────> Refreshing
  ├─ automatic refresh allowed ─────> Refreshing
  └─ automatic refresh in backoff ──> Backoff

Refreshing
  ├─ list + normalize success ───────> Publish(generation) -> Idle
  ├─ failure with prior snapshot ────> Retained(Stale) -> Backoff
  ├─ failure without snapshot ───────> Unavailable -> Backoff
  └─ concurrent caller ─────────────> joins same result

Backoff
  ├─ timer expires ──────────────────> Refreshing
  └─ explicit refresh ───────────────> Refreshing
```

Only the coordinator can transition or publish these states. Automatic backoff suppresses only automatic polling; an explicit user refresh bypasses the delay. Explicit refresh coalesces with an already-running refresh rather than starting a second listing.

### 5.2 Coalescing

The coordinator stores one in-flight shared future/result under a short async mutex. The first caller installs the refresh state and performs the Docker call. Later callers clone a waiter and await the same result. There is never more than one `DockerApi::list_containers` call in flight for one coordinator.

The shared result contains either the published immutable snapshot or the same typed failure/retained-state outcome. Concurrent callers do not each publish or notify. A successful refresh publishes once and notifies subscribers once.

### 5.3 Generation and publication

Inventory generation is a coordinator-owned monotonic `u64` that starts at zero and advances for each accepted successful observation. It must not reset on reconnect or disconnect. A successful refresh allocates the next generation only after the observation is associated with a valid current runtime session. Publication accepts only a generation greater than the current published generation; lower generations are discarded without replacing data or notifying subscribers. Equal generations are idempotent no-ops.

Each list request captures `runtime_session_id` and daemon fingerprint before awaiting Docker. If session identity changes while the request is in flight, the response is obsolete and is not published as current. The last successful snapshot remains available and is marked stale/unavailable through the derived failure state. A reconnect does not clear the inventory counter or durable profiles.

The coordinator and frontend both enforce ordering:

```text
backend publication: next.generation > current.generation
frontend acceptance: next.generation > current.generation
equal generation: return without cache replacement or subscriber notification
```

The frontend guard must preserve the existing cache object/reference for equal generation, because `setQueryData` with an equivalent value can still notify observers.

### 5.4 Backoff and failure retention

Automatic failures use bounded exponential delays: 1 second, 2 seconds, 4 seconds, 8 seconds, then 10 seconds maximum. A successful refresh resets the failure count and delay. Backoff state is in-memory and session-scoped; it is never persisted. Runtime errors are classified as `runtime_unavailable` or the existing typed runtime code. The previous `containers`, project snapshots, observed timestamp, and generation remain visible after failure; freshness becomes `stale` and the current failure is exposed separately. Without a prior successful snapshot, freshness is `unavailable` and no runtime data is invented.

## 6. Definitions path

`DefinitionCache` invokes the existing `ComposeRunner` with backend-resolved profile paths and the shared verified runtime environment. It uses `docker compose config` and parses normalized output into `ServiceDefinition` values. It never runs as part of `InventoryCoordinator::refresh()` and never runs on a three-second runtime poll.

### 6.1 Cache validity

An entry is a valid cache hit only when:

- key `(ProfileId, profile_revision)` matches current `ProjectProfile.id` and `ProjectProfile.revision`;
- current time is less than `loaded_at + 60 seconds`;
- no explicit invalidation has occurred after the entry was loaded.

An entry beyond 60 seconds is returned as `Stale` only when a fresh load cannot proceed or fails; normal foreground access attempts a new load. A profile revision change invalidates the old entry immediately. A load completing after a revision change is discarded and not written to cache.

### 6.2 Invalidation triggers

The sole cache exposes these triggers:

- successful `update_profile` invalidates that `ProfileId` after the registry mutation;
- foreground project details requests a fresh definition when the entry is missing or stale;
- explicit definition refresh always attempts a fresh load;
- a stale entry is refreshed on its next definition access after 60 seconds;
- profile removal evicts the entry.

Profile updates do not call `compose config` synchronously inside registry mutation. They invalidate the cache; the next eligible definition access performs the load.

### 6.3 Lifecycle priority

Before starting background `compose config`, the cache checks `OperationLockManager::is_busy`. If busy, it yields without spawning Compose. It returns the retained cached definition marked stale when available, or `Unchecked` when no entry exists. A lifecycle operation acquires its lock before Compose execution and holds it through the successful inventory refresh, preventing a definition load from overtaking either step.

Definition errors and runtime errors remain separate. An invalid profile remains in profile lists and can have an invalid definition while its last runtime snapshot remains present.

## 7. Lifecycle locks and refresh integration

`OperationLockManager` stores at most one active operation per `ProfileId`, including operation kind and start timestamp for projection. The atomic `try_acquire` returns `operation_conflict` for a duplicate and does not call Docker, Compose, or the inventory coordinator. Different profiles may hold different operation locks while waiting on the existing global Compose semaphore of one.

The lock guard releases on Compose failure, timeout, inventory refresh failure, cancellation, and success. No code path manually releases a lock in addition to the guard. The lock owner is shared by lifecycle operations and `DefinitionCache`; no second busy map may be introduced in `RuntimeGateway`, the coordinator, or React.

Successful lifecycle flow:

```text
profileId
  -> ProfileReader current profile
  -> OperationLockManager.try_acquire
  -> RuntimeGateway.run_profile
  -> InventoryCoordinator.refresh exactly once
  -> LifecycleResult(profileId, success, inventoryGeneration)
  -> guard release
```

`LifecycleResult` changes in `crates/colui-app/src/lifecycle.rs` to include `inventory_generation: u64`. `src-tauri/src/dto/status.rs` maps it to `inventoryGeneration`. Existing command inputs remain `{ profileId }` only. Stop, Tear down, Apply, and Restart semantics do not change.

## 8. IPC DTO and command surface

Add commands through existing `src-tauri/src/commands` and `AppState` wiring:

| Command | Input | Result | Durable write |
|---|---|---|---|
| `refresh_inventory` | none | `RuntimeInventoryDto` | no |
| `get_project_details` | `{ profileId }` | profile + definition + runtime projection | no |
| `refresh_project_definition` | `{ profileId }` | `ProjectDefinitionDto` | no |

Retain existing `get_project_status` during this increment so Increment 3 clients remain decodable, but change its implementation to read the shared inventory and definition owners. It must not perform its own Docker listing or inspect loop. New frontend code uses the coordinator-backed inventory command; the compatibility command is removed only after all current callers migrate, without introducing another refresh implementation.

### 8.1 Runtime inventory DTO

`src-tauri/src/dto/inventory.rs` defines explicit `Serialize`, `Deserialize`, and `JsonSchema` DTOs using camelCase:

```rust
pub struct RuntimeInventoryDto {
    pub generation: u64,
    pub observed_at: String,
    pub runtime_session_id: String,
    pub daemon_fingerprint: DaemonFingerprintDto,
    pub freshness: InventoryFreshnessDto,
    pub last_successful_observed_at: Option<String>,
    pub projects: Vec<ProjectRuntimeSnapshotDto>,
    pub standalone_containers: Vec<ContainerInstanceDto>,
    pub error: Option<AppErrorDto>,
}
```

`RuntimeInventoryDto` includes `containers` as the normalized full list from the single Docker listing. It is not a second independently fetched dataset. Project snapshots use `composeProjectName`, optional `workingDirectory`, ordered `configFiles`, and normalized `containers`.

All timestamp fields are RFC 3339 strings. Runtime freshness is `fresh`, `stale`, or `unavailable`. Runtime inventory errors do not become definition errors and do not turn a retained snapshot into an empty successful response.

### 8.2 Definition and lifecycle DTOs

Add `ProjectDefinitionDto` and extend `LifecycleResultDto`:

```rust
pub struct LifecycleResultDto {
    pub profile_id: String,
    pub success: bool,
    pub inventory_generation: u64,
}

pub struct ProjectDefinitionDto {
    pub profile_id: String,
    pub definition_revision: Option<String>,
    pub loaded_at: Option<String>,
    pub state: DefinitionStateDto,
    pub services: Vec<ServiceDefinitionDto>,
    pub issues: Vec<IssueDto>,
}
```

A failed lifecycle command returns `AppErrorDto`, not a fake generation. A successful lifecycle always carries the generation returned by its one coordinator refresh, including the last published generation when the post-operation observation is stale/unavailable. Request DTOs remain profile-ID based; frontend never supplies paths, Compose names, or environment files for lifecycle or definition commands.

## 9. TanStack Query integration

Add `inventoryKeys.snapshot()` and `projectKeys.definition(profileId)` using immutable IDs. Add `src/ipc/commands.ts` wrappers and Zod schemas/types for every new request and response. All Tauri and mock responses continue through the existing decoder.

`useInventory` owns one query for the full runtime inventory. Its policy is:

- `queryFn` calls `refreshInventory`;
- `refetchInterval` is 3000 ms while `document.visibilityState` is visible;
- polling pauses while hidden and resumes when visible;
- automatic retries use the existing QueryClient retry function and are capped by coordinator/runtime backoff rather than creating a tight loop;
- previous successful inventory data remains visible during errors;
- no per-profile runtime polling remains.

The query function passes responses through `acceptInventory(next)`. The guard reads the current `inventoryKeys.snapshot()` cache and returns the current value for lower or equal generations. Equal generations must not call `setQueryData`; lower generations must not call it either. Only strictly newer generations replace data and notify subscribers.

Lifecycle mutation success receives `inventoryGeneration`, accepts the response through the same generation guard, and invalidates/refetches the single inventory query once. It must not call a second explicit refresh after the backend already performed its required refresh. The implementation may use `invalidateQueries` to schedule the query refetch, but the generation guard remains authoritative when transport responses reorder. Profile update invalidates definition cache/query for that ID; removal evicts definition and inventory-derived project projections without writing runtime state.

Foreground details use `getProjectDetails` or the existing profile detail plus definition command, with the definition cache policy enforced by backend. React is never authority for runtime or definition state and never performs optimistic projection mutation.

## 10. Race semantics

The following behavior is normative:

| Race | Required result |
|---|---|
| Two simultaneous `refresh_inventory` calls | One Docker list call; both callers await the shared result and receive the same published generation. |
| Lifecycle success during background refresh | Lifecycle calls the coordinator once and joins the current refresh if one is in flight; no second concurrent list call. Returned generation is the shared published generation. |
| Lifecycle action during definition refresh | Lifecycle acquires the operation lock; background definition refresh observes busy and yields without spawning Compose. |
| Reconnect/disconnect during inventory request | Session identity check rejects obsolete response; prior snapshot remains visible with stale/unavailable state. |
| Old response after newer published generation | Backend and frontend discard lower generation without replacement or notification. |
| Equal generation response | Idempotent no-op; cache reference and subscriber notification remain unchanged. |
| Profile revision change during definition load | Result is discarded when revision no longer matches; no stale entry is written. |
| Runtime outage after successful snapshot | Profiles and last successful containers remain visible; freshness/error/timestamp expose outage. |
| Duplicate lifecycle action for one profile | Atomic lock returns `operation_conflict`; no Compose or inventory call. |
| Successful lifecycle inventory refresh failure | Last successful inventory is retained and marked stale; lifecycle reports Compose success with retained generation, while inventory exposes typed refresh/runtime error. |

## 11. Testing strategy

### 11.1 Hermetic application tests

Add fake ports under existing test modules or focused test helpers. Tests must not require Docker, filesystem profiles beyond temporary registry fixtures, or network access.

Inventory tests cover:

- one listing for concurrent refresh callers;
- shared result and one publication notification;
- immutable snapshots;
- monotonic generations across refreshes and reconnects;
- lower-generation rejection and equal-generation no-op;
- retained last success after failure;
- unavailable state before first success;
- 1/2/4/8/10-second automatic backoff;
- explicit refresh bypassing automatic backoff;
- old session response rejection;
- one-list normalization, official label association, and standalone derivation.

Definition tests cover:

- no `compose config` from fast inventory refresh;
- cache hit for matching profile revision within 60 seconds;
- stale expiry;
- explicit invalidation after profile update;
- foreground refresh behavior;
- invalid definition retention and visible issues;
- profile revision invalidation during an in-flight load;
- lifecycle busy yielding without Compose invocation.

Operation tests cover:

- atomic per-profile conflict;
- lock release after success, failure, timeout, and cancellation;
- different profiles sharing the global Compose gate;
- one coordinator refresh after successful lifecycle;
- no refresh after failed lifecycle;
- no definition load overtaking a lifecycle operation.

### 11.2 Frontend tests

Add Vitest tests for:

- inventory schema and command decoder contracts;
- lower-generation discard;
- equal-generation no cache replacement and no subscriber notification;
- lifecycle generation handling without redundant refresh;
- visible-window polling and hidden-window pause;
- retained snapshot during runtime failure;
- bounded retry/backoff behavior;
- separate runtime and definition errors;
- no direct Tauri imports outside `src/ipc/dispatch.ts`;
- immutable ID query keys and lifecycle payloads containing only `profileId`.

### 11.3 Fake Docker API protocol

The fake `DockerApi` must expose deterministic queued observations, call count, in-flight barrier, session identity, and injected errors. Each queued list response contains raw normalized container candidates and official labels. Tests assert exactly one invocation and inspect the produced `RuntimeInventory`, not implementation-private collection state.

### 11.4 Fake Compose runner expectations

The fake `ComposeRunner` records executable, argv, working directory, selected environment, operation kind, and invocation count. It supports queued `config` success with deterministic normalized output, invalid config, failure, blocking load, and lifecycle success/failure. It must distinguish lifecycle commands from `compose config` so tests prove definition refresh yields and fast polling never invokes config.

### 11.5 Real Docker scope

No new fixture is required for core Increment 4 acceptance because Increment 2's gated real-Docker contract already verifies the shared RuntimeGateway, daemon fingerprints, Compose CLI, and lifecycle semantics. The existing feature-gated smoke is extended, using its existing fixture, to verify one list call per explicit refresh, official label normalization, and definition revision change after profile update. Hermetic tests remain mandatory and are sufficient for coalescing, generations, failure retention/backoff, cache TTL, revision invalidation, and operation conflicts.

## 12. Verification commands

The implementation plan must make these commands pass without Docker for ordinary workspace checks:

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
git diff --check
```

The existing feature-gated real-Docker command remains separate:

```bash
cargo test -p colui-adapters --features docker-tests
```

Add deterministic Increment 4 checks to a new `scripts/verify-increment-4.sh`, invoked after the existing Increment 3 verification. The gate must reject a second inventory refresh API, direct `compose config` use from fast polling, a second generation counter, name-based React keys, path-bearing lifecycle payloads, hidden registry writes, and a second operation lock map.

## 13. Acceptance checklist

- `InventoryCoordinator` is the only fast inventory refresh owner.
- Concurrent refresh callers share one in-flight result and one Docker list call.
- `RuntimeInventory` is immutable and contains generation, timestamp, session ID, daemon fingerprint, freshness, project snapshots, and standalone containers.
- Inventory generation is independent of RuntimeGateway session generation and remains monotonic across reconnects.
- Backend rejects lower generations; equal generations are no-op publications.
- Frontend rejects lower and equal generations without cache replacement or subscriber notification.
- Fast refresh performs one `list all containers` call and no `compose config`.
- Raw containers normalize into `ContainerInstance` and use official Compose labels.
- Project snapshots and standalone projections are derived without registry writes.
- Last successful inventory remains visible after runtime failure with stale/unavailable state, timestamp, and bounded backoff.
- `DefinitionCache` is independent from `InventoryCoordinator`; entries are keyed by `ProfileId` and valid only for matching profile revision.
- Definition cache invalidates after profile changes, foreground demand, explicit refresh, stale expiry, and profile removal.
- No definition `compose config` runs on each three-second runtime poll.
- `DefinitionState` remains `unchecked`, `valid`, `invalid`, or `stale`; invalid profiles remain visible.
- One `OperationLockManager` owns per-profile locks; duplicates return `operation_conflict`.
- Background definition refresh yields to lifecycle operations.
- Global Compose CLI concurrency remains one.
- Every successful lifecycle operation performs exactly one coordinator refresh before releasing its lock; a failed observation retains the prior generation and reports freshness/error separately from Compose success.
- Lifecycle IPC remains ID-based and returns the resulting inventory generation.
- TanStack Query has one inventory polling owner, pauses while hidden, and preserves retained data on failure.
- Runtime and definition errors remain separate.
- Required hermetic fake Docker and fake Compose tests pass without Docker.
- No Discovery UI, Diagnostics, standalone lifecycle/log/port UI, Docker Events, multi-context support, or visual redesign is introduced.
