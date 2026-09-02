# Task 8 Report

## Required Commands

`pnpm test -- src/features/projects/components/__tests__/ActionMenu.test.tsx`

```text
zsh:1: command not found: pnpm
```

`pnpm typecheck`

```text
zsh:1: command not found: pnpm
```

## Equivalent Verification

`npm exec vitest run src/features/projects/components/__tests__/ActionMenu.test.tsx`

```text
Test Files  1 passed (1)
Tests  3 passed (3)
```

`npm exec vitest run src/features/projects`

```text
Test Files  3 passed (3)
Tests  17 passed (17)
```

`npm exec tsc -- --noEmit`

```text
No output; exit 0.
```

## Scope

- Added primary Stop/Restart controls.
- Added overflow Apply, Tear down, and Remove profile controls.
- Added separate accessible AlertDialogs with explicit confirmation.
- Kept lifecycle payloads to `{ profileId }` and remove payload to `{ profileId, expectedRevision }`.
- Isolated mutation pending state within each mounted profile card.
- Kept card and dialog context visible after typed mutation errors.

## Task 8 Review Fixes

- Lifecycle pending state now participates in every same-card Compose action predicate.
- Action controls render from an unavailable fallback projection after status failure; Compose actions disable while registry-only Remove remains available.
- Tear down and Remove use separate controlled `AlertDialog.Root` instances.
- Destructive dialogs focus Cancel initially and return focus to More actions after Escape or accepted close.
- Rejected mutations keep dialog open and render typed error copy; accepted mutations close dialog.
- Lifecycle payload remains `{ profileId }`; Remove payload remains `{ profileId, expectedRevision }`.

## Review Fix Verification

Exact commands and outputs:

```text
$ npm test -- src/features/projects/components/__tests__/ActionMenu.test.tsx
Test Files  1 passed (1)
Tests  9 passed (9)

$ npm test -- src/features/projects
Test Files  3 passed (3)
Tests  23 passed (23)

$ npm test
Test Files  8 passed (8)
Tests  58 passed (58)

$ npm run typecheck
tsc --noEmit: passed

$ npm run lint
tsc --noEmit: passed

$ git diff --check
passed
```

## Review Fix Concerns

- `pnpm` remains unavailable; equivalent `npm` commands used.
- Full frontend suite emits expected jsdom stderr for intentional error-boundary and unnamed icon-only button tests.
- Live Docker lifecycle matrix not rerun; local daemon availability remains outside this UI/DTO fix.
