# CoLUI Increment 3: Typed IPC and Minimum Usable UI

**Status:** design approved in conversation; written spec awaiting user review
**Date:** 2026-09-02
**Scope:** Tauri IPC, DTO contracts, browser mock, and first usable Projects UI
**Depends on:** [`2026-09-01-colui-2-design.md`](2026-09-01-colui-2-design.md), Increment 1, and Increment 2

## 1. Goal and scope

Increment 3 is the first runnable user-facing slice of CoLUI 2. It exposes the existing `colui-app` profile use cases and Increment 2 `RuntimeGateway` through typed Tauri commands, then supplies a responsive React/Vite/TypeScript strict shell for manually registered Compose profiles.

The slice supports profile list, create, edit, remove, single-shot project status, runtime auto-connect, and lifecycle actions addressed only by immutable `profileId`: Apply, Stop, Tear down, and Restart. Profile access and CRUD remain available when Docker is unavailable. Tear down removes Compose runtime resources while retaining the profile. Remove profile changes only the registry and never calls Docker.

Increment 3 does not implement `InventoryCoordinator`, polling, generation guards, stale/backoff policy, definition cache, discovery, standalone containers, logs, ports, Diagnostics navigation, Docker Events, remote contexts, or a second lifecycle implementation. The single-shot status command uses the already available application/runtime contracts and does not create a coordinator.

## 2. Approved decisions

- Browser mock uses the same TypeScript IPC interface as Tauri. It is a replaceable dispatch implementation, not a second frontend API.
- Every mock and Tauri response passes through the same Zod decoder boundary.
- Rust DTOs derive `schemars::JsonSchema`; generated JSON Schemas and contract fixtures are committed under `schemas/`.
- Zod schemas are authored manually. Contract tests convert Zod schemas to normalized JSON Schema and compare them semantically with Rust-generated schemas.
- TanStack Query is installed and configured in Increment 3. Basic queries, mutations, cache invalidation, and Devtools are included; polling, generations, stale handling, and coordinated refresh belong to Increment 4.
- Frontend structure is feature-sliced: `features/projects`, `features/runtime`, `ipc`, and reusable `ui` components.
- Projects uses a feature-level ErrorBoundary. Expected application errors remain local to queries/forms/actions; unexpected render failures and `protocol_mismatch` reach the feature fallback.
- Forms use multi-stage validation: immediate client validation for domain-shape rules, followed by backend draft inspection for filesystem/runtime-dependent issues.
- Profile cards show one derived status label with expandable runtime/definition/issue details, not a row of competing status badges.
- Stop and Restart are visible primary actions. Apply, Tear down, and Remove profile are in an overflow menu. Tear down and Remove require Radix AlertDialog confirmation.
- Empty Projects state explicitly asks the user to add a first Compose project. It does not promise discovery.
- Runtime connection starts disconnected at the backend, while the frontend calls `connect_runtime` automatically on first render. Failure does not hide profiles.
- Lifecycle commands receive only `{ profileId }`; frontend never sends paths, environment files, Compose names, or working directories.

## 3. Architecture and ownership

```text
React feature components
        |
TanStack Query hooks
        |
src/ipc/commands.ts
        |
src/ipc/dispatch.ts ---- browser mock
        |
Tauri invoke
        |
colui-tauri DTO commands
        |
colui-app use cases + RuntimeGateway
        |
registry and verified Docker session
```

`colui-tauri` owns command registration, application-state wiring, DTO definitions, and DTO mapping. It contains no domain rules and does not call Bollard, filesystem, or process APIs directly. Commands call existing application use cases and runtime gateway methods only.

`src/ipc/` owns transport invocation, request construction, runtime response decoding, and transport-error normalization. No other frontend file may import Tauri or call `invoke`.

React owns view state, dialog state, form draft state, and expanded-card state. Backend owns profile identity/revisions, paths, runtime state, inventory projections, operation locks, authoritative status, and error classification. UI never performs optimistic mutation of authoritative projections.

## 4. Rust DTO contract

