# CoLUI Increment 5 Discovery Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete CoLUI 2.0 baseline with safe runtime discovery, diagnostics/recovery, and standalone-container actions, logs, and ports.

**Architecture:** Add narrow application use cases over existing `InventoryCoordinator`, `RuntimeGateway`, `DefinitionCache`, `OperationLockManager`, and `JsonProfileRegistry`. Preserve one inventory poller and owner per existing subsystem; transport only typed projections through Tauri/Schemars/Zod into TanStack Query.

**Tech Stack:** Rust 2021, Tokio, Bollard 0.18, Tauri 2, Schemars, React 18, TypeScript strict, TanStack Query 5, Zod 3, Radix, Vitest, Testing Library, pnpm 9.

**Spec:** `docs/superpowers/specs/2026-09-04-colui-increment-5-discovery-design.md`

## Global Constraints

- `InventoryCoordinator` remains sole fast-refresh, generation, coalescing, retention, publication, and backoff owner.
- Discovery reads immutable inventory observation groups; no second Docker listing, inspect, registry write, filesystem canonicalization, or `compose config` call.
- Queries never write `ProfileRegistry`; auto-registration is a separate explicit use case and defaults disabled.
- Existing profiles are never changed from Docker labels; same-name profiles/candidates never merge automatically.
- `RuntimeGateway` remains sole Bollard client, API/CLI session, fingerprint, and Compose-process owner.
- `OperationLockManager` remains sole operation-lock owner and gains container, mutation, and recovery leases.
- Profile lifecycle IPC remains exactly `{ profileId }`; Stop/TearDown/Remove semantics do not change.
- Lower inventory generations are discarded; equal generations preserve cache identity and emit no publication.
- Definition and runtime errors remain independent; application/profile access remains available without Docker.
- Logs are on-demand only: Docker `tail=4096`, 10-second deadline, 8 MiB transfer ceiling, newest 262,144 payload bytes, no follow/history/persistence.
- Browser open accepts IDs only and is available only for complete recognized web TCP bindings; UDP never becomes URL.
- All IPC uses Rust DTO/Schemars plus Zod semantic parity and typed `AppErrorDto`; direct frontend `invoke` outside `src/ipc/` is forbidden.
- Target remains local macOS Docker. Do not add Docker Events, Images, remote contexts, SQLite, RBAC, plugins, or generic resource abstractions.

---

### Task 1: Preserve Compose observation groups and safe profile association

**Files:**
- Modify: `crates/colui-domain/src/definition.rs`
- Modify: `crates/colui-adapters/src/inventory.rs`
- Modify: `crates/colui-app/src/status.rs`
- Test: `crates/colui-adapters/tests/inventory.rs`
- Test: `crates/colui-app/tests/status.rs`

**Interfaces:**
- Produces: `ComposeObservationGroup`, `RuntimeInventory.compose_observation_groups`, registry-wide association context, `IssueCode::AmbiguousRuntimeAssociation`.
- Preserves: one `RuntimeInventorySource::list_containers()` call and existing project/standalone projections.

- [ ] **Step 1: Add failing inventory grouping tests**

Add fixtures with equal Compose names and different working directories/config lists. Assert two deterministic groups, one list call, and no inspect call. Add status tests asserting full-tuple match, no container projection for duplicate matching profiles, and merged definition plus ambiguity issues.

- [ ] **Step 2: Verify red state**

Run: `cargo test -p colui-adapters --test inventory compose_observation -- --nocapture && cargo test -p colui-app --test status ambiguous_runtime -- --nocapture`

Expected: FAIL because observation groups and ambiguity issue do not exist.

- [ ] **Step 3: Add domain values and coordinator grouping**

Implement:

```rust
pub struct ComposeObservationGroup {
    pub compose_project_name: String,
    pub working_directory: Option<String>,
    pub config_files: Vec<String>,
    pub container_ids: Vec<ContainerId>,
}
```

Build groups in existing `normalize` pass using full tuple key. Sort tuple groups and container IDs deterministically. Keep `project_snapshots` compatibility projection but stop using it as profile association authority.

- [ ] **Step 4: Implement registry-wide profile association**

