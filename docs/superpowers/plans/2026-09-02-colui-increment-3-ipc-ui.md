# CoLUI Increment 3 IPC and UI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Expose existing profile/runtime capabilities through typed Tauri IPC and deliver an offline-capable Projects UI for manual profiles and ID-based lifecycle actions.

**Architecture:** `colui-tauri` maps application results to transport DTOs and owns Tauri command registration; it contains no domain rules or direct Bollard/process calls. React feature code uses only `src/ipc/commands.ts`, whose single decoder boundary validates every response with manual Zod schemas. TanStack Query provides basic cache/query/mutation state, while single-shot status remains separate from Increment 4's coordinator and generation model.

**Tech Stack:** Rust stable, Tauri 2, `schemars`, `serde`, React 18, TypeScript strict, Vite, TanStack Query, Radix primitives, Zod, Vitest, Testing Library, pnpm.

**Spec:** `docs/superpowers/specs/2026-09-02-colui-increment-3-ipc-ui-design.md`

## Global Constraints

- Lifecycle commands accept only `{ profileId }`; frontend never sends paths, environment files, or Compose names.
- Increment 2 provides `RuntimeGateway`, `ComposeRunner`, and Compose argument construction; Increment 3 adds exactly one set of application lifecycle use cases: `ApplyProject`, `StopProject`, `TearDownProject`, and `RestartProject`.
- `Stop` invokes `compose stop`; `Tear down` invokes `compose down`; `Remove profile` changes registry only and never invokes Docker.
- Queries and status reads never mutate the durable registry.
- Invalid profiles remain visible.
- `ContextMismatch` blocks Compose operations but permits profile access and registry CRUD.
- `colui-tauri` contains DTO mapping only; no domain rules, Bollard calls, filesystem calls, or process calls.
- All Tauri command failures use `AppErrorDto`; no `Result<_, String>`.
- Rust DTOs derive `Serialize`, `Deserialize`, and `JsonSchema`; committed schemas live under `schemas/`.
- Zod schemas are manual and every response crosses one `protocol_mismatch` decoder boundary.
- Direct Tauri `invoke` imports are forbidden outside `src/ipc/dispatch.ts`.
- Browser mock implements the same raw command protocol and uses the same command decoders.
- TanStack Query has no polling, generation guards, stale/backoff logic, or `InventoryCoordinator` in this increment.
- React owns only view/dialog/form/expanded-card state; backend projections remain authoritative.
- React keys and pending state use immutable profile IDs.
- Tear down and Remove profile require separate accessible Radix AlertDialogs.
- App starts backend-disconnected; frontend auto-connects once on first render without blocking profile access.
- UI is responsive, keyboard usable, and macOS-only as a product target.
- No discovery, standalone containers, logs, ports, Images item, Diagnostics view, remote contexts, or compatibility lifecycle endpoints.

---

## File Map

Create or modify files as follows. Do not create `src-tauri` compatibility wrappers for the old application; this workspace currently has no frontend or Tauri crate.

- Modify `Cargo.toml`: add `src-tauri` workspace member only if Tauri's generated crate is kept in workspace; preserve existing three crate members.
- Create `src-tauri/Cargo.toml`: Tauri 2 crate, `colui-app`, `colui-adapters`, `colui-domain`, `serde`, `schemars`, `thiserror`/mapping dependencies, and `tauri-build`.
- Create `src-tauri/build.rs`: Tauri build metadata.
- Create `src-tauri/src/lib.rs`: `AppState`, dependency wiring, command registration entry.
- Create `src-tauri/src/main.rs`: desktop entry calling the library builder.
- Create `src-tauri/src/dto/{mod.rs,profile.rs,runtime.rs,status.rs,error.rs,request.rs}`: transport types and mapping.
- Create `src-tauri/src/commands/{mod.rs,profiles.rs,runtime.rs,lifecycle.rs}`: thin Tauri commands.
- Create `src-tauri/tests/dto_contracts.rs`: deterministic schema and fixture generation checks.
- Create `crates/colui-app/src/lifecycle.rs`: four lifecycle use cases and shared operation contract.
- Modify `crates/colui-app/src/lib.rs`: export lifecycle API.
- Create `crates/colui-app/tests/lifecycle.rs`: fake reader/gateway/runner tests.
- Modify `crates/colui-adapters/src/lib.rs` and `crates/colui-adapters/src/runtime/mod.rs` to expose the concrete gateway operation entry point required by `LifecycleRuntime`; do not add lifecycle policy or a second operation path.
- Create `schemas/*.json` and `schemas/fixtures/*.json`: committed generated DTO schemas and fixtures.
- Create `package.json`, `tsconfig.json`, `vite.config.ts`, `vitest.config.ts`, `index.html`: strict Vite shell and scripts.
- Create `src/ipc/{commands.ts,dispatch.ts,schemas.ts,types.ts,validation.ts,errors.ts,mock-backend.ts}` and `src/ipc/__tests__/*`.
- Create `src/app/{App.tsx,query-client.ts,routes.tsx}`.
- Create `src/features/runtime/{RuntimeInitializer.tsx,hooks/useRuntimeSession.ts,query-keys.ts}`.
- Create `src/features/projects/{ProjectsView.tsx,query-keys.ts,hooks/*,components/*}`.
- Create `src/ui/components/*`: Button, Card, Input, Alert, Dialog, AlertDialog, DropdownMenu, ErrorBoundary, and status/accessibility primitives.
- Create `scripts/check-boundaries.sh` and `scripts/verify-increment-3.sh`.

