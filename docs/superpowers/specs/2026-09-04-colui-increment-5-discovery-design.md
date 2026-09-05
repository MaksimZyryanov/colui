# CoLUI Increment 5: Discovery, Diagnostics, and Recovery

**Status:** design approved in conversation; written specification pending user review
**Date:** 2026-09-04
**Depends on:** `docs/superpowers/specs/2026-09-01-colui-2-design.md`, Increment 2 runtime design, Increment 3 IPC/UI design, and Increment 4 inventory design

## 1. Goal and scope

Increment 5 completes the CoLUI 2.0 baseline with runtime-derived discovery, Diagnostics and explicit registry recovery, and safe standalone-container workflows. It extends the owners established by Increment 4 rather than adding replacement coordinators, gateways, generation counters, definition caches, or profile lifecycle implementations.

This increment includes:

- read-only discovery candidates derived from the current immutable `RuntimeInventory`;
- explicit candidate conflicts, manual registration, optional auto-registration, session-local ignore, and a bounded in-memory journal;
- Diagnostics for runtime session, registry, inventory, definitions, operations, imports, and journal events;
- connect, disconnect, reconnect, and explicit `.bak` restore;
- the existing coordinated `standalone_containers` projection as the Other containers source;
- standalone start, stop, and restart through Bollard;
- bounded on-demand logs and complete port-binding presentation/actions;
- responsive, keyboard-accessible Projects and Diagnostics workflows;
- a final acceptance pass across all 22 master criteria and quality gates.

This increment excludes Docker Events, live-follow or persisted logs, remote or multiple Docker contexts, Images, SQLite, RBAC, plugins, Kubernetes/Podman abstractions, event sourcing, generic resource frameworks, permanent ignored candidates, automatic mutation of existing profiles, and unrelated visual redesign.

## 2. Facts from the current worktree

The design targets the implementation through commit `a1b4a6d` and its final Increment 4 fixes:

- `RuntimeInventory`, `ProjectRuntimeSnapshot`, `ContainerObservation`, and `ComposeContainerMetadata` live in `crates/colui-domain/src/definition.rs`.
- `InventoryCoordinator` in `crates/colui-adapters/src/inventory.rs` owns refresh, immutable snapshots, generations, coalescing, retention, publication, and backoff.
- `DockerApi` in `crates/colui-app/src/runtime.rs` exposes one list path and an on-demand inspect path.
- `DockerApiAdapter` reads the four official `com.docker.compose.*` project labels from the one list response.
- Current project grouping uses only Compose project name. That projection cannot retain evidence for two runtime observations with the same name and different paths.
- `RuntimeGateway` in `crates/colui-adapters/src/runtime/gateway.rs` owns the verified Bollard client, runtime session, API/CLI fingerprint gate, Compose runner, and global Compose permit.
- `OperationLockManager` in `crates/colui-adapters/src/operations.rs` owns per-profile lifecycle and definition leases.
- `JsonProfileRegistry` in `crates/colui-adapters/src/registry/format.rs` owns locked read-modify-write, validation, atomic replacement, and legacy-import backup. Explicit canonical backup restore is absent.
- `RuntimeConnector` already supports disconnect, but Tauri/frontend expose only state and connect.
- Rust and TypeScript runtime DTOs can represent `ContextMismatch`, but the current runtime command converts mismatch into a rejected command and hides its structured fingerprints.
- Frontend has one inventory poller and direct generation-aware cache publication after profile lifecycle. Only Projects routing exists.

These facts are constraints. Increment 5 changes the narrow interfaces needed for final workflows but preserves their owners and invariants.

## 3. Architecture and ownership

The selected architecture is a set of vertical use cases over existing peer owners.

```text
RuntimeGateway ── DockerApi/Bollard control and logs
      │
      └─ InventoryCoordinator ── immutable RuntimeInventory
                                      ├─ project/standalone projections
                                      └─ compose observation groups

InventoryReader + ProfileReader ── DiscoverySession ── discovery queries
ProfileStore + DiscoverySession ── registration commands

Runtime/Registry/Inventory/Definition/Operation readers
      └─ Diagnostics query ── typed DiagnosticsSnapshot

OperationLockManager + RegistryRecovery ── explicit backup restore
OperationLockManager + RuntimeGateway + InventoryCoordinator
      └─ standalone action command
```