Change status composition to receive immutable registry snapshot plus inventory, normalize all profile tuples once, and pass cardinality to `project_status_from_inventory`. Supply containers only for one observation tuple and one profile tuple. Append ambiguity issue without replacing definition/runtime issues.

- [ ] **Step 5: Verify green state and regressions**

Run: `cargo test -p colui-domain && cargo test -p colui-app --test status && cargo test -p colui-adapters --test inventory`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/colui-domain/src/definition.rs crates/colui-adapters/src/inventory.rs crates/colui-app/src/status.rs crates/colui-adapters/tests/inventory.rs crates/colui-app/tests/status.rs
git commit -m "feat(inventory): preserve discovery evidence"
```

### Task 2: Add discovery domain classification and candidate identity

**Files:**
- Create: `crates/colui-domain/src/discovery.rs`
- Modify: `crates/colui-domain/src/lib.rs`
- Create: `crates/colui-domain/tests/discovery.rs`

**Interfaces:**
- Consumes: `ComposeObservationGroup`, `ProjectProfile`, `RuntimeSessionId`, `Revision`.
- Produces: `CandidateId`, `DiscoveryCandidate`, `DiscoveryClassification`, `DiscoveryConflictEvidence`, `classify_candidates`.

- [ ] **Step 1: Write table-driven failing tests**

Cover valid new candidate, invalid Compose grammar, absent working directory/files, equal registered tuple, one/several profile conflicts, complete plus incomplete same-name observations, same identity with changed config files, non-UTF-8/relative profile paths, deterministic ordering, and candidate ID session reset.

- [ ] **Step 2: Verify red state**

Run: `cargo test -p colui-domain --test discovery -- --nocapture`

Expected: FAIL because discovery module is absent.

- [ ] **Step 3: Implement exact normalization and ID encoding**

Use `ComposeProjectName::try_from`; lexical absolute path normalization only. Hash `colui-candidate-v1`, 16 UUID bytes, big-endian `u32` UTF-8 lengths, name, presence byte, and working directory using SHA-256. Config files affect metadata hash, not candidate identity.

- [ ] **Step 4: Implement classification precedence**

Implement `IncompleteMetadata > NameConflict > AlreadyRegistered > NewUnambiguous`. Any differing complete/incomplete same-name observation blocks unambiguous registration. Collapse duplicate candidate IDs into one conflict candidate with complete evidence.

- [ ] **Step 5: Verify green state**

Run: `cargo test -p colui-domain --test discovery && cargo test -p colui-domain`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/colui-domain/src/discovery.rs crates/colui-domain/src/lib.rs crates/colui-domain/tests/discovery.rs
git commit -m "feat(domain): classify discovery candidates"
```

### Task 3: Add pure discovery session, ignore, journal, and auto policy

**Files:**
- Create: `crates/colui-app/src/discovery.rs`
- Modify: `crates/colui-app/src/lib.rs`
- Create: `crates/colui-app/tests/discovery.rs`
- Modify: `crates/colui-adapters/src/inventory.rs`

**Interfaces:**
- Produces: `DiscoverySession`, `DiscoveryReader`, `CandidateLease`, `ListDiscoveryCandidates`, `IgnoreCandidate`, `ConfigureAutoRegistration`, 256-entry `SessionJournal`.
- Consumes: `InventoryReader`, registry snapshot reader, successful inventory publication subscription.

- [ ] **Step 1: Add failing purity/session tests**

Use counting fakes to assert list never refreshes Docker or mutates registry. Cover ignore idempotency, reconnect reset, journal retention across reconnect, exact FIFO 256, static-template redaction, disabled auto default, equal-generation no reschedule, disable cancellation, and metadata-hash dedup transitions.

- [ ] **Step 2: Verify red state**

Run: `cargo test -p colui-app --test discovery -- --nocapture`

Expected: FAIL because discovery application module is absent.

- [ ] **Step 3: Implement session and journal**

Use one async state owner keyed by runtime session. Journal messages select closed templates and interpolate only typed opaque IDs/numeric counts. Add `CandidateLease` that pins current discovery evidence while registration executes and delays replacement publication without blocking Docker observation.

- [ ] **Step 4: Implement auto-registration scheduler state**