---

### Task 1: Add Application Lifecycle Use Cases

**Files:**
- Create: `crates/colui-app/src/lifecycle.rs`
- Modify: `crates/colui-app/src/lib.rs`
- Test: `crates/colui-app/tests/lifecycle.rs`

**Interfaces:**
- Consumes: `ProfileReader`, `RuntimeSessionState`, `ProfileId`, `ProjectProfile`, and `AppError`. Keep adapter `ComposeOperation` out of `colui-app`; define an application operation enum and map it in the adapter implementation.
- Produces: `LifecycleOperation`, `LifecycleRuntime`, `ApplyProject`, `StopProject`, `TearDownProject`, `RestartProject` with `new(reader, runtime)` and `execute(profile_id) -> LifecycleFuture<'_, LifecycleResult>`; `LifecycleRuntime::run_profile(profile, operation) -> LifecycleFuture<'_, LifecycleResult>`; `LifecycleResult { profile_id: ProfileId, success: bool }`.

- [ ] **Step 1: Write failing tests for backend lookup and operation mapping**

```rust
#[tokio::test]
async fn apply_looks_up_profile_and_uses_up_without_caller_paths() {
    let reader = FakeReader::with_profile(profile("id-1"));
    let runtime = FakeRuntime::ready();
    let result = ApplyProject::new(&reader, &runtime).execute(profile_id("id-1")).await.unwrap();
    assert_eq!(result.profile_id, profile_id("id-1"));
    assert_eq!(runtime.invocations()[0].operation, LifecycleOperation::Apply);
}

#[tokio::test]
async fn stop_and_tear_down_are_distinct() {
    let reader = FakeReader::with_profile(profile("id-1"));
    let runtime = FakeRuntime::ready();
    StopProject::new(&reader, &runtime).execute(profile_id("id-1")).await.unwrap();
    TearDownProject::new(&reader, &runtime).execute(profile_id("id-1")).await.unwrap();
    assert_eq!(runtime.operations(), vec![LifecycleOperation::Stop, LifecycleOperation::TearDown]);
}

#[tokio::test]
async fn mismatch_blocks_lifecycle_before_runner_call() {
    let reader = FakeReader::with_profile(profile("id-1"));
    let runtime = FakeRuntime::context_mismatch();
    let error = ApplyProject::new(&reader, &runtime).execute(profile_id("id-1")).await.unwrap_err();
    assert_eq!(error.code, AppErrorCode::RuntimeContextMismatch);
}
```

- [ ] **Step 2: Run focused tests and verify failure**

Run: `cargo test -p colui-app --test lifecycle`

Expected: FAIL because lifecycle types and use cases do not exist.

- [ ] **Step 3: Implement one shared lifecycle execution path**

Define `type LifecycleFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, AppError>> + Send + 'a>>` and this application-only port:

```rust
pub trait LifecycleRuntime: Send + Sync {
    fn run_profile(
        &self,
        profile: ProjectProfile,
        operation: LifecycleOperation,
    ) -> LifecycleFuture<'_, LifecycleResult>;
}
```