`InventoryCoordinator` remains the sole owner of Docker inventory refresh. `RuntimeGateway` remains the sole API/CLI session and client owner. `OperationLockManager` remains the sole owner of profile lifecycle/definition locks. `DefinitionCache` remains the sole definition owner. The frontend remains a consumer of derived projections and never becomes policy authority.

New owners are narrowly scoped:

- `DiscoverySession` owns candidate/session correlation, ignored candidate IDs, auto-registration configuration/deduplication, and the session journal.
- `OperationLockManager` is extended as the one lock owner. It adds atomic container-keyed action leases, shared registry-mutation leases, and an exclusive recovery lease without changing existing profile lifecycle/definition semantics.
- `RegistryRecovery` is an explicit capability of the same `JsonProfileRegistry` adapter, not a second persistence owner. It is the sole exception to normal `ProfileStore::mutate` writes because recovery must replace an unreadable canonical file.

## 4. Inventory evidence and normalization

### 4.1 Compose observation groups

Extend `RuntimeInventory` with immutable `compose_observation_groups: Vec<ComposeObservationGroup>`. Existing `containers`, `project_snapshots`, and `standalone_containers` remain available and retain their current meanings.

```rust
pub struct ComposeObservationGroup {
    pub compose_project_name: String,
    pub working_directory: Option<String>,
    pub config_files: Vec<String>,
    pub container_ids: Vec<ContainerId>,
}
```

Groups are derived during the same coordinator normalization pass as existing projections. No second Docker list, inspect, registry read, or write occurs. Group identity is the normalized tuple `(compose_project_name, working_directory, config_files)`, not Compose name alone. Groups sort by normalized project name, working directory, config-file sequence, then container IDs so equal observations produce deterministic output.

### 4.2 Metadata normalization

- Compose name is trimmed and accepted only when `ComposeProjectName::try_from` accepts the exact value. Invalid grammar, including uppercase characters, is incomplete metadata; discovery does not rewrite runtime namespace.
- Working directory is trimmed, must be absolute, and is lexically normalized by removing `.` and resolving `..` without crossing the root. Filesystem canonicalization and existence checks are forbidden because discovery must remain observation-only and work offline against retained metadata.
- Config-file labels preserve source order. Each trimmed non-empty path is resolved against the normalized working directory when relative, then lexically normalized. Duplicate normalized paths are removed first-wins.
- Case is preserved. Runtime path comparison is byte-for-byte after lexical normalization; no unsupported cross-platform case-folding claim is made. Registered profile paths use the same representation: relative Compose files resolve against the profile working directory; relative working directories and non-UTF-8 paths are noncomparable and force `NameConflict` for the same Compose name rather than a guessed match.
- A group with missing/invalid working directory or no valid config files remains visible as incomplete metadata.

The four official labels remain the only public Compose metadata source. A generic labels map is not added to `ContainerInstance`.

## 5. Discovery model and state machine

### 5.1 Candidate contract

```rust
pub struct DiscoveryCandidate {
    pub candidate_id: CandidateId,
    pub runtime_session_id: RuntimeSessionId,
    pub inventory_generation: u64,
    pub compose_project_name: String,
    pub working_directory: Option<String>,
    pub config_files: Vec<String>,
    pub container_count: u32,
    pub classification: DiscoveryClassification,
    pub conflicts: Vec<DiscoveryConflictEvidence>,
    pub ignored: bool,
}

pub struct DiscoveryConflictEvidence {
    pub source: DiscoveryConflictSource,
    pub profile_id: Option<ProfileId>,
    pub working_directory: Option<String>,
    pub config_files: Vec<String>,
}

pub enum DiscoveryConflictSource { RuntimeObservation, RegisteredProfile }

pub enum DiscoveryClassification {
    NewUnambiguous,
    NameConflict,
    IncompleteMetadata,
    AlreadyRegistered,
}
```