Use key `(runtime_session_id, candidate_id, metadata_hash)` and states `Pending`, `Succeeded`, `TerminalFailure`, `RetryableFailure { next_generation }`. Subscribe after successful inventory publication; never schedule from query/refresh body.

- [ ] **Step 5: Verify green state**

Run: `cargo test -p colui-app --test discovery && cargo test -p colui-adapters --test inventory`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/colui-app/src/discovery.rs crates/colui-app/src/lib.rs crates/colui-app/tests/discovery.rs crates/colui-adapters/src/inventory.rs
git commit -m "feat(app): add discovery session policy"
```

### Task 4: Implement atomic candidate registration and typed errors

**Files:**
- Modify: `crates/colui-domain/src/error.rs`
- Modify: `crates/colui-app/src/discovery.rs`
- Modify: `crates/colui-app/src/profiles.rs`
- Modify: `crates/colui-app/tests/discovery.rs`
- Modify: `crates/colui-app/tests/profiles.rs`

**Interfaces:**
- Produces: `RegisterCandidate`, `AutoRegisterCandidates`, codes `CandidateStale`, `DiscoveryConflict`, `ProfileAlreadyRegistered`, `RecoveryConflict`, typed error subjects.
- Consumes: `CandidateLease`, `ProfileStore::mutate`, 16 pre-generated UUIDv4 IDs.

- [ ] **Step 1: Add failing race tests**

Cover candidate disappearance/change, reconnect during wait, two instances, manual/auto race, matching profile lost race, name conflict, corrupt/locked/write-failed registry, 16 ID collisions, no update existing, retryable next-generation retry, and manual dedup bypass.

- [ ] **Step 2: Verify red state**

Run: `cargo test -p colui-app --test discovery registration -- --nocapture`

Expected: FAIL because registration use cases/codes are absent.

- [ ] **Step 3: Implement one locked registration path**

Build full `ProfileDraft`: display/Compose name from namespace, observed normalized working directory/files, empty environment files, discovered origin. Hold candidate lease; inside `ProfileStore::mutate`, revalidate session/generation/tuple and locked registry classification immediately before write.

- [ ] **Step 4: Add stable error subjects/retryability**

Generalize domain subject to `{ kind, id }`. Candidate stale/conflict/already-registered are nonretryable; recovery conflict and registry locked/write failed are retryable; registry corrupt remains nonretryable.

- [ ] **Step 5: Verify green state**

Run: `cargo test -p colui-app --test discovery && cargo test -p colui-app --test profiles && cargo test -p colui-domain`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/colui-domain/src/error.rs crates/colui-app/src/discovery.rs crates/colui-app/src/profiles.rs crates/colui-app/tests/discovery.rs crates/colui-app/tests/profiles.rs
git commit -m "feat(app): register discovery candidates"
```

### Task 5: Extend operation locks and registry recovery

**Files:**
- Modify: `crates/colui-app/src/operations.rs`
- Create: `crates/colui-app/src/recovery.rs`
- Modify: `crates/colui-app/src/lib.rs`
- Modify: `crates/colui-adapters/src/operations.rs`
- Modify: `crates/colui-adapters/src/registry/format.rs`
- Modify: `crates/colui-adapters/tests/operations.rs`
- Modify: `crates/colui-adapters/tests/registry.rs`

**Interfaces:**
- Produces: container/mutation/recovery leases on existing `OperationLockManager`, shared/exclusive `registry.recovery.lock`, `RegistrySnapshotIdentity`, `RegistryHealth`, `create_registry_backup`, `restore_registry_backup`.

- [ ] **Step 1: Add failing exclusion and recovery tests**

Cover per-container atomic exclusion, independent containers, shared operation leases, exclusive fail-fast restore, two-process file leases, fixed lock order, corrupt/missing/unreadable canonical, backup hash race, private permissions, fsync/rename, revision+SHA identity, pre-restore artifact cleanup/newest-three bound, and no silent repair.

- [ ] **Step 2: Verify red state**

Run: `cargo test -p colui-adapters --test operations recovery -- --nocapture && cargo test -p colui-adapters --test registry backup -- --nocapture`

Expected: FAIL because recovery contracts are absent.

- [ ] **Step 3: Extend the one lock owner**

