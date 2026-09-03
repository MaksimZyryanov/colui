# Runtime Query Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Add TanStack Query app composition and non-blocking runtime initialization over Task 4 IPC.

**Architecture:** One configured `QueryClient` owns query and mutation policy. Runtime feature exports stable keys, query/mutation hooks, and an initializer that attempts connection once while routes render independently.

**Tech Stack:** React 18, TanStack Query, Vitest, Testing Library.

**Spec:** `.superpowers/sdd/2026-09-02-colui-increment-3-ipc-ui/task-5-brief.md`

## Global Constraints

- Retry only retryable `AppError`, at most two retries.
- Disable mutation retries, polling, and window-focus refetch.
- Auto-connect once and never block Projects route rendering.
- Consume typed IPC functions from Task 4.

### Task 1: Runtime behavior tests

**Files:**
- Create: `src/features/runtime/__tests__/RuntimeInitializer.test.tsx`

- [ ] Write tests for one guarded connect, visible main during pending connection, and non-retryable protocol mismatch.
- [ ] Run `pnpm test -- src/features/runtime/__tests__/RuntimeInitializer.test.tsx` and capture expected missing-module failure.

### Task 2: Query/runtime implementation

**Files:**
- Create: `src/app/query-client.ts`, `src/app/routes.tsx`, `src/app/App.tsx`
- Create: `src/features/runtime/RuntimeInitializer.tsx`, `src/features/runtime/query-keys.ts`, `src/features/runtime/hooks/useRuntimeSession.ts`
- Modify: `package.json`

- [ ] Add QueryClient defaults and runtime keys/hooks.
- [ ] Add once-only initializer and independent Projects route.
- [ ] Run focused tests and typecheck.

### Task 3: Report and commit

**Files:**
- Create: `.superpowers/sdd/2026-09-02-colui-increment-3-ipc-ui/task-5-report.md`

- [ ] Record exact command outputs and concerns.
- [ ] Run `git add package.json src/app src/features/runtime` and commit `feat(ui): add query and runtime foundation`.
