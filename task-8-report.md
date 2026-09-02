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
