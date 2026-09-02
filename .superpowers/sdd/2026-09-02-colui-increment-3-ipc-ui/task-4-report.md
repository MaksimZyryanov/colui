# Task 4 Report

## Status

Implemented typed TypeScript IPC boundary and browser mock. Required commit: `feat(ipc): add typed decoder and browser mock`.

## Changes

- Added strict Vite/Vitest TypeScript package configuration.
- Added manual Zod schemas and `z.infer` transport types for profiles, runtime, status, lifecycle, requests, and errors.
- Added single `decodeResponse` protocol boundary with bounded validation paths and `protocol_mismatch` errors.
- Isolated Tauri 2 `invoke` dynamic import in `src/ipc/dispatch.ts`.
- Added typed command wrappers, including profileId-only lifecycle signatures.
- Added normalized transport/application errors.
- Added browser mock reset/override API, list/CRUD/revision behavior, runtime connection, status, lifecycle transitions, unknown-command rejection.
- Extended boundary script to reject direct Tauri imports outside dispatch module.
- Added IPC tests for decoder, lifecycle authority, CRUD revisions, and malformed injection.

## Verification

Environment did not provide `pnpm`:

```text
$ pnpm test -- src/ipc/__tests__
```

Equivalent npm commands were run after `npm install`:

```text
$ npm test
Test Files  3 passed (3)
Tests  6 passed (6)

$ npm run typecheck
tsc --noEmit
exit 0

$ npm run build
vite v5.4.21 building for production...
dist/index.html  0.06 kB | gzip: 0.07 kB
exit 0

$ npm run test:contracts
Test Files  3 passed (3)
Tests  6 passed (6)

$ bash scripts/check-boundaries.sh
Dependency boundaries OK

$ git diff --check
exit 0
```

The first boundary-script run exposed its own overly broad `invoke(` matcher because dispatch is intentionally allowed. Matcher was narrowed to direct Tauri package imports; fresh rerun passed.

## TDD Evidence

- Tests were authored before the remaining production modules.
- Focused `pnpm` red run was attempted and blocked by missing executable.
- npm run after initial implementation exposed four failures: real Tauri selection in test mode, lifecycle fixture setup, and error normalization effects.
- Fixes were applied, then full npm suite passed with 6/6 tests.

## Self-review

- Tauri dependency appears only in `src/ipc/dispatch.ts`; boundary script checks this.
- Lifecycle wrappers construct `{ profileId }` only and reject non-string direct calls.
- Decoding occurs after dispatch for every non-unit response; unit response validates undefined/null/empty object convention.
- Mock state is reset through public API, not private test mutation.
- Revision conflicts and unknown commands return stable typed errors.

## Concerns

- `pnpm` unavailable, so exact requested package-manager commands could not execute.
- Contract tests currently exercise representative schemas and fixtures rather than semantic conversion of every committed Rust JSON schema.
- Mock uses one deterministic UUID sequence and simplified runtime/lifecycle semantics; it is suitable as protocol fake, not application parity.
- `npm install` reports 5 dependency vulnerabilities (3 moderate, 1 high, 1 critical); dependency remediation is outside Task 4.
- No React UI or query foundation was added; those belong to later Task 5/6 work.

## Review Fixes

- Aligned Zod AppError optional fields, Rust-compatible empty strings, and nonnegative u64 revisions.
- Added request validation before dispatch for real/mock parity, retaining one response decoder boundary and profileId-only lifecycle calls.
- Added mock domain draft validation, duplicate compose-project-name checks on create/update, full draft persistence, lifecycle operation recording, and typed unknown-command errors.
- Parsed serialized Tauri AppError payloads, preserved optional fields, bounded Error/object transport details, and expanded protocol mismatch details with paths/messages.
- Expanded DTO, fixture, command-shape, mock parity, error, and lifecycle tests.
- Added `packageManager: "pnpm@9.15.5"`; local pnpm executable remains unavailable, so npm remains reproducible fallback through committed `package-lock.json`.

## Review Fix Verification

```text
$ npm test -- src/ipc/__tests__
Test Files  3 passed (3)
Tests  16 passed (16)

$ npm run test:contracts
Test Files  3 passed (3)
Tests  16 passed (16)

$ npm run typecheck
tsc --noEmit
exit 0

$ npm run build
vite v5.4.21 building for production...
dist/index.html  0.06 kB | gzip: 0.07 kB
exit 0

$ bash scripts/check-boundaries.sh --self-test
Boundary parser self-test OK
Dependency boundaries OK

$ git diff --check
exit 0

$ pnpm --version
zsh: command not found: pnpm

## Scoped Re-review Verification

```text
$ npm test -- src/ipc/__tests__
Test Files  3 passed (3)
Tests  23 passed (23)

