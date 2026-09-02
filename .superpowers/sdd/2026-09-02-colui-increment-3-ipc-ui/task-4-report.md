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
```