`candidate_id` is stable only within one runtime session and identifies normalized Compose name plus normalized working directory; config files are mutable candidate metadata, not identity. It is lowercase hex SHA-256 over this byte sequence: ASCII tag `colui-candidate-v1`, 16 raw UUID bytes for runtime session, a big-endian `u32` UTF-8 byte length plus Compose-name bytes, then one presence byte (`0` absent, `1` present) followed when present by a big-endian `u32` length and working-directory UTF-8 bytes. Repeated projections from equal identity fields in the same session produce the same ID. Candidate IDs are opaque outside that session and are never persisted. If multiple observation groups share candidate identity but disagree on config files, Discovery emits one `NameConflict` candidate containing the deterministic first sorted tuple and conflict details; it never emits duplicate IDs.

### 5.2 Classification

Classification uses one immutable inventory snapshot and one immutable registry snapshot:

- `IncompleteMetadata`: normalized working directory is absent/invalid or normalized config files are empty.
- `NameConflict`: more than one complete runtime tuple has the same Compose name, or any registered profile with that Compose namespace has a different normalized working-directory/config-file tuple.
- `AlreadyRegistered`: no name conflict exists and a registered profile has the same Compose namespace and normalized tuple.
- `NewUnambiguous`: metadata is complete, exactly one runtime tuple has the Compose name, and no registered profile uses that Compose name.

Precedence is `IncompleteMetadata`, `NameConflict`, `AlreadyRegistered`, then `NewUnambiguous`. Multiple matching registered profiles still classify as `NameConflict`; discovery never chooses or merges them automatically.

Candidate listing is a pure query. It reads snapshots and `DiscoverySession`; it does not refresh inventory, inspect containers, validate files, run `compose config`, write registry, or schedule registration.

### 5.3 Session-local state

`DiscoverySession` observes runtime session ID changes. On a new successful session it clears projected candidates, ignored IDs, and registration-dedup state. It retains journal entries, each tagged with its originating session ID. Inventory generation changes within one runtime session do not clear ignore state. Disconnect removes active candidates but does not erase journal history.

Ignoring is an explicit in-memory command keyed by current `candidate_id`. It is idempotent, produces a journal event only on state change, and has no durable storage.

The journal is a single FIFO of at most 256 registration and operational events. Appending event 257 removes the oldest event. Entries contain monotonic sequence, timestamp, optional runtime session ID, event kind, severity, optional profile/container/candidate subject, optional stable error code, and a sanitized message capped at 500 Unicode scalar values. Error code is present only for failure events. Messages come from closed static templates selected by event kind/code and interpolate only validated opaque IDs and numeric counts; upstream error messages, paths, environment values, Docker labels, registry contents, and log text are never interpolated. Event kinds cover start/success/failure for connect, disconnect, reconnect, manual/auto registration, backup, restore, profile lifecycle, and container actions, plus candidate ignored. Inventory polling and log reads do not journal routine success. Tests inject secret marker strings through every excluded source and assert markers never reach journal DTOs.

## 6. Candidate registration and races

Manual and automatic registration call one application registration use case. Auto-registration defaults disabled and is an in-memory current-application-session setting owned by `DiscoverySession`; it is not persisted. Diagnostics/Projects expose an explicit accessible toggle command and current setting. When enabled, a subscriber to successful `InventoryCoordinator` publication schedules one separate `AutoRegisterCandidates` use case for that published session/generation. Publication completes before scheduling; refresh and discovery query contain no write. Equal generation is not rescheduled, and session dedup prevents repeat scheduling. Scheduled use case rechecks enabled state, runtime session, and scheduled generation before processing, so disabling toggle cancels queued work before any write. Auto-registration considers only nonignored `NewUnambiguous` candidates with complete labels. Filesystem existence and `compose config` are deferred to the existing definition flow.

Registration input carries `candidate_id`, `runtime_session_id`, `inventory_generation`, and the expected normalized tuple. The use case:

1. Acquires a generation-scoped candidate lease from `InventoryCoordinator`. Lease permits Docker observation but delays publication/replacement of current discovery evidence until release; it does not create another inventory owner.
2. Reads the current runtime session and inventory.
3. Rejects a changed session/generation, missing candidate, changed tuple, or candidate no longer registrable with a typed stale-candidate error.
4. Creates a discovered-origin profile draft using the candidate Compose namespace and ordered files. Profile ID is newly generated and immutable.
5. Enters `ProfileStore::mutate`, rereads under the registry's cross-process lock, revalidates the still-held candidate lease/session/generation immediately before mutation, and repeats classification against the locked registry snapshot.
6. Returns `profile_already_registered` when the same tuple now exists. It never overwrites, merges, or updates that profile.
7. Returns an explicit conflict when the Compose name now maps to another tuple or several profiles.
8. Writes once only when candidate remains new and unambiguous. Use case pre-generates 16 UUIDv4 profile IDs before entering `ProfileStore::mutate`; locked closure picks first unused ID and returns `registry_write_failed` if all 16 collide. Tests inject 16 collisions and verify bounded failure without write.

Repeated manual scheduling, repeated auto scheduling, manual/auto races, and two application instances therefore converge through the same locked check. Tuple equivalence, not generated profile ID, establishes registration idempotency.

Registry corruption, lock timeout, and write failure preserve their existing typed codes and produce journal events. Auto-registration does not retry nonretryable failures. Retryable lock/write failures may be retried only by a later explicit auto-registration command; no unbounded background loop is introduced.

## 7. Diagnostics

`DiagnosticsSnapshot` is a read-only aggregate of independently versioned projections:

- current `RuntimeSessionState`, resolved endpoint, API fingerprint, CLI fingerprint, session ID, and connection timestamp;
- registry path, backup path, revision, health, lock timeout, and last recovery result;
- retained legacy v1 import result with source path, imported count, source-preserved flag, and typed failure;
- active profile and container operations with kind, subject ID, start timestamp, and phase;
- inventory generation, freshness, observed timestamp, last successful timestamp, current session/fingerprint, and retained error;
- definition state/issues per profile without triggering definition loads;
- the 256-entry session journal.

The aggregate does not promise one global transaction. Each section carries its own generation, revision, or observed timestamp. Concurrent inventory refresh or lifecycle completion may make one section newer than another; Diagnostics displays those markers rather than fabricating consistency. Reading Diagnostics never acquires operation locks, runs Docker/Compose, refreshes definitions/inventory, or writes registry.

`ContextMismatch` is returned as structured runtime state, not converted into command failure. Diagnostics shows resolved endpoint and both fingerprints, explains that Compose operations are blocked, and permits API-backed inventory when the verified API client remains usable. `RuntimeGateway` therefore exposes an API-read context for both `Ready` and `ContextMismatch`; it contains session ID, endpoint, and API fingerprint, while the existing Compose-ready context remains `Ready`-only. `RuntimeInventorySource` and the coordinator session guard use API-read context before and after listing. Runtime, registry, inventory, definition, and operation errors remain independent typed fields.

Connect, disconnect, and reconnect are explicit commands. Connect remains idempotent. Disconnect drops the gateway client and updates state without disabling profile/registry access. Reconnect returns `ReconnectResult { runtime_state, inventory }`, where `inventory` is optional. It performs disconnect then connect as one use case; after `Ready` or API-readable `ContextMismatch`, it requests exactly one explicit coordinated inventory refresh. If connection reaches an API-readable terminal state but refresh fails, reconnect still resolves with that runtime state and the coordinator's retained `current_inventory()` carrying stale/unavailable error. A failed connect rejects with its typed error and does not invent inventory.

## 8. Registry health, backup, and restore

### 8.1 Canonical backup