DTOs use `Serialize`, `Deserialize`, `JsonSchema`, and `#[serde(rename_all = "camelCase")]`. Field optionality and enum representations are explicit and must match TypeScript schemas. DTOs are transport types; domain newtypes are converted to strings or scalar values at this boundary.

### 4.1 Profile DTOs

```rust
#[serde(rename_all = "camelCase")]
pub struct ProfileSummaryDto {
    pub id: String,
    pub revision: u64,
    pub display_name: String,
    pub compose_project_name: String,
    pub working_directory: String,
    pub registration_origin: RegistrationOriginDto,
}

#[serde(rename_all = "camelCase")]
pub struct ProfileDetailsDto {
    pub profile: ProfileSummaryDto,
    pub compose_files: Vec<String>,
    pub environment_files: Vec<String>,
}

#[serde(rename_all = "camelCase")]
pub struct ProfileDraftDto {
    pub display_name: String,
    pub compose_project_name: String,
    pub working_directory: String,
    pub compose_files: Vec<String>,
    pub environment_files: Vec<String>,
}

#[serde(rename_all = "camelCase")]
pub struct ProfilePatchDto {
    pub display_name: String,
    pub compose_project_name: String,
    pub working_directory: String,
    pub compose_files: Vec<String>,
    pub environment_files: Vec<String>,
}

#[serde(rename_all = "camelCase")]
pub struct UpdateProfileRequestDto {
    pub profile_id: String,
    pub expected_revision: u64,
    pub patch: ProfilePatchDto,
}

#[serde(rename_all = "camelCase")]
pub struct RemoveProfileRequestDto {
    pub profile_id: String,
    pub expected_revision: u64,
}

#[serde(rename_all = "camelCase")]
pub enum RegistrationOriginDto { Manual, Discovered, Migrated }
```

`ProfilePatchDto` contains editable fields only. `UpdateProfileRequestDto` and `RemoveProfileRequestDto` wrap the immutable `profileId` and `expectedRevision` metadata around the patch or removal request. The patch cannot contain an ID or revision. Backend validation preserves invalid profiles as visible registry entries. Each DTO explicitly declares `#[serde(rename_all = "camelCase")]`.

### 4.2 Runtime and status DTOs

```rust
#[serde(rename_all = "camelCase", tag = "state")]
pub enum RuntimeStateDto {
    Disconnected,
    Connecting,
    Ready { context: SessionContextDto },
    ContextMismatch { details: MismatchDetailsDto },
    Failed { error: AppErrorDto },
}

#[serde(rename_all = "camelCase")]
pub struct ProjectStatusDto {
    pub profile_id: String,
    pub runtime: RuntimeProjectionDto,
    pub definition: DefinitionProjectionDto,
    pub operation: Option<OperationDto>,
    pub issues: Vec<IssueDto>,
}

#[serde(rename_all = "camelCase")]
pub struct RuntimeProjectionDto {
    pub presence: RuntimePresenceDto,
    pub activity: Option<RuntimeActivityDto>,
    pub container_count: u32,
    pub running_container_count: u32,
    pub observed_at: Option<String>,
}

#[serde(rename_all = "camelCase")]
pub struct DefinitionProjectionDto {
    pub state: DefinitionStateDto,
    pub revision: Option<String>,
    pub service_count: Option<u32>,
}

#[serde(rename_all = "camelCase")]
pub struct OperationDto {
    pub kind: OperationKindDto,
    pub phase: OperationPhaseDto,
    pub started_at: String,
}

#[serde(rename_all = "camelCase")]
pub struct LifecycleResultDto {
    pub profile_id: String,
    pub success: bool,
}

#[serde(rename_all = "camelCase")]
pub struct ProfileValidationDto {
    pub valid: bool,
    pub issues: Vec<IssueDto>,
}
```

`RuntimePresenceDto` is `unavailable | absent | present`; activity is `all-running | mixed | none-running`; definition state is `unchecked | valid | invalid | stale`; operation kind is `apply | stop | tear-down | restart`. Timestamps are RFC 3339 strings. `LifecycleResultDto` contains operation success and `profileId`, but no inventory generation. After success, only the matching `projectKeys.status(profileId)` query is invalidated exactly once. Generation-bearing lifecycle responses and generation acceptance belong to Increment 4.

