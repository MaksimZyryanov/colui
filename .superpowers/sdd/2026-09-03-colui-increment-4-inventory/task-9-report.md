# Task 9 Report

## RED/GREEN

- RED: `npx pnpm@9.15.5 test -- src/features/runtime/__tests__/useInventory.test.tsx src/features/projects/__tests__/projects.test.ts` failed because `useInventory` and `shouldPublishLifecycleInventory` did not exist.
- GREEN: focused runtime/projects tests pass, including generation reference preservation, pre-observation acceptance, and lifecycle publisher behavior.

## Files

- Added `src/features/runtime/hooks/useInventory.ts` and its focused tests.
- Added `inventoryKeys.snapshot()` and `projectKeys.definition(profileId)`.
- Updated query defaults, project status derivation, lifecycle cache publication, profile definition invalidation, profile removal eviction, and the status UI integration.
- Updated project tests for inventory-backed status.
- No Rust, domain, or Tauri transport files changed.

## Query Semantics

- One shared `refresh_inventory` query uses 3000 ms visible polling, pauses while hidden, and refetches on visibility return/reconnect.
- `inventoryStructuralSharing` returns prior data for equal/lower published generations and accepts the first `hasSnapshot` response from generation zero.
- Lifecycle success publishes returned inventory through the same guard; it does not invalidate/refetch or call `refresh_inventory`.
- Runtime status derives from inventory; definition state comes from cached `get_project_details` data keyed by immutable profile ID.

## Verification

- `npx pnpm@9.15.5 test -- src/features/runtime src/features/projects`: 6 files, 37 tests passed.
- `npx pnpm@9.15.5 test:contracts`: 3 files, 33 tests passed.
- `npx pnpm@9.15.5 typecheck`: passed.
- `npx pnpm@9.15.5 lint`: passed.
- `npx pnpm@9.15.5 build`: passed.
- `bash scripts/check-boundaries.sh`: passed.

## Concerns

- Existing `get_project_details` command supplies definition projection; no new transport contract was required.

## Fix Round 1

- RED: focused tests failed because generation-zero unavailable replaced generation 4, and missing Compose project mapped to `present`/`none-running` instead of `absent`/`null`.
- Fixed structural sharing to retain any published snapshot when an equal/lower generation arrives, including pre-observation generation zero.
- Hoisted active inventory ownership to `ProjectsView`; profile cards consume shared observer data and create only disabled cache observers. Multi-profile pages therefore have one poll and one visibility listener/refetch owner.
- Missing inventory project now maps to absent; pre-observation remains unavailable. Definition errors remain separate while retained runtime projection stays visible.
- Profile removal no longer evicts global inventory data. Lifecycle mutation tests exercise equal and newer inventory publication and prove no extra refresh, invalidation, or refetch.
- GREEN evidence: focused review tests passed (20 tests); full frontend verification and exact final outputs recorded below after final gate.
- Concern: standalone test/demo cards without shared inventory retain compatibility through a local owner unless supplied `initialStatus`; production project lists always use the hoisted owner.