The canonical backup path is `registry.json.bak` beside `registry.json`. It is created only by explicit `create_registry_backup` after an accessible confirmation naming canonical and backup paths; ordinary profile mutations never rotate it. Legacy migration continues to back up its legacy source separately and never writes legacy bytes to `registry.json.bak`. Backup command validates `registry.json` first and atomically writes its exact bytes. Missing, corrupt, or unsupported canonical input cannot replace a valid backup. Diagnostics exposes backup existence, timestamp, validation state, and Create/Replace backup control.

Canonical backup update uses temp write, private owner-only permissions, file fsync, atomic rename, and parent-directory fsync. Failure aborts explicit backup/repair with `registry_write_failed`; durability is not weakened silently. Legacy migration retains its existing separate source-backup guarantees.

### 8.2 Recovery exclusion

Every profile mutation use case, discovery registration, definition lease, profile lifecycle, and container action must acquire its typed lease from the single `OperationLockManager`; ports do not expose an unguarded production constructor. Each process-local lease also holds a shared advisory file lease on `registry.recovery.lock`; restore obtains its exclusive nonblocking file lease after obtaining process-local exclusive recovery lease. Thus another app instance's mutation, definition, or operation makes restore fail fast. After shared file lease acquisition and before definition/runtime work, each profile operation rereads canonical registry revision, invalidates local profile/definition projections if revision differs, and resolves current profile again. Idle instances therefore cannot operate on pre-restore cached profiles. Lock order is process-local operation/recovery lease, recovery file lease, registry file lock, then runtime call. While recovery is pending/active, new local leases fail with `operation_conflict`. Existing registry lock timeout semantics remain separate.

Once exclusive:

1. Read and fully validate `.bak` as schema v2, recording SHA-256 of its bytes.
2. Acquire the existing cross-process registry lock with its bounded timeout. Canonical state read under this lock is authoritative; changes before lock acquisition are serialized into this request rather than compared with an unlocked snapshot.
3. Reread and revalidate `.bak`; a changed SHA-256 rejects the request as a recovery conflict rather than silently selecting another backup.
4. If canonical exists, preserve its bytes, including corrupt bytes, in the registry directory as `registry.pre-restore.<UTC timestamp>.<random suffix>.json` using exclusive private creation. Missing canonical needs no artifact and may be restored. Keep newest three artifacts and delete older artifacts only after successful restore. Failure to preserve an existing canonical file aborts restore.
5. Build restored schema-v2 bytes from backup profiles. If canonical validates, set registry revision above both canonical and backup revisions and each restored profile revision above matching canonical/backup profile revisions. If canonical is missing or corrupt, it is never parsed as authority: registry and profile revisions advance from validated backup only. Full frontend/backend registry and definition caches are invalidated after restore in either case. Revision overflow aborts with `registry_write_failed`.
6. Atomically replace canonical through `JsonProfileRegistry`'s recovery capability and fsync the parent.
7. Reread and validate canonical, publish its actual revision/health, invalidate all definition entries, and journal the result.

Cross-instance profile mutations are excluded by the registry lock. Backup replacement before lock acquisition is detected by hash comparison; canonical changes are serialized and the locked reread becomes restore authority. Restore never parses a corrupt canonical as authority, never silently repairs on startup/read, and never invokes Docker.

Registry health is computed on each Diagnostics read from canonical parse first: `corrupt` has precedence over retained transient failures, then `missing`, then latest retained operation failure (`locked` or `write_failure`), otherwise `healthy`. Successful reads do not clear transient failure history. A later successful mutation, explicit backup, or restore clears it; corruption is always derived from current bytes and cannot be masked. Health includes operation and failure timestamps plus the stable typed error needed for UI recovery. Application startup invokes existing v1 import once where applicable and retains the result for Diagnostics. Malformed legacy data remains preserved and does not prevent the offline shell.

## 9. Standalone container actions

Other containers is derived from `RuntimeInventory.standalone_containers`; no second list endpoint or polling owner is introduced.

```rust
pub enum ContainerAction { Start, Stop, Restart }

pub struct ContainerActionResult {
    pub container_id: ContainerId,
    pub action: ContainerAction,
    pub observation: ContainerActionObservation,
    pub inventory: RuntimeInventory,
}

pub enum ContainerActionObservation { ConfirmedInSession, IndeterminateAfterSessionChange }
```