Add typed RAII leases to existing manager. Every production profile mutation, definition lease, profile lifecycle, registration, and container action must hold shared recovery file lease. Restore holds process-local exclusive then nonblocking exclusive file lease.

- [ ] **Step 4: Implement health, identity, backup, and restore**

Use `(registry_revision, canonical_content_sha256)` for validated snapshots and `None` for missing/corrupt/unreadable. Create canonical `.bak` only via explicit command. Restore validates/hash-checks backup, preserves current bytes privately, writes revisions above validated authority, atomically replaces, rereads, invalidates caches, and cleans failed artifacts.

- [ ] **Step 5: Verify green state**

Run: `cargo test -p colui-adapters --test operations && cargo test -p colui-adapters --test registry && cargo test -p colui-app`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/colui-app/src/operations.rs crates/colui-app/src/recovery.rs crates/colui-app/src/lib.rs crates/colui-adapters/src/operations.rs crates/colui-adapters/src/registry/format.rs crates/colui-adapters/tests/operations.rs crates/colui-adapters/tests/registry.rs
git commit -m "feat(registry): add explicit recovery"
```

### Task 6: Add diagnostics projection and serialized runtime controls

**Files:**
- Create: `crates/colui-app/src/diagnostics.rs`
- Modify: `crates/colui-app/src/runtime.rs`
- Modify: `crates/colui-app/src/lib.rs`
- Modify: `crates/colui-adapters/src/runtime/gateway.rs`
- Modify: `crates/colui-adapters/src/operations.rs`
- Modify: `crates/colui-adapters/src/registry/import_v1.rs`
- Create: `crates/colui-app/tests/diagnostics.rs`
- Modify: `crates/colui-app/tests/runtime.rs`
- Modify: `crates/colui-adapters/tests/runtime.rs`

**Interfaces:**
- Produces: `DiagnosticsReader`, `DiagnosticsSnapshot`, operation projection reader, retained import result, `ReconnectRuntime`, API-readable mismatch context.

- [ ] **Step 1: Add failing diagnostics purity/concurrency tests**

Assert no Docker/registry write/definition refresh; independently versioned sections; mismatch fingerprints; registry health/import result; active operations; journal; concurrent refresh/lifecycle reads.

- [ ] **Step 2: Add failing runtime transition-table tests**

Cover every connect/disconnect/reconnect row from spec, one transition owner, join/cancel/conflict outcomes, mismatch API inventory, Compose mismatch block, reconnect one refresh, retained inventory on refresh failure.

- [ ] **Step 3: Verify red state**

Run: `cargo test -p colui-app --test diagnostics && cargo test -p colui-adapters --test runtime reconnect -- --nocapture`

Expected: FAIL.

- [ ] **Step 4: Implement diagnostics readers and gateway transition mutex**

Expose API-read context for Ready/ContextMismatch while preserving Ready-only Compose gate. Aggregate snapshots without claiming global atomicity. Retain startup import diagnostics in application state.

- [ ] **Step 5: Verify green state**

Run: `cargo test -p colui-app --test diagnostics && cargo test -p colui-app --test runtime && cargo test -p colui-adapters --test runtime`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/colui-app/src/diagnostics.rs crates/colui-app/src/runtime.rs crates/colui-app/src/lib.rs crates/colui-adapters/src/runtime/gateway.rs crates/colui-adapters/src/operations.rs crates/colui-adapters/src/registry/import_v1.rs crates/colui-app/tests/diagnostics.rs crates/colui-app/tests/runtime.rs crates/colui-adapters/tests/runtime.rs
git commit -m "feat(runtime): expose diagnostics controls"
```

### Task 7: Add standalone container actions and causal refresh

**Files:**
- Create: `crates/colui-app/src/containers.rs`
- Modify: `crates/colui-app/src/lib.rs`
- Modify: `crates/colui-app/src/inventory.rs`
- Modify: `crates/colui-adapters/src/inventory.rs`
- Modify: `crates/colui-adapters/src/runtime/docker_api.rs`
- Modify: `crates/colui-adapters/src/runtime/gateway.rs`
- Create: `crates/colui-app/tests/containers.rs`
- Create: `crates/colui-adapters/tests/containers.rs`

