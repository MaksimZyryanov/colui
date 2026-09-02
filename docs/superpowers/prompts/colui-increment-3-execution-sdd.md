# CoLUI Increment 3: Subagent-Driven Implementation

Реализуй план:
`docs/superpowers/plans/2026-09-02-colui-increment-3-ipc-ui.md`

Перед началом прочитай:

- `docs/superpowers/specs/2026-09-02-colui-increment-3-ipc-ui-design.md`;
- план Increment 3;
- планы и код Increment 1/2;
- `CoLUI_architecture_audit_v2.md`;
- `git status` и последние commits.

Используй `superpowers:subagent-driven-development`.

Правила:

- отдельный subagent на каждую задачу плана;
- после каждой задачи провести review diff, тестов и scope;
- исправить найденные проблемы до перехода к следующей задаче;
- выполнять TDD steps и verification commands из плана;
- не использовать прямой Tauri `invoke` вне `src/ipc/dispatch.ts`;
- не передавать paths/names в lifecycle payloads;
- не добавлять features Increment 4/5;
- не отменять и не перезаписывать чужие изменения;
- commits делать по структуре плана, только после успешного review;
- при конфликте спеки, плана и текущего кода остановиться и сообщить.

Финальный отчёт:

- выполненные задачи и commits;
- результаты всех verification commands;
- найденные и исправленные review issues;
- оставшиеся проблемы или skipped tests;
- `git status`.

Не создавай PR и не push без отдельного запроса.