### 4.3 Errors

```rust
pub struct AppErrorDto {
    pub code: AppErrorCodeDto,
    pub operation: String,
    pub subject_id: Option<String>,
    pub message: String,
    pub details: Option<String>,
    pub retryable: bool,
}
```

`AppErrorCodeDto` serializes all stable domain/application codes, including `runtime_unavailable`, `runtime_connection_failed`, `runtime_context_mismatch`, `profile_not_found`, `profile_already_registered`, `profile_revision_conflict`, `profile_invalid`, `definition_failed`, `compose_failed`, `container_operation_failed`, `operation_conflict`, `operation_timeout`, `registry_corrupt`, `registry_locked`, `registry_write_failed`, `permission_denied`, and `protocol_mismatch`. Tauri commands return `Result<T, AppErrorDto>`, never `Result<T, String>`.

## 5. Tauri command surface

Commands decode input, call one existing use case/gateway operation, map the result, and serialize errors. No command accepts path-bearing lifecycle input.

| Command | Input | Result | Durable write |
|---|---|---|---|
| `get_runtime_state` | none | `RuntimeStateDto` | no |
| `connect_runtime` | none | `RuntimeStateDto` | no |
| `list_profiles` | none | `Vec<ProfileSummaryDto>` | no |
| `get_profile` | `{ profileId }` | `ProfileDetailsDto` | no |
| `inspect_profile_draft` | `ProfileDraftDto` | `ProfileValidationDto` | no |
| `create_profile` | `ProfileDraftDto` | `ProfileSummaryDto` | yes |
| `update_profile` | `{ profileId, expectedRevision, patch }` | `ProfileSummaryDto` | yes |
| `remove_profile` | `{ profileId, expectedRevision }` | `()` | yes, registry only |
| `get_project_status` | `{ profileId }` | `ProjectStatusDto` | no |
| `apply_project` | `{ profileId }` | `LifecycleResultDto` | no |
| `stop_project` | `{ profileId }` | `LifecycleResultDto` | no |
| `tear_down_project` | `{ profileId }` | `LifecycleResultDto` | no |
| `restart_project` | `{ profileId }` | `LifecycleResultDto` | no |

`create_profile` and `update_profile` perform domain validation offline and persist valid drafts. Filesystem/definition inspection is surfaced separately and must not make an invalid path disappear. Increment 2 provides the `ComposeRunner`, `RuntimeGateway`, and Compose argument construction, but not lifecycle use-case objects. Increment 3 adds `ApplyProject`, `StopProject`, `TearDownProject`, and `RestartProject` in `colui-app`, each using the existing ports and shared backend-resolved profile lookup, with adapters only wiring the one existing Compose path. No compatibility endpoint or second lifecycle implementation is added. Lifecycle commands resolve the current profile and paths in backend state, verify `RuntimeGateway` readiness, then invoke the corresponding use case. `ContextMismatch` blocks Compose operations but does not block profile list/detail or registry CRUD.

## 6. TypeScript IPC boundary

```text
src/ipc/
  commands.ts       high-level typed API used by features
  dispatch.ts       Tauri/mock selection and raw transport errors
  schemas.ts        manually authored Zod response/request schemas
  types.ts          z.infer exports
  validation.ts     one response decoder helper
  errors.ts         AppError and protocol_mismatch normalization
  mock-backend.ts   in-memory implementation of raw command protocol
```

`commands.ts` is the only API imported by feature code. Each response command calls `dispatch`, then `decodeResponse(schema, raw, operation)`. `decodeResponse` uses `safeParse`; failure always throws a typed `AppError` with code `protocol_mismatch`, operation, non-sensitive validation details, and `retryable: false`. A malformed success payload never becomes `[]`, `null`, or an inferred fallback.

Transport errors are normalized into `AppError` without pretending they are protocol errors. Tauri error payloads are parsed as `AppErrorDto` when valid; otherwise they become an operation-specific typed transport error with original details bounded for display.