**Interfaces:**
- Produces: `ContainerAction`, `ContainerActionResult`, `ContainerActionObservation`, `ContainerRuntime`, `InventoryRefresher::refresh_after(ObservationOrder)`.

- [ ] **Step 1: Add failing action tests**

Cover expected session mismatch, Compose-container rejection, disappearing standalone container, duplicate lock, independent IDs, exact Bollard start/stop/restart calls, daemon error mapping, post-response session change, and stale refresh retention.

- [ ] **Step 2: Add failing causal refresh tests**

Start list before action completion marker and assert `refresh_after` waits then starts another list. Start list after marker and assert coalescing. Verify one post-action refresh call and no pre-action result.

- [ ] **Step 3: Verify red state**

Run: `cargo test -p colui-app --test containers && cargo test -p colui-adapters --test containers`

Expected: FAIL.

- [ ] **Step 4: Implement Bollard action path and shared observation order**

Use coordinator-owned atomic order counter for list starts and post-action marker. Validate current-session `standalone_containers` membership under action lease before gateway mutation. Never invoke Compose.

- [ ] **Step 5: Verify green state**

Run: `cargo test -p colui-app --test containers && cargo test -p colui-adapters --test containers && cargo test -p colui-adapters --test inventory`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/colui-app/src/containers.rs crates/colui-app/src/lib.rs crates/colui-app/src/inventory.rs crates/colui-adapters/src/inventory.rs crates/colui-adapters/src/runtime/docker_api.rs crates/colui-adapters/src/runtime/gateway.rs crates/colui-app/tests/containers.rs crates/colui-adapters/tests/containers.rs
git commit -m "feat(containers): add standalone actions"
```

### Task 8: Add bounded logs and safe port actions

**Files:**
- Modify: `crates/colui-domain/src/definition.rs`
- Modify: `crates/colui-app/src/containers.rs`
- Modify: `crates/colui-adapters/src/runtime/docker_api.rs`
- Modify: `crates/colui-adapters/src/runtime/gateway.rs`
- Modify: `crates/colui-app/tests/containers.rs`
- Modify: `crates/colui-adapters/tests/containers.rs`

**Interfaces:**
- Produces: `ContainerLogsRequest`, `ContainerLogs`, `PortBindingAction`, pure `binding_display`/`binding_url`, ID-only browser-open use case.

- [ ] **Step 1: Add failing bounded-log tests**

Cover 0, exactly 262,144, and larger payloads; newest bytes; stdout/stderr item order; multiplex framing exclusion; TTY chunks; partial leading/trailing UTF-8; 4096-line request; 10-second deadline; 8 MiB abort; cancellation; old runtime session; disappearing container; no retained text.

- [ ] **Step 2: Add failing port table tests**

Cover complete/partial IPv4/IPv6/wildcard binds, exact copy forms, TCP/UDP/unknown transport, recognized ports, container-port scheme precedence, host-port URL authority, and backend rejection of arbitrary URL/binding index/session.

- [ ] **Step 3: Verify red state**

Run: `cargo test -p colui-app --test containers logs -- --nocapture && cargo test -p colui-app --test containers ports -- --nocapture`

Expected: FAIL.

- [ ] **Step 4: Implement incremental byte ring and port policy**

Never buffer full logs. Make host IP/port optional in domain/DTO path. Browser use case accepts container ID, runtime session ID, and binding index, rereads current standalone inventory, then calls shell opener with backend-built URL.

- [ ] **Step 5: Verify green state**

Run: `cargo test -p colui-app --test containers && cargo test -p colui-adapters --test containers`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/colui-domain/src/definition.rs crates/colui-app/src/containers.rs crates/colui-adapters/src/runtime/docker_api.rs crates/colui-adapters/src/runtime/gateway.rs crates/colui-app/tests/containers.rs crates/colui-adapters/tests/containers.rs
git commit -m "feat(containers): add logs and port actions"
```

### Task 9: Add Tauri DTO, command, schema, and event contracts

