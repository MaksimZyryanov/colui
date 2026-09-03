# Increment 4: Subagent-Driven Implementation

Выполни Increment 4 по плану:

`docs/superpowers/plans/2026-09-03-colui-increment-4-inventory.md`

Сначала прочитай план и связанную спецификацию:

`docs/superpowers/specs/2026-09-03-colui-increment-4-inventory-design.md`

Работай в этой сессии как **Subagent-Driven Development**:

- используй `subagent-driven-development` skill;
- на каждую задачу плана запускай отдельный свежий subagent;
- subagent сначала пишет failing test, затем реализацию, запускает focused tests и делает commit только своих файлов;
- после каждого subagent выполняй два review: сначала проверка требований/тестов, затем code review diff;
- исправляй замечания отдельными follow-up subagent или inline только после фиксации причины;
- переходи к следующей задаче только после успешных focused tests и review;
- не смешивай задачи и не делай крупный общий commit;
- не трогай пользовательские изменения и не включай `docs/superpowers/prompts/*` в commits;
- Docker-dependent smoke запускай только отдельно и только если Docker доступен;
- перед финалом выполни полный verification из Task 10 и проверь `git status`, `git diff --check`, commits и acceptance checklist.

При конфликте между планом и фактическим кодом остановись, зафиксируй конфликт, проверь spec и исправь план/решение до продолжения. Не расширяй scope за пределы Increment 4.

В конце сообщи: какие задачи завершены, какие commits созданы, результаты verification, оставшиеся риски и незавершённые пункты.