`dispatch` selects `mockBackend` when `VITE_MOCK_IPC=true`; otherwise it uses Tauri `invoke`. The selection is internal and stable. No component, hook, or test imports `@tauri-apps/api` directly. ESLint `no-restricted-imports` and a repository boundary script reject violations.

Zod schemas cover every response DTO and every request DTO that crosses the boundary. `types.ts` exports `z.infer` types, so feature code does not duplicate transport interfaces. UUID, revision ranges, enum values, timestamps, and non-empty strings are checked at runtime.

## 7. Schema and fixture workflow

Rust DTO schemas are generated by a deterministic test or dedicated schema command into committed paths such as:

```text
schemas/
  ProfileSummaryDto.json
  ProfileDetailsDto.json
  ProfileDraftDto.json
  ProfileValidationDto.json
  RuntimeStateDto.json
  ProjectStatusDto.json
  LifecycleResultDto.json
  AppErrorDto.json
  fixtures/
    profile_summary_valid.json
    profile_summary_invalid_missing_id.json
    project_status_runtime_unavailable.json
    app_error_context_mismatch.json
```

The generation command is run in CI, then `git diff --exit-code schemas/` terminates with a non-zero status when committed schemas are stale. Contract tests convert each manual Zod schema with `zod-to-json-schema`, remove non-semantic metadata, normalize `$ref`/definitions, `additionalProperties`, required ordering, and property ordering, then compare against the Rust schema. The comparison is semantic, not raw JSON text comparison.

Rust-generated positive and negative fixtures are checked by TypeScript Vitest contract tests. Positive fixtures must decode. Negative fixtures must fail for the intended structural reason. The fixture suite includes missing required fields, invalid enum values, invalid UUIDs, malformed timestamps, and invalid project status combinations where schema-level validation can express them. Domain semantic validation remains backend responsibility.

## 8. Browser mock protocol

`mock-backend.ts` implements the raw command names and argument shapes from Section 5. It stores profiles in memory, assigns UUIDs, applies revisions, preserves file order, models runtime connection, returns single-shot status, and records lifecycle calls for tests. It must reject unknown commands and malformed command arguments with the same typed error shape rather than silently accepting them.

Mock state reset is an explicit public test method, not mutation of private fields. Mock fixtures include disconnected, ready, unavailable, context-mismatch, invalid-profile, running, stopped, and operation-pending projections. Mock lifecycle calls update its status projection sufficiently for UI tests, but this is a protocol fake, not a second implementation of application lifecycle semantics.

All mock responses call the same `commands.ts` decoders as Tauri responses. Tests therefore verify both normal UI behavior and the `protocol_mismatch` path by injecting malformed raw responses.

## 9. TanStack Query boundaries

`QueryClientProvider` wraps the application. Default policy:

- queries retry only retryable `AppError`s, with at most two retries;
- mutations do not retry automatically;
- window-focus refetch and polling are disabled in Increment 3;
- no optimistic updates are used;
- single-shot status queries are manually invalidated after lifecycle success;
- profile list/detail queries invalidate after create/update/remove;
- runtime state is set from successful connect and only the matching status query is invalidated once.

Query keys use immutable IDs:

```ts
projectKeys.list()
projectKeys.detail(profileId)
projectKeys.status(profileId)
runtimeKeys.state()
```

`useProfiles`, `useProfile`, `useProjectStatus`, `useRuntimeState`, `useConnectRuntime`, profile CRUD mutations, and four lifecycle mutations wrap `src/ipc/commands.ts`. Hooks do not construct payloads with paths for lifecycle actions. Increment 4 may add coordinator-backed queries and generation acceptance without changing component ownership.

Auto-connect is guarded against duplicate mutation calls: initializer tracks whether it has attempted connection and invokes `connect_runtime` once on mount. A failed connection is rendered as runtime-unavailable state; it does not prevent profile query completion.

## 10. React UI structure