**Files:**
- Create: `src-tauri/src/dto/discovery.rs`
- Create: `src-tauri/src/dto/diagnostics.rs`
- Create: `src-tauri/src/dto/containers.rs`
- Modify: `src-tauri/src/dto/{mod,error,inventory,request,runtime}.rs`
- Create: `src-tauri/src/commands/discovery.rs`
- Create: `src-tauri/src/commands/diagnostics.rs`
- Create: `src-tauri/src/commands/containers.rs`
- Modify: `src-tauri/src/commands/{mod,runtime,profiles}.rs`
- Modify: `src-tauri/src/{lib,schema_generation}.rs`
- Modify: `src-tauri/tests/dto_contracts.rs`
- Modify: `schemas/*.json`

**Interfaces:**
- Produces: thin Tauri commands and Schemars schemas for every Increment 5 request/result; `application_state_changed { sequence, scopes }` event.

- [ ] **Step 1: Add failing DTO/command contract tests**

Assert camelCase, required/null fields, typed subject/error codes, candidate conflict evidence, nullable registry identity, reconnect inventory, action observation, exact logs, ID-only browser request, no path-bearing lifecycle regression, and command purity.

- [ ] **Step 2: Verify red state**

Run: `cargo test -p colui-tauri --test dto_contracts -- --nocapture`

Expected: FAIL.

- [ ] **Step 3: Implement thin DTOs/commands and composition**

Commands decode, invoke one use case, map result/error. Wire one instance of every existing owner plus `DiscoverySession`. Emit invalidation event only after mutations/transitions, never from Diagnostics reads.

- [ ] **Step 4: Generate schemas and verify drift**

Run: `bash scripts/generate-schemas.sh && cargo test -p colui-tauri --test dto_contracts && git diff --exit-code schemas/`

Expected: PASS with committed `schemas/*.json` matching generated output.

- [ ] **Step 5: Commit**

```bash
git add src-tauri/src src-tauri/tests/dto_contracts.rs schemas
git commit -m "feat(ipc): expose increment five contracts"
```

### Task 10: Extend frontend IPC and TanStack coherence

**Files:**
- Modify: `src/ipc/{schemas,types,commands,errors,mock-backend}.ts`
- Modify: `src/ipc/__tests__/contracts.test.ts`
- Create: `src/features/discovery/query-keys.ts`
- Create: `src/features/discovery/hooks.ts`
- Create: `src/features/diagnostics/query-keys.ts`
- Create: `src/features/diagnostics/hooks.ts`
- Create: `src/features/containers/hooks.ts`
- Modify: `src/features/runtime/query-keys.ts`
- Modify: `src/features/runtime/hooks/{useInventory,useRuntimeSession}.ts`
- Create: `src/features/runtime/useApplicationStateEvents.ts`
- Create: `src/features/runtime/__tests__/application-state-events.test.tsx`

**Interfaces:**
- Consumes: Task 9 JSON schemas/commands/events.
- Produces: decoded IPC functions, registry-aware discovery keys, session-aware log keys, two-second visible Diagnostics polling, one app-shell event subscriber.

- [ ] **Step 1: Add failing semantic contract tests**

Register every new schema in manifest; add valid fixtures and malformed enum/identity/session/inventory fixtures. Assert protocol mismatch and stable retryability.

- [ ] **Step 2: Add failing cache-coherence tests**

Cover create/update/remove/restore/manual/auto registration invalidation, missed-event polling repair, no read-event loop, reconnect log/discovery clear, container embedded inventory structural sharing, and hidden-window poll pause.

- [ ] **Step 3: Verify red state**

Run: `pnpm test:contracts && pnpm test -- src/features/runtime/__tests__/application-state-events.test.tsx`

Expected: FAIL.

- [ ] **Step 4: Implement schemas, wrappers, keys, hooks, and subscriber**

All mock responses pass same decoder. Discovery key includes session, generation, and nullable tagged registry identity. Logs include session and container ID. Subscriber invalidates scopes only; event payload is never authoritative.

- [ ] **Step 5: Verify green state**