The use-case implementation loads the profile through `ProfileReader`, returns `profile_not_found` before runtime access when the ID is absent, then calls `LifecycleRuntime::run_profile(profile, operation)`. The adapter implementation checks `RuntimeSessionState::Ready`, builds and invokes the profile's Compose command, maps non-zero/timeout results to `compose_failed`/`operation_timeout`, and returns `LifecycleResult`. `colui-app` defines `LifecycleOperation`; only the adapter implementation maps it to `ComposeOperation::{Up, Stop, Down, Restart}`. Implement the four public use-case wrappers by passing only `LifecycleOperation::{Apply, Stop, TearDown, Restart}`. Do not accept paths, names, or prebuilt commands from callers.

- [ ] **Step 4: Run tests and boundary check**

Run: `cargo test -p colui-app --test lifecycle && cargo test --workspace && bash scripts/check-boundaries.sh`

Expected: PASS; app has no Bollard/Tauri dependency and each operation maps to its distinct Compose verb.

- [ ] **Step 5: Commit**

```bash
git add crates/colui-app
git commit -m "feat(app): add profile lifecycle use cases"
```

### Task 2: Scaffold Tauri Crate and DTO Modules

**Files:**
- Create: `src-tauri/Cargo.toml`, `src-tauri/build.rs`, `src-tauri/src/{main.rs,lib.rs}`
- Create: `src-tauri/src/dto/{mod.rs,profile.rs,runtime.rs,status.rs,error.rs,request.rs}`
- Create: `src-tauri/src/commands/{mod.rs,profiles.rs,runtime.rs,lifecycle.rs}`
- Modify: `Cargo.toml`
- Test: `src-tauri/tests/dto_contracts.rs`

**Interfaces:**
- Consumes: Task 1 use cases, Increment 1 profile/error types, Increment 2 `RuntimeGateway`/runtime contracts.
- Produces: `ProfileSummaryDto`, `ProfileDetailsDto`, `ProfileDraftDto`, `ProfilePatchDto`, `UpdateProfileRequestDto`, `RemoveProfileRequestDto`, `RuntimeStateDto`, `ProjectStatusDto`, `LifecycleResultDto`, `ProfileValidationDto`, `AppErrorDto`, and `AppState`.

- [ ] **Step 1: Write failing DTO serialization tests**

```rust
#[test]
fn update_request_serializes_only_camel_case_metadata_and_patch() {
    let request = UpdateProfileRequestDto::fixture();
    let value = serde_json::to_value(request).unwrap();
    assert_eq!(value["profileId"], "00000000-0000-0000-0000-000000000001");
    assert_eq!(value["expectedRevision"], 3);
    assert!(value["patch"].get("id").is_none());
}

#[test]
fn lifecycle_result_contains_profile_id_and_success() {
    let value = serde_json::to_value(LifecycleResultDto::success("id-1")).unwrap();
    assert_eq!(value, serde_json::json!({"profileId":"id-1","success":true}));
}
```

- [ ] **Step 2: Run test and verify failure**

Run: `cargo test -p colui-tauri --test dto_contracts`

Expected: FAIL because the crate and DTOs do not exist.

- [ ] **Step 3: Implement DTOs with explicit serde attributes**

Use `#[serde(rename_all = "camelCase")]` on every struct and enum. Use externally tagged or explicitly tagged representations consistently: `RuntimeStateDto` is `#[serde(tag = "state", rename_all = "camelCase")]` with `disconnected`, `connecting`, `ready`, `contextMismatch`, and `failed`. Convert domain IDs/newtypes to strings, timestamps to RFC 3339 strings, and domain errors to `AppErrorDto`. Define exact request wrappers: update has `profileId`, `expectedRevision`, `patch`; remove has `profileId`, `expectedRevision`; lifecycle has `profileId` only. Define `ProfileValidationDto { valid: bool, issues: Vec<IssueDto> }` and `LifecycleResultDto { profile_id: String, success: bool }`.

- [ ] **Step 4: Wire AppState and thin command modules**

Create `AppState` holding `Arc<ProfileReader/ProfileStore>` and the shared `Arc<RuntimeGateway>` or application façade. Register commands through Tauri's `generate_handler!`. Keep command bodies limited to input conversion, use-case call, DTO mapping, and typed error mapping. Do not implement Compose logic in commands.

- [ ] **Step 5: Run DTO tests and compile**