Commands accept `container_id`, action, and expected `runtime_session_id`; no names, endpoints, or paths. The use case rejects session mismatch before acquiring action lease and rechecks it immediately before Bollard mutation. It obtains an atomic container lease from extended `OperationLockManager`. Duplicate actions for one ID return `operation_conflict`; different container IDs may proceed concurrently. Existing profile lifecycle IPC remains strictly `{ profileId }`.

Start, stop, and restart map directly to Bollard APIs. They do not invoke Compose. A missing/disappearing container and daemon errors map to `container_operation_failed` with container subject metadata and sanitized details.

If Bollard reports daemon success, the action remains successful even when runtime session changes before response handling; result observation is marked `indeterminate_after_session_change` and exactly one coordinator refresh is still attempted against current session. After every daemon success, use case calls `InventoryCoordinator.refresh()` exactly once while its lease remains held. Because `refresh()` currently returns `Err` while retaining failure state, error path immediately reads `current_inventory()` and returns that retained stale/unavailable snapshot with typed observation error. Frontend publishes embedded inventory through existing generation structural sharing and performs no invalidation or follow-up refresh.

## 10. Container logs

Logs are explicit on-demand snapshots. Docker options are fixed: `stdout=true`, `stderr=true`, `follow=false`, `timestamps=false`, `since=0`, `until=0`, and `tail=all`. No polling, live follow, history, persistence, or query retry occurs. Cancellation drops the stream/client future without retaining partial text.

```rust
pub struct ContainerLogsRequest { pub container_id: ContainerId }

pub struct ContainerLogs {
    pub container_id: ContainerId,
    pub text: String,
    pub retained_bytes: u32,
    pub truncated: bool,
    pub observed_at: Timestamp,
}
```

Ordering is Bollard `LogOutput` item arrival order for multiplexed streams and raw chunk arrival order for TTY streams. Adapter framing bytes are excluded from payload accounting. Chunks feed the ring incrementally; full output is never buffered first. A ring retains the newest `262_144` payload bytes across both streams. `truncated` is true exactly when at least one payload byte was discarded; output of exactly 262,144 bytes is not truncated. Decode occurs only after byte retention using UTF-8 replacement semantics. Every maximal invalid sequence, including a leading fragment caused by truncation or trailing partial code point from Docker, becomes `U+FFFD`. `retained_bytes` reports bytes before UTF-8 replacement.

Session generation is checked before and after the request. Container disappearance returns `container_operation_failed`; no stale text is retained. Log text and container output never enter the session journal or diagnostics errors.

## 11. Port binding policy

Every published binding preserves and displays host IP, host port, container port, and normalized lowercase transport. Domain/DTO host IP and host port become optional so Docker entries lacking either remain representable instead of being dropped. The canonical complete display/copy string is:

```text
<host>:<hostPort> -> <containerPort>/<transport>
```

IPv6 hosts use brackets. Wildcard hosts remain `0.0.0.0` or `[::]` in display and copy. Copy is always available. Exact incomplete display/copy forms are `<host>:? -> <containerPort>/<transport>` when only host IP exists, `?:<hostPort> -> <containerPort>/<transport>` when only host port exists, and `<unpublished> -> <containerPort>/<transport>` when both are absent.

Browser open is available only for complete TCP bindings where container port or host port belongs to `80`, `443`, `3000`, `5173`, `8000`, `8080`, or `8443`. Container port determines recognized service and scheme when recognized; otherwise recognized host port does. Effective service ports `443` and `8443` use `https`; others use `http`. URL authority always uses actual host port. UDP and unknown transports never become URLs. For URL construction only, `0.0.0.0` maps to `127.0.0.1` and `::` maps to `[::1]`; displayed/copied values remain unchanged. Nonwildcard IPv6 remains bracketed. Partial bindings retain exact partial display/copy forms above and disable browser open; no host component is guessed.