Run: `pnpm test:contracts && pnpm test -- src/features/runtime && pnpm typecheck`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/ipc src/features/discovery src/features/diagnostics src/features/containers src/features/runtime
git commit -m "feat(frontend): add increment five data hooks"
```

### Task 11: Build Projects discovery/containers and Diagnostics UI

**Files:**
- Modify: `src/app/{App,routes}.tsx`
- Create: `src/app/AppShell.tsx`
- Modify: `src/features/projects/ProjectsView.tsx`
- Create: `src/features/discovery/{DiscoverySection,CandidateCard}.tsx`
- Create: `src/features/containers/{OtherContainers,ContainerCard,ContainerLogsDialog,PortBindings}.tsx`
- Create: `src/features/diagnostics/DiagnosticsView.tsx`
- Modify: `src/features/runtime/RuntimeInitializer.tsx`
- Modify: `src/ui/styles.css`
- Create: `src/features/discovery/__tests__/DiscoverySection.test.tsx`
- Create: `src/features/containers/__tests__/OtherContainers.test.tsx`
- Create: `src/features/diagnostics/__tests__/DiagnosticsView.test.tsx`
- Modify: `src/features/projects/__tests__/{ProjectsView,browser-smoke}.test.tsx`

**Interfaces:**
- Consumes: Task 10 hooks only; no direct invoke or authoritative local projections.
- Produces: working Projects/Diagnostics navigation, discovery controls, Other containers actions/logs/ports, recovery controls.

- [ ] **Step 1: Add failing accessible workflow tests**

Cover navigation on desktop/mobile, no dead item, offline Projects/Diagnostics, mismatch explanation/fingerprints, connect/disconnect/reconnect, candidate classes/ignore/manual/auto, standalone pending isolation, disappearing container recovery, log truncation/UTF-8 display, exact copy/open eligibility, restore/backup confirmation with initial Cancel focus, Escape/focus restoration, and status/alert names.

- [ ] **Step 2: Verify red state**

Run: `pnpm test -- src/features/discovery src/features/containers src/features/diagnostics src/features/projects/__tests__/ProjectsView.test.tsx`

Expected: FAIL.

- [ ] **Step 3: Implement shared shell and responsive views**

Lift sole inventory observer to `AppShell`; children read cache without creating pollers. Use semantic nav/main/section headings, Radix dialogs/menus, visible focus, selectable log `pre`, and exact typed recovery controls.

- [ ] **Step 4: Verify green state and browser smoke**

Run: `pnpm test && pnpm typecheck && pnpm build && pnpm test:browser`

Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/app src/features src/ui/styles.css
git commit -m "feat(ui): add discovery diagnostics workflows"
```

### Task 12: Extend real-Docker checks and run final baseline acceptance

**Files:**
- Modify: `crates/colui-adapters/tests/docker.rs`

**Interfaces:**
- Consumes: all prior tasks.
- Produces: focused real-Docker coverage and final 22-criterion evidence.

- [ ] **Step 1: Add focused real-Docker cases**

Reuse fixture setup. Assert official-label discovery tuple, one standalone start/stop/restart path, logs bounded/truncation behavior, and one published TCP binding. Do not duplicate apply-stop-apply-tear-down flow.

- [ ] **Step 2: Run hermetic Rust verification**

Run: `cargo test --workspace`

Expected: PASS, zero failures.

- [ ] **Step 3: Run Rust static checks**

Run: `cargo fmt --all -- --check && cargo clippy --workspace --all-targets -- -D warnings`

Expected: PASS.

- [ ] **Step 4: Run frontend verification**

Run: `pnpm test && pnpm test:contracts && pnpm typecheck && pnpm lint && pnpm build && pnpm test:browser`

Expected: PASS.

- [ ] **Step 5: Run opt-in Docker suite when local daemon is available**

Run: `cargo test -p colui-adapters --features docker-tests --test docker -- --nocapture`

Expected: PASS. If daemon unavailable, record exact skip/blocker; do not weaken hermetic gates.

- [ ] **Step 6: Audit all 22 criteria and quality gates**

Map each criterion in spec section 16 to passing test/file evidence. Search for direct frontend invokes, `Result<_, String>`, Compose-name React/query keys, path-bearing lifecycle DTOs, duplicate owners, hidden registry writes, unsupported platform text, and navigation without views. Fix any violation before commit.

- [ ] **Step 7: Commit**

```bash
git add crates/colui-adapters/tests/docker.rs
git commit -m "test: verify colui two baseline"
```
