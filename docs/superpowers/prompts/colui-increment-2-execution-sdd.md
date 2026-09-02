# CoLUI Increment 2: Execute via Subagent-Driven Development

Выполни implementation plan:

`docs/superpowers/plans/2026-09-02-colui-increment-2-runtime.md`

Сначала прочитай plan и его spec:

`docs/superpowers/specs/2026-09-02-colui-increment-2-runtime-design.md`

Обязательный процесс:

1. Используй `superpowers:subagent-driven-development`.
2. Для каждой задачи dispatch отдельный fresh implementer subagent.
3. После каждой задачи проведи task review на spec compliance и code quality.
4. Findings исправляй через implementer и выполняй scoped re-review.
5. В конце проведи broad whole-branch review.
6. Перед завершением используй `superpowers:finishing-a-development-branch`.
7. Выполняй TDD-шаги и команды в plan буквально.
8. Реализацию выполняй только в isolated worktree; не работай напрямую в `main`.
9. Не добавляй frontend, Tauri IPC, InventoryCoordinator, definition cache,
   Discovery, lifecycle UI, Docker Events, container logs, remote/multi-context
   support или speculative abstractions.
10. Не пропускай hermetic tests, boundary checks или feature-gated real-Docker
    tests. Docker Desktop и Colima проверяй отдельно, если доступны.

Работай до полного завершения plan. При blocker, security-sensitive action,
irreversible operation или side effect вне worktree остановись и сообщи причину.