Run: `cargo test -p colui-tauri --test dto_contracts && cargo check -p colui-tauri`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add Cargo.toml src-tauri
git commit -m "feat(tauri): add typed ipc dto boundary"
```

### Task 3: Generate Committed Schemas and Rust Command Contracts

**Files:**
- Modify: `src-tauri/src/dto/*.rs`, `src-tauri/tests/dto_contracts.rs`
- Create: `schemas/*.json`, `schemas/fixtures/*.json`
- Create: `scripts/generate-schemas.sh`

**Interfaces:**
- Consumes: DTOs from Task 2.
- Produces: deterministic schema generation and positive/negative fixture files consumed by TypeScript contract tests.

- [ ] **Step 1: Add schema generation test**

```rust
#[test]
fn generated_schema_files_are_deterministic() {
    let generated = generate_all_schemas();
    assert_eq!(generated.get("LifecycleResultDto").unwrap()["properties"]["profileId"]["type"], "string");
    write_schemas_only_when_running_explicit_generator(&generated);
}
```

- [ ] **Step 2: Run generator and verify files are produced**

Run: `bash scripts/generate-schemas.sh`

Expected: one JSON file for each DTO in Section 7 of the spec plus fixtures under `schemas/fixtures/`.

- [ ] **Step 3: Implement deterministic generation**

Use `schemars::schema_for!` for every public DTO. Serialize stable pretty JSON, generate valid fixtures for profile summary, runtime unavailable, context mismatch, project status, lifecycle result, and profile validation, and generate negative fixtures for missing ID, invalid enum, malformed timestamp, and missing required fields. The generator must write only `schemas/`, never registry data.

- [ ] **Step 4: Add stale-schema verification**

Run generator in a temporary comparison mode, then execute `git diff --exit-code schemas/`; this command must terminate non-zero when committed schemas differ from generated schemas. Add the check to `scripts/verify-increment-3.sh`.

- [ ] **Step 5: Run Rust schema tests**

Run: `cargo test -p colui-tauri --test dto_contracts && bash scripts/generate-schemas.sh && git diff --exit-code schemas/`

Expected: PASS with no schema diff.

- [ ] **Step 6: Commit**

```bash
git add src-tauri schemas scripts/generate-schemas.sh
git commit -m "test(tauri): commit ipc json schemas"
```

### Task 4: Build TypeScript IPC Boundary and Browser Mock

**Files:**
- Create: `package.json`, `tsconfig.json`, `vite.config.ts`, `vitest.config.ts`, `index.html`
- Create: `src/ipc/{commands.ts,dispatch.ts,schemas.ts,types.ts,validation.ts,errors.ts,mock-backend.ts}`
- Test: `src/ipc/__tests__/{contracts.test.ts,commands.test.ts,mock-backend.test.ts}`
- Modify: `scripts/check-boundaries.sh`

**Interfaces:**
- Consumes: committed schemas/fixtures and raw Tauri command names from Task 2.
- Produces: `listProfiles()`, `getProfile(profileId)`, `inspectProfileDraft(draft)`, `createProfile(draft)`, `updateProfile(request)`, `removeProfile(request)`, `getRuntimeState()`, `connectRuntime()`, `getProjectStatus(profileId)`, and four lifecycle functions accepting only `profileId`.

- [ ] **Step 1: Write failing decoder and protocol tests**

```ts
it('turns malformed success payload into protocol_mismatch', async () => {
  const raw = { revision: 1, displayName: 'Missing id' };
  expect(() => decodeResponse(profileSummarySchema, raw, 'list_profiles'))
    .toThrowError(expect.objectContaining({ code: 'protocol_mismatch' }));
});

it('rejects lifecycle arguments containing backend authority', () => {
  expect(() => applyProject({ profileId: 'id-1', workingDirectory: '/tmp' } as never))
    .toThrow();
});
```

- [ ] **Step 2: Run focused tests and verify failure**

Run: `pnpm test -- src/ipc/__tests__`

Expected: FAIL because the Vite package and IPC modules do not exist.

- [ ] **Step 3: Implement Zod schemas and inferred types**

Define schemas matching committed Rust DTOs, including UUIDs, positive revisions, camelCase fields, RFC 3339 timestamps, tagged runtime state, enum values, `LifecycleResultDto`, and `ProfileValidationDto`. Export all types through `z.infer`; do not duplicate interfaces manually.

- [ ] **Step 4: Implement errors, dispatch, and one decoder boundary**

Define `AppError` with stable code, operation, optional subject ID, message, details, and retryable. `decodeResponse<T>(schema, raw, operation): T` uses `safeParse` and throws `protocol_mismatch` with bounded validation details. `dispatch(command, args)` selects the mock when `VITE_MOCK_IPC === 'true'`, otherwise imports Tauri 2 `invoke` from `@tauri-apps/api/core`; only `dispatch.ts` may import it. Normalize valid backend errors and bound unknown transport details.

- [ ] **Step 5: Implement typed command wrappers**

Each response wrapper dispatches then decodes. Void lifecycle/remove responses still validate the expected `undefined`/unit transport convention. Lifecycle wrappers have signatures `applyProject(profileId: string)`, `stopProject(profileId: string)`, `tearDownProject(profileId: string)`, and `restartProject(profileId: string)`, and pass `{ profileId }` only.

- [ ] **Step 6: Implement mock protocol and reset/injection API**

Expose `mockBackend.reset()` and `mockBackend.setResponseOverride(command, value)`. Implement list/CRUD/revision checks, runtime auto-connect, single-shot status fixtures, lifecycle recording/status updates, unknown-command errors, and malformed-response injection. Do not mutate private fields from tests.

- [ ] **Step 7: Run IPC and contract tests**

Run: `pnpm test -- src/ipc/__tests__ && pnpm test:contracts`

Expected: PASS; Rust/Zod schemas match semantically, fixtures pass/fail correctly, and malformed responses never become fallback empty results.

- [ ] **Step 8: Commit**

```bash
git add package.json tsconfig.json vite.config.ts vitest.config.ts index.html src/ipc scripts/check-boundaries.sh
git commit -m "feat(ipc): add typed decoder and browser mock"
```

### Task 5: Add TanStack Query and Runtime Initialization

**Files:**
- Create: `src/app/{App.tsx,query-client.ts,routes.tsx}`
- Create: `src/features/runtime/{RuntimeInitializer.tsx,query-keys.ts,hooks/useRuntimeSession.ts}`
- Test: `src/features/runtime/__tests__/RuntimeInitializer.test.tsx`
- Modify: `package.json`

**Interfaces:**
- Consumes: typed IPC functions from Task 4.
- Produces: `QueryClientProvider`, `useRuntimeState`, `useConnectRuntime`, and once-only auto-connect behavior.

- [ ] **Step 1: Write failing query and auto-connect tests**

```tsx
it('connects once on first render without blocking profiles', async () => {
  render(<TestApp />);
  await waitFor(() => expect(mockBackend.calls('connect_runtime')).toHaveLength(1));
  expect(screen.getByRole('main')).toBeVisible();
});

it('does not retry non-retryable protocol mismatch', async () => {
  mockBackend.setResponseOverride('get_runtime_state', { state: 'bad' });
  render(<TestApp />);
  await screen.findByText(/response format mismatch/i);
  expect(mockBackend.calls('get_runtime_state')).toHaveLength(1);
});
```

- [ ] **Step 2: Run tests and verify failure**

Run: `pnpm test -- src/features/runtime/__tests__/RuntimeInitializer.test.tsx`

Expected: FAIL because QueryClient and runtime hooks do not exist.

- [ ] **Step 3: Implement QueryClient defaults**

Configure at most two retries only for retryable `AppError`, no mutation retries, no polling, and no window-focus refetch. Export stable `runtimeKeys.state()` and project keys in the feature modules.

- [ ] **Step 4: Implement runtime hooks and initializer**

Wrap IPC functions with `useQuery`/`useMutation`. On mount, initializer performs one guarded `connectRuntime()` call; connection failure remains query/mutation state and does not prevent Projects route rendering. Successful connect sets runtime cache and invalidates only the matching status query when a profile action later succeeds.

- [ ] **Step 5: Run tests and typecheck**

Run: `pnpm test -- src/features/runtime/__tests__/RuntimeInitializer.test.tsx && pnpm typecheck`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add package.json src/app src/features/runtime
git commit -m "feat(ui): add query and runtime foundation"
```

### Task 6: Add Shared Accessible UI Primitives and Error Boundary

**Files:**
- Create: `src/ui/components/{Button,Card,Input,Alert,Dialog,AlertDialog,DropdownMenu,ErrorBoundary}.tsx`
- Create: `src/ui/styles.css`
- Modify: `src/main.tsx`, `src/app/App.tsx`
- Test: `src/ui/components/__tests__/*.test.tsx`

**Interfaces:**
- Consumes: React and Radix packages.
- Produces: typed primitives with keyboard/focus behavior and feature-level ErrorBoundary fallback that distinguishes `protocol_mismatch`.

- [ ] **Step 1: Write failing accessibility tests**

```tsx
it('returns focus to AlertDialog trigger after Escape', async () => {
  const user = userEvent.setup();
  render(<ConfirmationFixture />);
  const trigger = screen.getByRole('button', { name: /remove profile/i });
  await user.click(trigger);
  await user.keyboard('{Escape}');
  expect(trigger).toHaveFocus();
});

it('shows protocol mismatch recovery in boundary fallback', () => {
  render(<ErrorBoundary><ThrowProtocolMismatch /></ErrorBoundary>);
  expect(screen.getByText(/response format mismatch/i)).toBeVisible();
  expect(screen.getByRole('button', { name: /retry/i })).toBeVisible();
});
```

- [ ] **Step 2: Run tests and verify failure**

Run: `pnpm test -- src/ui/components/__tests__`

Expected: FAIL because primitives do not exist.

- [ ] **Step 3: Implement Radix-backed primitives**

Use Radix Dialog/AlertDialog/DropdownMenu primitives. Set labelled title/description, initial focus to Cancel for destructive dialogs, focus return to trigger, Escape cancellation, visible focus rings, icon-only accessible names, and destructive styling that is not color-only.

- [ ] **Step 4: Implement feature ErrorBoundary**

Use a class boundary or `react-error-boundary`. Render bounded details, a retry/reset button, and explicit protocol mismatch copy. Do not add a root Diagnostics surface.

- [ ] **Step 5: Run tests and commit**

Run: `pnpm test -- src/ui/components/__tests__ && pnpm lint`

Expected: PASS.

```bash
git add src/ui src/main.tsx src/app/App.tsx
git commit -m "feat(ui): add accessible primitives"
```

### Task 7: Implement Projects Queries, Forms, List, and Status Projection

**Files:**
- Create: `src/features/projects/{ProjectsView.tsx,query-keys.ts}`
- Create: `src/features/projects/hooks/{useProfiles,useProfile,useProjectStatus,useProfileMutations,useLifecycleActions}.ts`
- Create: `src/features/projects/components/{ProfileList,ProfileCard,ProfileFormDialog,StatusBadge,StatusDetails,ActionMenu,EmptyState}.tsx`
- Test: `src/features/projects/**/__tests__/*.test.tsx`

**Interfaces:**
- Consumes: Task 4 IPC functions, Task 5 QueryClient, Task 6 UI primitives.
- Produces: Projects route and hooks for profiles/status/lifecycle, all keyed by `profile.id`.

- [ ] **Step 1: Write failing view tests**

```tsx
it('renders explicit empty state and opens Add Project form', async () => {
  const user = userEvent.setup();
  renderProjects();
  expect(await screen.findByText('No projects yet')).toBeVisible();
  await user.click(screen.getByRole('button', { name: /add project/i }));
  expect(screen.getByRole('dialog', { name: /add project/i })).toBeVisible();
});

it('renders running as one label and expands independent details', async () => {
  mockBackend.seedRunningProfile('id-1');
  const user = userEvent.setup();
  renderProjects();
  expect(await screen.findByText('Running')).toBeVisible();
  expect(screen.queryByText(/runtime:/i)).not.toBeInTheDocument();
  await user.click(screen.getByRole('button', { name: /show status details/i }));
  expect(screen.getByText(/3\/3 running/i)).toBeVisible();
});
```

- [ ] **Step 2: Run focused tests and verify failure**

Run: `pnpm test -- src/features/projects`

Expected: FAIL because Projects feature files do not exist.

- [ ] **Step 3: Implement query keys and hooks**

Use `projectKeys.list()`, `projectKeys.detail(profileId)`, and `projectKeys.status(profileId)`. Profile create invalidates `projectKeys.list()`. Profile update invalidates `projectKeys.list()` and `projectKeys.detail(updated.id)`. Profile removal invalidates `projectKeys.list()` and removes its detail/status cache entries. Lifecycle mutations accept `string profileId`, invalidate only `projectKeys.status(profileId)` exactly once after successful `LifecycleResultDto`, and never optimistically mutate projections.

- [ ] **Step 4: Implement ProjectsView and list**

Render loading skeletons, explicit empty state, and responsive one-column-to-grid cards. Use `profile.id` as React key. Wrap ProjectsView in feature ErrorBoundary. Keep profile list query independent of runtime query so offline startup works.

- [ ] **Step 5: Implement status projection**

Derive one label with explicit precedence: active operation, runtime unavailable, invalid definition, valid runtime activity, then unchecked/unknown. Expand details through a disclosure button with `aria-expanded` and `aria-controls`; show counts, definition state/service count, timestamp, and issues. Keep all DTO axes intact.

- [ ] **Step 6: Implement multi-stage profile form**

Create/edit fields are display name, Compose name, working directory, ordered Compose files, and ordered environment files. Immediate validation covers required values, lowercase namespace regex, at least one Compose file, and duplicate paths. Submit calls `inspectProfileDraft`; backend issues render inline with `aria-invalid`/`aria-describedby`. Save valid domain drafts offline. Edit keeps `profileId` and `expectedRevision` outside the patch and leaves values visible on revision conflict.

- [ ] **Step 7: Run feature tests and typecheck**

Run: `pnpm test -- src/features/projects && pnpm typecheck && pnpm lint`

Expected: PASS; no direct Tauri import or path-bearing lifecycle payload.

- [ ] **Step 8: Commit**

```bash
git add src/features/projects
git commit -m "feat(projects): add profile list and forms"
```

### Task 8: Add Lifecycle Actions and Destructive Confirmations

**Files:**
- Modify: `src/features/projects/components/ActionMenu.tsx`, `ProfileCard.tsx`
- Modify: `src/features/projects/hooks/useLifecycleActions.ts`, `useProfileMutations.ts`
- Test: `src/features/projects/components/__tests__/ActionMenu.test.tsx`

**Interfaces:**
- Consumes: typed lifecycle hooks and Radix primitives.
- Produces: visible Stop/Restart controls, overflow Apply/Tear down/Remove controls, separate confirmations, and isolated pending state by profile ID.

- [ ] **Step 1: Write failing action tests**

```tsx
it('sends only profileId for lifecycle actions', async () => {
  const user = userEvent.setup();
  renderRunningCard('id-1');
  await user.click(screen.getByRole('button', { name: /stop/i }));
  expect(mockBackend.lastCall('stop_project')).toEqual({ profileId: 'id-1' });
});

it('requires explicit confirmation and retains profile after tear down', async () => {
  const user = userEvent.setup();
  renderRunningCard('id-1');
  await user.click(screen.getByRole('button', { name: /more actions/i }));
  await user.click(screen.getByRole('menuitem', { name: /tear down/i }));
  expect(screen.getByRole('alertdialog', { name: /tear down project/i })).toBeVisible();
  await user.click(screen.getByRole('button', { name: /^tear down$/i }));
  await waitFor(() => expect(screen.getByText('Test Project')).toBeVisible());
});
```

- [ ] **Step 2: Run tests and verify failure**

Run: `pnpm test -- src/features/projects/components/__tests__/ActionMenu.test.tsx`

Expected: FAIL until action placement and dialogs are implemented.

- [ ] **Step 3: Implement split primary/overflow actions**

Show Stop and Restart as primary buttons. Put Apply, Tear down, and Remove profile in overflow. Disable actions for unavailable runtime, unavailable semantic state, active operation, and matching mutation pending state. Keep pending state keyed by `profile.id`; do not disable unrelated cards.

- [ ] **Step 4: Implement confirmation semantics**

Tear down dialog says containers/networks are removed and profile remains. Remove dialog says Docker is untouched. Cancel/Escape aborts without IPC. Confirm invokes only the corresponding ID/revision payload. Close after mutation acceptance; show typed mutation error without losing form/card context.

- [ ] **Step 5: Run tests and commit**

Run: `pnpm test -- src/features/projects/components/__tests__/ActionMenu.test.tsx && pnpm typecheck`

Expected: PASS.

```bash
git add src/features/projects
git commit -m "feat(projects): add lifecycle actions"
```

### Task 9: Add Contract, Accessibility, Boundary, and Browser Smoke Gates

**Files:**
- Create/modify: `src/ipc/__tests__/contracts.test.ts`, `src/features/**/__tests__`, `src/ui/**/__tests__`
- Create: `scripts/check-boundaries.sh`, `scripts/verify-increment-3.sh`
- Modify: `package.json`, `vitest.config.ts`

**Interfaces:**
- Consumes: all Increment 3 modules.
- Produces: deterministic local verification for Rust/TypeScript drift, direct invoke prohibition, UI behavior, and browser mock flow.

- [ ] **Step 1: Add semantic schema normalization test**

Normalize `$schema`, title/description, `$ref`/definitions, `additionalProperties`, required ordering, and property ordering before comparing `zod-to-json-schema` output with Rust schemas. Assert every DTO has one comparison and every positive/negative fixture is exercised.

- [ ] **Step 2: Add complete frontend behavior coverage**

Cover loading, offline list, empty state, create/edit, ordered files, client/server validation, revision conflict, status precedence, expandable details, pending isolation, typed errors, protocol mismatch fallback, keyboard form submission, Escape cancellation, focus return, and all four lifecycle verbs.

- [ ] **Step 3: Add boundary checks**

Make `scripts/check-boundaries.sh` fail on direct `@tauri-apps/api/core` or legacy Tauri imports outside `src/ipc/dispatch.ts`, lifecycle payload source fields other than `profileId`, frontend React keys based on Compose/display names, `Result<_, String>` in `src-tauri`, and forbidden `colui-domain`/`colui-app` dependencies.

- [ ] **Step 4: Add browser mock smoke test**

Run Vite mock mode and verify: empty state, create profile, edit profile, lifecycle call recording, Tear down confirmation, Remove cancellation, Remove confirmation, return to empty state, and profiles visible when runtime connection fails.

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
bash scripts/verify-increment-3.sh
git diff --check
```