URL derivation is a pure domain/application presentation policy with table-driven tests. Browser launch occurs through a narrow Tauri shell opener after URL validation; frontend cannot submit arbitrary URLs to that command.

## 12. IPC and frontend contracts

Add focused Rust DTOs, Schemars output, TypeScript Zod schemas, semantic contract-manifest entries, validated command wrappers, and mock-backend cases for:

- discovery list, ignore, auto-registration configuration, manual registration, and auto-registration;
- diagnostics snapshot, connect, disconnect, reconnect, create/replace backup, and restore;
- container action and logs;
- port action projection where backend derivation is required.

All errors use `AppErrorDto`; no `Result<_, String>` is introduced. Error subjects are generalized to a typed subject `{ kind, id }` so profile, candidate, and container failures are not misrepresented as `ProfileId`. Existing profile error serialization remains semantically compatible through `kind: "profile"` within this unreleased baseline.

Frontend transport remains exclusively in `src/ipc/`. Mock and Tauri responses pass the same decoder. Protocol mismatch stays nonretryable and never yields fallback data.

Primary navigation has working Projects and Diagnostics views. Projects contains registered profiles, discovery candidates/conflicts, and Other containers. Diagnostics owns no second inventory poller; the application shell owns the single visible-window inventory observer and shares its cache. Query keys use immutable IDs:

- `inventoryKeys.snapshot()` remains the only inventory cache key and polling owner;
- discovery is keyed by current runtime session ID and inventory generation;
- diagnostics uses one read-only projection key;
- logs use container ID and explicit request shape and are disabled until requested;
- candidate/profile/container mutations use their immutable IDs for pending state.

Registration success invalidates profile list and discovery projection. Ignore changes only discovery session projection. Runtime commands update runtime state; reconnect clears old discovery cache and publishes its one returned/refreshed inventory through the generation guard. Container action publishes embedded inventory directly.

Views are responsive from narrow mobile width through desktop. Every action is keyboard reachable. Menus restore trigger focus; dialogs trap focus; destructive restore initially focuses Cancel and names the selected backup/path and consequences. Logs use a selectable preformatted region with an accessible truncation notice. Runtime/registry failures use stable-code recovery actions and expandable sanitized details. Offline profile navigation/edit/remove and Diagnostics registry data remain available without Docker.

## 13. Race and safety matrix

| Scenario | Required result |
| --- | --- |
| Candidate disappears between list/register | Registration rejects stale/missing candidate; registry unchanged. |
| Candidate metadata changes while pending | Expected tuple/session/generation check rejects stale candidate. |
| Two instances register same candidate | Locked registry recheck returns `profile_already_registered` to loser; no overwrite. |
| Manual and auto registration race | Shared registration use case and locked tuple check produce one profile. |
| Compose name collides with one/several profiles | `NameConflict`; no automatic selection, merge, or update. |
| Registry corrupt/locked during registration | Existing typed registry error; candidate remains transient; journal records sanitized failure. |
| Restore during mutations/operations | Exclusive recovery barrier fails fast with `operation_conflict`. |
| Other process mutates around restore | Registry lock plus backup reread detects/serializes race. |
| Reconnect changes runtime session | Candidate IDs, candidates, ignore, and dedup reset; journal retains session-tagged history. |
| Container disappears before action/logs | `container_operation_failed`; no invented success or retained logs. |
| Logs exceed 256 KiB | Newest 262,144 payload bytes retained; `truncated=true`. |
| Log retention cuts UTF-8 | Decode retained bytes lossy; invalid fragment becomes `U+FFFD`. |
| Wildcard/IPv6/UDP/unknown port | Original display/copy preserved; only eligible TCP web binding gets validated URL. |
| Diagnostics races refresh/operation | Independently versioned sections; query has no side effects and no false global atomicity. |

## 14. Test strategy

Implementation follows red-green-refactor for each vertical slice.

