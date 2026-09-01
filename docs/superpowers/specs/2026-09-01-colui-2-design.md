# CoLUI 2.0 Greenfield Design

**Status:** approved design
**Date:** 2026-09-01
**Source:** `CoLUI_architecture_audit_v2.md`

## 1. Scope and decisions

CoLUI 2.0 is a greenfield macOS desktop application for managing local Docker containers and Docker Compose projects. It uses Tauri 2, Rust stable, React 18, TypeScript strict, Vite, Bollard, and the external `docker compose` CLI.

Implementation targets the audit's 2.0 model directly. No legacy IPC, `SavedProject`, hybrid `ComposeProject`, path-bearing lifecycle commands, name-based identity, or hidden registry writes are introduced. Legacy `~/.colui/projects.json` receives one read-only first-start import path; it is never modified.

Work proceeds headless-first in five vertical increments:

1. Domain model and registry.
2. RuntimeGateway and process ownership.
3. Typed IPC and minimum usable UI.
4. Inventory coordinator and definitions.
5. Discovery, diagnostics, offline recovery, standalone containers, logs, and ports.

This is the product-level master design. Each increment receives its own bounded implementation plan and completion/review cycle; planning begins with Increment 1 rather than treating all five increments as one execution unit.

Defaults from audit section 25 are accepted: project-first UI pending product validation; standalone containers under Other containers; auto-registration only for new unambiguous projects; no automatic updates to existing profiles; Compose-name collisions are explicit conflicts; one local Docker context; no remote Docker; macOS is the only supported baseline; missing explicit env files invalidate definitions; one registry writer; coordinated polling rather than Docker Events; no Images view; bounded log snapshots; no SQLite, RBAC, plugins, or live-follow logs.

## 2. Repository architecture

```text
colui2/
├── Cargo.toml
├── crates/
│   ├── colui-domain/
│   ├── colui-app/
│   └── colui-adapters/
├── src-tauri/                 # colui-tauri crate
├── package.json
├── src/
│   ├── app/
│   ├── features/
│   │   ├── projects/
│   │   ├── containers/
│   │   └── diagnostics/
│   ├── ipc/
│   └── ui/
└── schemas/                   # committed Rust-generated JSON Schema
```

Dependency direction is `colui-domain <- colui-app <- colui-adapters <- colui-tauri`.

- `colui-domain`: pure types and rules. May use `serde`, `schemars`, `uuid`, and `thiserror`; must not use Tokio, Bollard, Tauri, filesystem, or process APIs.
- `colui-app`: use cases and adapter ports. May use domain and async primitives; must not use Bollard or Tauri.
- `colui-adapters`: filesystem registry, Bollard adapter, Compose adapter, and child process runner. Must not use Tauri.
- `colui-tauri`: shell, tray, dependency wiring, IPC commands, and DTO mapping. Must not contain domain rules or directly invoke Bollard/process APIs.

Runtime, Profiles, Definitions, Inventory, Operations, and Discovery remain focused modules within application and adapter crates. Separate crates are used only where compiler-enforced dependency boundaries add value.

Application ports include `ProfileReader`, `ProfileStore`, `DockerApi`, `ComposeRunner`, `Clock`, and `IdGenerator`. Deterministic clock and ID ports support reliable tests.

## 3. Domain model

### Project identity

```rust
pub struct ProfileId(Uuid);
pub struct ComposeProjectName(String);
pub struct DisplayName(String);

pub struct ProjectProfile {
    pub id: ProfileId,
    pub revision: Revision,
    pub display_name: DisplayName,
    pub compose_project_name: ComposeProjectName,
    pub working_directory: PathBuf,
    pub compose_files: Vec<PathBuf>,
    pub environment_files: Vec<PathBuf>,
    pub registration_origin: RegistrationOrigin,
}
```

`ProfileId` is immutable. Display name, Compose namespace, and persistence identity are distinct types. Duplicate display names are allowed. Compose names are validated namespaces. File order is significant. Profiles contain no runtime state, container IDs, or transient errors.

`ProfileDraft` is validated by one domain function. Domain validation checks names, non-empty Compose files, and duplicate paths. Filesystem existence is adapter-level validation that produces issues; invalid or missing paths do not remove or hide a profile.

### Definition and runtime

```rust
pub struct ProjectDefinition {
    pub profile_id: ProfileId,
    pub definition_revision: DefinitionRevision,
    pub loaded_at: Timestamp,
    pub state: DefinitionState,
    pub services: Vec<ServiceDefinition>,
    pub issues: Vec<Issue>,
}

pub struct ContainerInstance {
    pub id: ContainerId,
    pub name: String,
    pub image: String,
    pub state: ContainerState,
    pub status_text: String,
    pub service_name: Option<String>,
    pub published_ports: Vec<PortBinding>,
}
```