```text
src/
  app/
    App.tsx
    query-client.ts
    routes.tsx
  features/
    projects/
      ProjectsView.tsx
      components/
        ProfileList.tsx
        ProfileCard.tsx
        ProfileFormDialog.tsx
        StatusBadge.tsx
        StatusDetails.tsx
        ActionMenu.tsx
        EmptyState.tsx
      hooks/
        useProfiles.ts
        useProfile.ts
        useProjectStatus.ts
        useProfileMutations.ts
        useLifecycleActions.ts
      query-keys.ts
    runtime/
      RuntimeInitializer.tsx
      hooks/useRuntimeSession.ts
  ipc/
  ui/
    components/
      Button.tsx
      Card.tsx
      Input.tsx
      Alert.tsx
      Dialog.tsx
      AlertDialog.tsx
      DropdownMenu.tsx
      ErrorBoundary.tsx
```

### 10.1 Projects view

Projects loads profiles independently from runtime. Loading shows skeletons. Empty state shows `No projects yet`, explanatory manual-registration text, and `Add Project`. Non-empty state renders cards keyed by `profile.id`, never Compose name or display name.

Profile card displays display name, Compose namespace, working directory, one derived status label, and action controls. Status details expand through a real disclosure button with `aria-expanded` and `aria-controls`; details include runtime presence/counts, definition state/service count, observed timestamp, and issues. Invalid profiles remain in the list with configuration attention status.

Status precedence is explicit: active operation, runtime unavailable, invalid definition, valid runtime activity, then unchecked/unknown fallback. The label is presentation-only and does not replace the independent DTO axes.

### 10.2 Actions and confirmations

Stop and Restart are visible primary buttons. They are disabled when profile status is unavailable, an operation is pending, or the action is semantically unavailable. Apply, Tear down, and Remove profile are in an accessible overflow menu. Every pending state is keyed by immutable `profileId`; one card's pending state cannot disable or replace another card's state.

Tear down uses Radix `AlertDialog` and explains that containers and networks are removed while the profile remains. Remove profile uses a separate `AlertDialog` and explains that Docker is not touched. Dialogs provide labelled title/description, focus trap, initial focus on Cancel, keyboard Tab traversal, Escape cancellation, and explicit destructive confirmation. Confirmation closes only after the mutation has been accepted for execution; mutation errors remain visible in the originating card/dialog surface.

### 10.3 Profile forms

Create and edit use the same field model: display name, Compose project name, working directory, ordered Compose files, and ordered environment files. File lists use add/remove/reorder controls and preserve order. Text entry is an acceptable browser/mock fallback, but the submitted DTO always contains ordered arrays.

Immediate client validation checks required values, Compose namespace regex, non-empty Compose file list, and duplicate paths. Each error is associated with its input through `aria-invalid` and `aria-describedby`. Submit then calls `inspect_profile_draft`; backend/domain errors and filesystem issues are rendered inline. A valid domain draft can be saved offline; definition validation remains deferred according to approved runtime behavior.

Edit submits `profileId`, `expectedRevision`, and patch. It never permits editing identity. Revision conflict keeps dialog values visible and asks the user to reload/reconcile rather than overwriting newer data.

### 10.4 Error boundary and typed error presentation

`ProjectsView` is wrapped by a feature-level ErrorBoundary. It presents a retry action and distinguishes `protocol_mismatch` from generic unexpected failures without exposing unbounded diagnostics. Expected errors are local:

- `operation_conflict`: action-level busy message;
- `runtime_unavailable` or `runtime_context_mismatch`: disabled Compose actions plus recovery message;
- `profile_revision_conflict`: form remains open with reload/reconcile instruction;
- `profile_invalid` and filesystem issues: field/status-level errors;
- `profile_not_found`: card/query-level not-found message;
- retryable runtime/registry errors: retry action.

No app-wide Diagnostics view is added in this increment.

## 11. Accessibility and responsive behavior

The shell is usable at narrow desktop/window widths and mobile-sized browser widths used by Vite development. Cards collapse to one column below the layout breakpoint; controls remain reachable without horizontal scrolling. Focus-visible styles are present for all interactive elements. Icon-only overflow buttons have accessible names. Dialogs return focus to their trigger after close. Form errors and operation changes use appropriate live-region announcements without duplicating every visual label. Color is never the only status signal.

Keyboard acceptance includes navigation to Add Project, form submission with Enter, Cancel/Escape behavior, disclosure toggling, overflow menu navigation, confirmation with explicit focused action, and no focus loss after async mutation errors.