Hermetic Rust tests cover metadata normalization, deterministic IDs, all classifications, query purity, idempotent/lost-race registration, no-update-existing policy, auto/manual races, session reset, ignore behavior, 256-event FIFO bounds, registry health/backup/restore, recovery barrier races, operation projections, disappearing containers, action refresh semantics, exact 262,144-byte retention, UTF-8 replacement, and port URL/copy tables.

Adapter tests use fake Docker controls and temporary registries. They assert one list per inventory refresh, no inspect during discovery, runtime-session guards, exact Bollard action mapping, stream framing exclusion, backup durability sequence, corruption preservation, and cross-instance lock behavior.

IPC tests update generated schemas and semantic Zod parity. Frontend tests cover query keys/invalidation/publication, typed recovery, mismatch explanation, offline shell, discovery conflict/manual/auto flows, keyboard focus, responsive navigation structure, logs truncation notice, copy/open eligibility, and no dead navigation item.

The real-Docker fixture receives only four focused additions: official-label discovery, one standalone action, bounded logs, and one published TCP binding. The existing apply-stop-apply-tear-down fixture is reused, not duplicated.

Required verification commands are the repository's Rust workspace tests and checks, Tauri DTO/schema drift tests, frontend Vitest/typecheck/build/lint commands, and the opt-in real-Docker feature suite on a supported local daemon. The implementation plan must record exact commands from current package manifests and Cargo features.

## 15. Delivery sequence

1. Preserve observation groups and add discovery domain normalization/classification.
2. Add pure discovery queries and session-local ignore/journal.
3. Add atomic manual/auto registration and race handling.
4. Add registry health, explicit canonical backup/recovery leases, restore, and import-result retention.
5. Add Diagnostics projection and runtime disconnect/reconnect controls.
6. Add standalone container ports, locks, actions, and coordinated inventory results.
7. Add bounded logs and port URL/copy policy.
8. Extend DTO/schema/IPC/mock contracts.
9. Build shared shell, Discovery/Other containers workflows, and Diagnostics UI.
10. Extend focused real-Docker coverage and run final baseline acceptance.

Each step includes its tests and a scoped commit. No step introduces a temporary second owner or path-bearing compatibility API.

## 16. Final CoLUI 2.0 acceptance

Completion requires all 22 master criteria:

1. Persisted profiles use stable `ProfileId`.
2. Display rename does not change Compose namespace.
3. Lifecycle IPC contains no paths.
4. Backend resolves current profile through registry.
5. Profiles never merge solely by name.
6. Project/discovery/diagnostics queries never write registry.
7. Runtime refresh performs one container listing.
8. Lower generations are discarded and equal generations are no-ops.
9. Fast polling never runs `compose config`.
10. API and CLI fingerprints identify the same daemon before Compose actions.
11. Context mismatch blocks Compose actions and remains diagnosable.
12. Compose timeout terminates and reaps the OS child.
13. Stop invokes `docker compose stop`.
14. Tear down invokes `docker compose down`.
15. Remove profile never invokes Docker.
16. Docker outage does not block profile/offline shell access.
17. Reconnect requires no application restart.
18. One invalid profile does not hide others.
19. Runtime and definition errors remain independent.
20. Service definitions and container instances remain separate types.
21. v1 import preserves ordered paths and source data.
22. Real fixture passes apply, stop, apply, and tear down.

Quality gates additionally reject hidden registry writes, name-only discovery merge, path-bearing lifecycle IPC, duplicate inventory/runtime/lock/lifecycle owners, direct frontend `invoke`, stale schemas, string IPC errors, unsupported platform claims, and navigation entries without working views.

## 17. Explicit invariants

- Existing profiles are never changed from Docker labels.
- Discovery consumes immutable coordinated inventory and performs no Docker read.
- Auto-registration is a separate visible write use case.
- Profile ID, display name, Compose namespace, and candidate ID remain distinct.
- Runtime reconnect never resets inventory generation.
- Definition and runtime failures remain independent.
- Stop, tear down, remove, and ID-only profile lifecycle semantics do not change.
- Successful standalone action performs one coordinated refresh; failed action performs none.
- Application remains useful without Docker.
