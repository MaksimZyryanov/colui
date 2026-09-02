# CoLUI Increment 1: SDD Session Prompt

Выполни implementation plan:

`docs/superpowers/plans/2026-09-02-colui-increment-1.md`

Используй `superpowers:subagent-driven-development`.

Правила:

- Работай в изолированном worktree.
- Перед началом прочитай plan и spec:
  `docs/superpowers/specs/2026-09-01-colui-2-design.md`.
- Для каждой задачи dispatch отдельный fresh implementer subagent.
- После каждой задачи запускай отдельный task reviewer.
- При findings используй fix loop и scoped re-review.
- Веди progress ledger согласно skill.
- Не пропускай TDD, тесты, commits и final whole-branch review.
- Не запускай несколько implementer subagents параллельно.
- Не меняй scope Increment 1: domain, profile use cases, registry v2,
  atomic persistence, lock/retry, corruption safety и v1 import.
- Не трогай `.DS_Store` и чужие изменения.

После завершения сообщи:

- commit range;
- результаты тестов и final review;
- deferred или parked findings;
- workspace cleanup status.