Expected: all commands exit 0; schema check reports no diff; no Docker/network is required.

- [ ] **Step 6: Commit**

```bash
git add package.json src scripts
git commit -m "test: verify increment three ipc ui"
```

### Task 10: Final Increment 3 Review and Acceptance

**Files:**
- Modify only files required by failing final checks; do not broaden scope.
- Review: all files from Tasks 1-9 and `docs/superpowers/specs/2026-09-02-colui-increment-3-ipc-ui-design.md`

**Interfaces:**
- Consumes: complete implementation and verification output.
- Produces: accepted Increment 3 branch with no uncommitted generated schema drift or unrelated files.

- [ ] **Step 1: Run full acceptance commands again**

```bash
cargo fmt --all -- --check
cargo test --workspace
pnpm lint
pnpm typecheck
pnpm test
pnpm test:contracts
pnpm build
bash scripts/check-boundaries.sh
git diff --check
```

- [ ] **Step 2: Inspect scope and forbidden behavior**

Confirm lifecycle requests contain `{ profileId }` only; backend resolves paths; Remove profile makes no runtime call; Stop and Tear down remain distinct; invalid profiles remain listed; runtime failure does not prevent profile CRUD; only matching status query invalidates after lifecycle success; mock and Tauri use identical decoders; no polling/coordinator/discovery/Diagnostics/logs/ports/Images code was added.