`ServiceDefinition` and `ContainerInstance` are separate. A service has zero to many runtime instances. Runtime inventory is an immutable snapshot with `generation`, `observed_at`, `runtime_session_id`, daemon fingerprint, project snapshots, and standalone containers.

State axes remain independent:

```rust
pub enum RuntimePresence { Unavailable, Absent, Present }
pub enum RuntimeActivity { AllRunning, Mixed, NoneRunning }
pub enum DefinitionState { Unchecked, Valid, Invalid, Stale }
pub enum OperationKind { Apply, Stop, TearDown, Restart }
```

No combined `ProjectStatus` exists. User-facing labels are derived presentation values from definition state, runtime presence, and activity.

Port bindings preserve `host_ip`, `host_port`, `container_port`, and transport protocol. Copying is always available. URL opening is available only for recognized web TCP ports. UDP is never treated as a URL. Displayed/copied addresses are not rewritten; wildcard host addresses may be converted to loopback only when constructing a browser URL.

### Typed errors

All application and IPC failures use `AppError` with `code`, `operation`, optional `subject_id`, user-facing `message`, optional diagnostic `details`, and derived `retryable`.

Required stable codes are `runtime_unavailable`, `runtime_connection_failed`, `runtime_context_mismatch`, `profile_not_found`, `profile_revision_conflict`, `profile_invalid`, `definition_failed`, `compose_failed`, `container_operation_failed`, `operation_conflict`, `operation_timeout`, `registry_corrupt`, `registry_locked`, `registry_write_failed`, `permission_denied`, and `protocol_mismatch`.

## 4. ProfileRegistry

The canonical registry is `~/.colui/registry.json`:

```json
{
  "schemaVersion": 2,
  "registryRevision": 14,
  "profiles": []
}
```

`registryRevision` increments on changed file writes. Each profile revision increments only when that profile changes and supports optimistic concurrency for update/removal.

Reading and writing use separate ports. Queries receive `ProfileReader`; mutating use cases receive `ProfileStore`. Only `ProfileStore::mutate` can write.

Write sequence:

1. Acquire an exclusive advisory lock at `~/.colui/registry.lock`, with a two-second timeout.
2. Read and parse canonical bytes. Parsing failure returns `registry_corrupt` without modifying the file.
3. Apply and validate one mutation.
4. Exit without writing if serialized content is unchanged.
5. Write a temporary file, fsync it, atomically rename it, then fsync the parent directory.
6. Re-read and validate the canonical file before releasing the lock.

Backups are created before schema migration and explicit repair, not before every normal write. Corrupt registries remain untouched. Diagnostics offers explicit restore from backup; repair never runs silently.

### One-time v1 import

When `registry.json` is absent and legacy `projects.json` exists, first startup performs one read-only import. It creates UUIDs, revision 1, maps old name to display and Compose names, preserves path order, and sets origin to `Migrated`. It backs up legacy bytes before writing v2. Legacy file is neither changed nor deleted.

Legacy names are normalized to lowercase Compose namespaces by replacing invalid runs with `-` and trimming separators. If the result is empty, importer uses `imported-<first-eight-profile-id-hex>`. Profiles that normalize to the same Compose name remain separate and surface as explicit conflicts; they are never merged.

Malformed legacy data does not block application startup. Legacy file remains untouched, an empty v2 registry is created, and Diagnostics reports the import issue with a recovery path to the source/backup. The importer is isolated in one removable adapter module and has no other consumers.

## 5. RuntimeGateway

`RuntimeGateway` owns one runtime session, Bollard adapter, Compose adapter, and `ComposeProcessRunner`. No use case creates its own Docker client or starts `docker` directly.

Session states are `Disconnected`, `Connecting`, `Ready`, `ContextMismatch`, and `Failed`. Application starts disconnected so profiles remain accessible without Docker.

Connection sequence:

1. Resolve one local endpoint from explicit preference, `DOCKER_HOST`, or default socket.
2. Connect Bollard and obtain API daemon fingerprint.
3. Build explicit CLI environment from the same resolved endpoint.
4. Invoke `docker info` and obtain CLI fingerprint.
5. Compare daemon ID, server version, OS type, and architecture.
6. Enter `Ready` only when equal; otherwise enter `ContextMismatch`.

Inventory may remain visible with a mismatch warning, but Compose operations are blocked. Reconnect is supported without restarting the application. Context-affecting inherited Docker variables are cleared or overwritten. Ordinary environment such as `PATH`, locale, and `HOME` remains available so Docker executable and credential/config helpers work.