## 12. Testing strategy

### Rust tests

- DTO serialization uses camelCase and matches committed schemas.
- Each command maps application success and every relevant typed error without domain logic in the command.
- Lifecycle request DTOs contain only `profileId`.
- Profile list/query commands do not mutate registry.
- Remove profile does not call Docker.
- Stop, Tear down, Apply, and Restart delegate to distinct Increment 3 application use cases built on Increment 2's existing runtime ports and Compose argument construction.
- Schema generation is deterministic.

### TypeScript contract and IPC tests

- Every Rust schema has matching Zod semantic schema.
- Positive and negative fixtures pass/fail as expected.
- Missing/invalid response fields throw `protocol_mismatch`, never fallback values.
- Transport AppErrors retain stable code and operation.
- Direct Tauri imports outside `src/ipc/dispatch.ts` fail lint/boundary checks.
- Mock supports list, CRUD, runtime, status, lifecycle protocol, reset, and malformed-response injection.

### Component tests

Vitest and Testing Library cover empty/list/loading/error states, profile create/edit validation, ordered files, revision conflict, derived status labels, expandable details, ID-keyed card rendering, action enabled/disabled rules, pending isolation, typed error presentation, and both destructive confirmations. Tests query by role/label and verify `aria-expanded`, `aria-describedby`, focus return, Escape cancellation, and keyboard submit.

### Integration smoke test

Browser mock smoke test creates a profile, sees it in the list, edits it, invokes lifecycle actions by ID, opens and confirms Tear down, opens and cancels Remove, then removes the profile and returns to the explicit empty state. A second test verifies profiles render while auto-connect returns runtime unavailable. Real Docker smoke tests remain the Increment 2/feature-gated application tests; Increment 3 adds no new Docker fixture semantics.

## 13. Verification commands

The implementation plan must make these commands pass:

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

Schema generation runs before `git diff --exit-code schemas/`; that command terminates with a non-zero status when generated and committed schemas differ. The canonical package scripts are `typecheck`, `lint`, `test`, `test:contracts`, and `build`, plus `scripts/check-boundaries.sh`; the implementation plan must add or preserve them with deterministic commands and no Docker/network dependency except separately gated existing integration tests.

## 14. Acceptance checklist

- Tauri commands call existing `colui-app` use cases and `RuntimeGateway`.
- DTO mapping exists in `colui-tauri` and contains no domain rules.
- Rust DTOs derive `schemars::JsonSchema`.
- Generated JSON Schemas and positive/negative fixtures are committed under `schemas/`.
- Manual Zod schemas match Rust schemas through semantic contract tests.
- One decoder boundary maps malformed responses to `protocol_mismatch`.
- No direct frontend `invoke` exists outside `src/ipc/dispatch.ts`.
- React/Vite/TypeScript strict shell starts without Docker.
- TanStack Query foundation exists without `InventoryCoordinator`.
- Projects view lists manually registered profiles offline.
- Create and edit forms preserve Compose and environment file order.
- Apply, Stop, Tear down, and Restart use `{ profileId }` only.
- Remove profile never invokes Docker.
- Typed errors render recovery appropriate to stable error code.
- Tear down and Remove use separate accessible destructive confirmations.
- Single-shot profile status exposes independent runtime, definition, operation, and issue data.
- Browser mock implements same command protocol and passes same Zod decoders.
- Profile cards use immutable IDs for React keys and pending state.
- Responsive and keyboard/accessibility behavior is covered by tests.
- Increment 4 concerns remain excluded: coordinator, polling, generations, stale/backoff, discovery, definition cache, containers/logs/ports, and Diagnostics view.

## 15. Delivery constraints

Implementation must not create compatibility lifecycle commands, path-bearing payloads, frontend authority over backend state, hidden registry writes, or a second lifecycle implementation. Commits should remain independently reviewable: schema/DTO contract, IPC boundary/mock, query foundation, UI/forms, tests/verification. This document is design only; Increment 3 code is implemented in a later session after this spec and its implementation plan are approved.
