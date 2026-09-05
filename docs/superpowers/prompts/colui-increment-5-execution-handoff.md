# CoLUI Increment 5 Execution Handoff

Use this file to resume Increment 5 implementation in a separate session.

## Resume instruction

```text
Resume docs/superpowers/plans/2026-09-04-colui-increment-5-discovery.md
using superpowers:subagent-driven-development.

Read this handoff first, then read this plan's SDD ledger:
.superpowers/sdd/2026-09-04-colui-increment-5-discovery/progress.md

Trust ledger and git history over conversation memory. Do not repeat Tasks 1-3.
Task 4 implementation exists at commit 370c1f0 but has not passed its
task-review gate. Resume by generating its review package from dabc30f to
370c1f0, dispatching the Task 4 reviewer, and entering the fix loop if needed.
After Task 4 is review-clean, continue Tasks 5-12 sequentially. Use a fresh
implementer and task reviewer for every task, update ledger after each review,
then run final whole-branch review and finishing-a-development-branch.
```

## Authoritative inputs

- Branch: `design/increment-5-discovery`
- Plan: `docs/superpowers/plans/2026-09-04-colui-increment-5-discovery.md`
- Spec: `docs/superpowers/specs/2026-09-04-colui-increment-5-discovery-design.md`
- Original design prompt: `docs/superpowers/prompts/colui-increment-5-design-plan.md`
- SDD workspace: `.superpowers/sdd/2026-09-04-colui-increment-5-discovery/`
- Progress ledger: `.superpowers/sdd/2026-09-04-colui-increment-5-discovery/progress.md`

Spec is binding authority. Plan argues from spec. If they conflict, follow spec
and record ruling in ledger.

## Environment note

Direct `pnpm` is unavailable in current environment. Use:

```bash
npx --yes pnpm@9.15.5 <command>
```

Do not alter global package-manager configuration.

## Baseline evidence

- `cargo test --workspace`: passed, 168 tests, 0 failures before implementation.
- `npx --yes pnpm@9.15.5 test`: passed, 87 tests, 0 failures before implementation.

## Task status

| Task | Status | Commits | Review state |
| --- | --- | --- | --- |
| 1. Compose observation groups and profile association | Complete | `f97a8f8`, `778a545` | Fix round 1; re-review clean |
| 2. Discovery classification and candidate identity | Complete | `50d53e3`, `6d843be` | Fix round 1; re-review clean |
| 3. Discovery session, ignore, journal, auto policy | Complete | `9482ab1`, `dabc30f` | Fix round 1; re-review clean |
| 4. Atomic registration and typed errors | Implemented | `370c1f0` | Reviewer not yet dispatched |
| 5. Operation locks and registry recovery | Pending | - | Not started |
| 6. Diagnostics and serialized runtime controls | Pending | - | Not started |
| 7. Standalone actions and causal refresh | Pending | - | Not started |
| 8. Bounded logs and safe port actions | Pending | - | Not started |
| 9. Tauri DTO/commands/schemas/events | Pending | - | Not started |
| 10. Frontend IPC and TanStack coherence | Pending | - | Not started |
| 11. Projects/containers/Diagnostics UI | Pending | - | Not started |
| 12. Real-Docker checks and final acceptance | Pending | - | Not started |

## Completed review history

### Task 1

- Base: `ff86293`
- Initial implementation: `f97a8f8`
- Reviewer found full-tuple cardinality, zero-match ambiguity, and production
  routing defects.
- Fix: `778a545`
- Scoped re-review: all six findings addressed; no new Critical/Important issue.
- Report: `.superpowers/sdd/2026-09-04-colui-increment-5-discovery/task-1-report.md`

### Task 2

- Base: `778a545`
- Initial implementation: `50d53e3`
- Reviewer found complete/incomplete duplicate identity classification and
  incomplete conflict-evidence aggregation defects.
- Fix: `6d843be`
- Scoped re-review: all findings addressed; no new Critical/Important issue.
- Report: `.superpowers/sdd/2026-09-04-colui-increment-5-discovery/task-2-report.md`

### Task 3

- Base: `6d843be`
- Initial implementation: `9482ab1`
- Reviewer found missing production publication subscription, missing registry
  snapshot in scheduling, stale query-derived work, and unsafe late completion.
- Fix: `dabc30f`
- Scoped re-review: all findings addressed; no new Critical/Important issue.
- Report: `.superpowers/sdd/2026-09-04-colui-increment-5-discovery/task-3-report.md`

## Current Task 4 gate

Task 4 implementer reports:

- Commit: `370c1f0 feat(app): register discovery candidates`
- Discovery tests: 13 passed.
- Profile tests: 18 passed.
- Domain tests: 34 passed.
- `cargo test --workspace`: passed with 0 failures.
- Worktree was clean after commit.

Known implementer concerns:

- Domain carries typed `subject` while legacy `subject_id` remains until Task 9
  migrates DTO/Schemars/Zod contracts.
- Production candidate lease validity relies on `InventoryCoordinator`
  publication gate introduced by Task 3.

Task 4 report:

`.superpowers/sdd/2026-09-04-colui-increment-5-discovery/task-4-report.md`

Generate review package with:

```bash
"/Users/max/.cache/opencode/packages/superpowers@git+https:/github.com/obra/superpowers.git/node_modules/superpowers/skills/subagent-driven-development/scripts/review-package" \
  "docs/superpowers/plans/2026-09-04-colui-increment-5-discovery.md" \
  "dabc30fe1b7e32b9a863c0404f308c8674570d20" \
  "370c1f0"
```

Reviewer must receive Task 4 brief, report, generated diff package, and global
constraints from plan. Require both spec-compliance and quality verdicts.

Task 4 brief:

`.superpowers/sdd/2026-09-04-colui-increment-5-discovery/task-4-brief.md`

## Execution rules

- Never dispatch multiple implementers concurrently.
- Fresh implementer per task; reviewer after every task.
- Fix rounds 1-3 resume original implementer where possible.
- Generate review package from recorded task base, never `HEAD~1`.
- Never let controller implement review fixes directly.
- Append every fix round and completion line to this plan's ledger.
- Do not read/write another plan's `.superpowers/sdd/` workspace.
- Preserve one owner for inventory, runtime, operation locks, definitions, and
  lifecycle behavior.
- Do not modify or revert unrelated concurrent user changes.
- Run task-specific tests and required broader regression commands before each
  completion claim.

## Final required flow

After Tasks 4-12 are complete:

1. Run full Rust/frontend/schema/static verification from Task 12.
2. Run opt-in Docker suite when daemon is available; record exact skip otherwise.
3. Generate whole-branch review package from branch merge base to `HEAD`.
4. Dispatch final reviewer and perform at most one final fix wave plus re-review.
5. Collect all ledger `Ruling:` lines for final report.
6. Use `superpowers:finishing-a-development-branch`.