### Child process ownership

Compose invocations contain executable, separate argv, backend-resolved cwd, explicit session environment, and deadline. Shell interpolation is never used.

The runner spawns a process group, drains stdout/stderr into 64 KiB tail buffers, waits until deadline, sends SIGTERM to the process group, escalates to SIGKILL after five seconds, and always waits/reaps before returning. Timeout does not merely drop a future. Results retain exit code, bounded output, and timeout state for typed diagnostics.

## 6. Use cases and IPC

Application use cases are focused objects: profile CRUD/inspection; runtime connect/disconnect/state; project apply/stop/tear-down/restart/details/summaries; inventory and definition refresh; discovery list/register/ignore; standalone container lifecycle/logs.

Read-only IPC includes application state, summaries, details, candidates, standalone containers, bounded logs, draft inspection, inventory refresh, and definition refresh. Refreshes may update memory caches but never durable registry state.

Lifecycle commands accept only `profileId`. Backend resolves current profile paths before execution. Flow is profile lookup, per-profile lock, ready session check, path resolution, Compose invocation, typed result, and exactly one inventory refresh on success.

- Apply executes `docker compose up -d`.
- Stop executes `docker compose stop`.
- Tear down executes `docker compose down`.
- Restart executes `docker compose restart`.
- Remove profile changes registry only and never invokes Docker.

One lifecycle operation per profile is allowed. Compose CLI starts with a global concurrency limit of one. Container operations remain independent.

Tauri commands only decode input, call use cases, map DTOs, and serialize `AppError`. No `Result<_, String>` is allowed at IPC boundaries.

Frontend calls pass through one `src/ipc/` function that invokes Tauri and validates every response with Zod. Validation failures become `protocol_mismatch`, never empty results. Direct Tauri invocation outside `src/ipc/` is linted as an error.

Rust DTOs derive `schemars` JSON Schema. Generated schemas are committed under `schemas/`; CI detects stale schemas. Zod schemas are converted to normalized JSON Schema and compared semantically with Rust-generated schemas, then validate generated positive and negative contract fixtures. DTO drift therefore fails CI while runtime responses still cross the Zod decoder boundary.

## 7. Inventory and definitions

One backend `InventoryCoordinator` owns refresh state and immutable snapshots. At most one refresh runs; concurrent callers share its result. Generation numbers increase monotonically. Publication discards generations older than the current snapshot.

Every fast refresh performs exactly one `list all containers` API call, normalizes instances, associates Compose containers using official labels, and derives project snapshots, discovery candidates, and standalone containers. Runtime observations never mutate profiles.

On refresh failure, last successful data remains visible with timestamp and stale/unavailable state. Coordinator applies backoff. Frontend rejects generations lower than its current generation because response delivery can be reordered; equal generations are accepted because concurrent callers may share one backend refresh result.

Definition loading is a slow path via `docker compose config`. It runs after profile changes, when project details enter foreground, on explicit refresh, or after a 60-second stale expiry; it never runs on each three-second runtime poll. Cache entries include profile revision, definition revision (hash of normalized config output), load time, services, and issues. No file watchers are introduced.

Operation locks allow one lifecycle action per profile and return `operation_conflict` for duplicates. Background definition refresh yields to lifecycle operations. The initial global Compose semaphore is one and may change only after measurement.

## 8. Discovery and diagnostics

Discovery candidates are transient runtime observations, not profiles. They carry a derived candidate ID, Compose name, optional working directory, config files, container count, and one classification: new unambiguous, name conflict, incomplete metadata, or already registered.

New unambiguous valid candidates may be auto-registered through a separate command/use case. Refresh/query itself does not write. Command deduplicates candidate IDs within the session and rechecks registry under its write lock, making repeated scheduling idempotent. Existing profiles are never auto-updated. Same Compose name with different paths is an explicit conflict. Incomplete metadata remains visible but cannot be registered. Auto-registration is configurable and logged in a bounded in-memory session journal. Ignored candidates remain ignored only for the current session.

Diagnostics displays runtime state, endpoint and fingerprints, reconnect controls, registry health/revision/path, restore action, import result, active operations, and bounded session events/errors. Runtime, registry, definition, and operation failures remain distinguishable.

## 9. Frontend

Frontend uses React 18, TypeScript strict, Vite, pnpm, TanStack Query, Tailwind, and headless Radix primitives. React owns view/dialog/form/sort state. Rust owns registry, session, inventory, definitions, operation locks, and error classification.