$ npm run test:contracts
Test Files  3 passed (3)
Tests  23 passed (23)

$ npm run typecheck
tsc --noEmit
exit 0

$ npm run build
vite v5.4.21 building for production...
dist/index.html  0.06 kB | gzip: 0.07 kB
exit 0

$ bash scripts/check-boundaries.sh --self-test
Boundary parser self-test OK
Dependency boundaries OK

$ git diff --check
exit 0
```

Scoped fixes: lifecycle wrappers validate `profileIdRequestSchema`; mock validates Rust display/project-name semantics; DTO tests cover committed positive/negative fixtures, invalid timestamp/status combination, context shape, requests, and lifecycle result; command tests cover exact wrapper command names, argument shapes, and unit response convention.

## Re-review Fixes

- Removed mock duplicate `composeProjectName` rejection from create/update. Rust application registry permits duplicate compose names; mock now matches that behavior.
- Canonicalized profile ID contract to lowercase hyphenated UUIDs. Rust `ProfileId::parse` rejects compact and uppercase forms; Rust profile/request DTO deserializers enforce same rule at IPC boundary; TS Zod wire schemas enforce same rule while retaining `uuid` format validation.
- Added Rust domain and DTO boundary tests plus TS contract coverage for canonical, compact, and uppercase UUID forms.

## Re-review Verification

```text
$ npm test -- src/ipc/__tests__
Test Files  3 passed (3)
Tests  23 passed (23)

$ npm run typecheck
tsc --noEmit

$ npm run build
vite v5.4.21 building for production...
dist/index.html  0.06 kB | gzip: 0.07 kB
exit 0

$ npm run test:contracts
Test Files  3 passed (3)
Tests  23 passed (23)

$ bash scripts/check-boundaries.sh --self-test
Boundary parser self-test OK
Dependency boundaries OK

$ cargo test --workspace
test result: ok. 27 passed; 0 failed
test result: ok. 4 passed; 0 failed
test result: ok. 17 passed; 0 failed
test result: ok. 7 passed; 0 failed
test result: ok. 1 passed; 0 failed
test result: ok. 15 passed; 0 failed
test result: ok. 7 passed; 0 failed
test result: ok. 11 passed; 0 failed

$ git diff --check
exit 0
```

## Re-review Concerns

- `pnpm` remains unavailable; verification used committed npm lockfile and npm scripts.
- Existing generated JSON schemas express `format: uuid`, but JSON Schema format alone does not encode lowercase/hyphenated canonicality. Runtime Rust/TS validation now enforces it.

## Task 4 Round 4 Fixes

- Added canonical UUID deserialization to `SessionContextDto.session_id`, `ProjectStatusDto.profile_id`, and `LifecycleResultDto.profile_id` using existing `deserialize_canonical_uuid` validator.
- Preserved existing `schemars(schema_with = "super::uuid_schema")` annotations and TypeScript schemas.
- Added DTO boundary tests covering valid canonical UUIDs and rejection of compact and uppercase UUIDs for all three fields.

## Round 4 Verification

### RED

Command:

```bash
cargo test -p colui-tauri --test dto_contracts
```

Output before DTO fix:

```text
running 14 tests
test result: FAILED. 11 passed; 3 failed; 0 ignored; 0 measured; 0 filtered out
failures:
    lifecycle_result_deserializes_only_canonical_profile_ids
    project_status_deserializes_only_canonical_profile_ids
    session_context_deserializes_only_canonical_session_ids
```

### GREEN

Command:

```bash
cargo fmt --all && cargo test -p colui-tauri --test dto_contracts
```

Output:

```text
running 14 tests
test result: ok. 14 passed; 0 failed; 0 ignored; 0 measured; 0 filtered out
```

### Schema Stability

Command:

```bash
bash scripts/generate-schemas.sh && git diff --exit-code schemas/
```

Output: none; exit 0. No generated schema changes required.

### Workspace Verification

Command:

```bash
cargo test --workspace
```

Output: all workspace test suites passed; 0 failed.

### Final Checks

Command:

```bash
cargo fmt --all -- --check && git diff --check
```

Output: none; exit 0.