- [ ] **Step 3: Review commit set**

Run `git status --short`, `git diff --stat`, and `git log --oneline -10`. Stage only intended Increment 3 files. Do not commit the untracked source prompt unless explicitly requested.

- [ ] **Step 4: Commit any final targeted fixes separately**

```bash
git add crates/colui-app crates/colui-adapters src-tauri src schemas scripts package.json tsconfig.json vite.config.ts vitest.config.ts index.html
git commit -m "fix: close increment three acceptance gaps"
```

## Acceptance Checklist

- [ ] Tauri commands call `colui-app` use cases and shared `RuntimeGateway`.
- [ ] `colui-tauri` DTO mapping contains no domain rules or direct runtime APIs.
- [ ] All DTOs derive `schemars::JsonSchema` and committed schemas are current.
- [ ] `LifecycleResultDto` and `ProfileValidationDto` are defined and decoded.
- [ ] Update/remove request wrappers contain ID/revision metadata outside editable patch.
- [ ] Every lifecycle request contains only `{ profileId }`.
- [ ] `protocol_mismatch` is thrown for every malformed response.
- [ ] Direct Tauri invoke outside `src/ipc/dispatch.ts` is rejected.
- [ ] Strict React/Vite shell opens without Docker and profiles remain accessible offline.
- [ ] TanStack Query foundation exists without polling or `InventoryCoordinator`.
- [ ] Projects list, empty state, create/edit forms, ordered files, and typed errors work.
- [ ] Status displays one derived label with expandable independent details.
- [ ] Stop/Restart are primary; Apply/Tear down/Remove are overflow actions.
- [ ] Tear down uses `compose down` and retains profile.
- [ ] Remove profile never calls Docker.
- [ ] Radix confirmations support focus trap, Escape, keyboard confirmation, and focus return.
- [ ] Mock implements same command protocol and decoder path as Tauri.
- [ ] IDs, not names or paths, drive React keys and pending state.
- [ ] Rust, TypeScript, component, accessibility, mock, and smoke tests pass.
- [ ] No Increment 4/5 features or second lifecycle implementation were introduced.