Primary navigation has Projects and Diagnostics. Projects includes registered environments, discovery conflicts, and Other containers. No Images item exists.

TanStack Query handles caching, invalidation, retries, and backoff. A generation guard keeps previous cache data when a stale response arrives. Polling defaults to three seconds, refreshes on window focus, and pauses when hidden. Successful lifecycle responses carry the already-produced generation; frontend invalidates against it and does not trigger a redundant second refresh.

List and pending keys use immutable IDs only. Frontend never sends paths or names in lifecycle requests and does not optimistically mutate authoritative server projections.

Stop is a normal non-destructive command. Tear down appears in an overflow/destructive context and requires an accessible Radix confirmation explaining that containers/networks are removed while profile remains. Remove profile has separate confirmation explaining that Docker is untouched.

Application shell and profiles load without Docker. Runtime unavailable, stale timestamp, invalid definition, and active runtime can be shown simultaneously rather than collapsed into one status. Error recovery is selected by stable error code; diagnostic details are expandable.

Browser development uses a mock implementation of the same IPC interface and passes responses through the same Zod decoders.

## 10. Testing

Three test levels are required:

1. Hermetic unit/application/frontend tests: `cargo test --workspace` and Vitest. Domain, use cases on fake ports, registry in temporary directories, contract logic, and frontend behavior require no Docker or network.
2. Hermetic adapter tests: real process runner with a fake executable verifies argv, cwd, env, bounded output, exit mapping, timeout, process-group termination, escalation, and reap. Docker application behavior uses a fake `DockerApi` port.
3. Real Docker tests: `cargo test --features docker-tests`, run separately where a daemon exists and validated on Docker Desktop and Colima.

Coverage includes stable identity after rename; separate display and Compose names; duplicate displays; collision conflicts; immutable snapshots; scaled services; invalid profile visibility; separate lifecycle intents; atomic writes; lock/revision conflicts; corruption and recovery; lossless v1 import; no writes from queries; profile-ID-only lifecycle; one refresh generation after success; stale generation rejection; no definition load on fast polling; mismatch blocking; offline startup/reconnect; typed UI recovery; destructive confirmation; and protocol mismatch handling.

Real Docker fixture creates a temporary Compose project, connects and verifies fingerprints, registers and applies it, observes containers, stops without deleting resources, reapplies, tears down while preserving profile, changes and refreshes definition revision, then removes profile and fixture. Additional cases cover same names in different directories, missing env files, timeout, registry corruption, context mismatch, scaled services, and startup without Docker.

## 11. Delivery increments and acceptance

### Increment 1: domain and registry

Create workspace, dependency rules, domain types, profile ports/use cases, atomic registry, lock/backup/recovery, and v1 import. Complete domain and registry tests.

### Increment 2: RuntimeGateway

Implement endpoint resolution, session/fingerprint gate, Bollard/Compose adapters, process ownership, hermetic adapter tests, and first real-Docker checks.

### Increment 3: minimum usable desktop slice

Implement schemas/Zod boundary, Tauri commands, profile list/editor, ID-based apply/stop/tear-down/restart, typed errors, single-shot inventory status, and confirmations. This is first runnable user-facing application.

### Increment 4: coordinated inventory

Implement generations, stale/backoff behavior, fast and slow paths, definition cache, operation locks, and TanStack polling.

### Increment 5: discovery and recovery

Implement candidates/conflicts/auto-registration policy, Diagnostics, offline shell/reconnect, standalone containers, bounded logs, and port actions.

Completion requires all 22 acceptance criteria and quality gates from the audit: stable profile IDs; identity-independent rename; path-free lifecycle IPC; backend lookup; no name-only merge; read-only project queries; one container listing per refresh; stale generation rejection; no Compose config in fast poll; shared daemon verification; mismatch blocking; hard child termination/reap; separate Stop/TearDown/Remove semantics; offline profile access/reconnect; invalid profile visibility; separate runtime/definition errors and types; lossless v1 import; and real apply-stop-apply-tear-down fixture.

Automated quality gates reject IPC `Result<_, String>`, direct frontend `invoke` outside `src/ipc/`, stale generated schemas, forbidden dependency edges, Compose-name React keys, hidden registry writes, duplicate lifecycle implementations, unsupported platform claims, and navigation entries without working views.

## 12. Explicit non-goals

Version 2.0 baseline excludes Linux/Windows release promises, remote/multiple Docker contexts, Docker Events, live-follow logs, Images workflow, SQLite, RBAC, plugin architecture, Kubernetes/Podman abstractions, custom Compose orchestration, event sourcing, generic resource frameworks, and visual redesign as a starting deliverable.
