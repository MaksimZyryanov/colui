# CoLUI Increment 2: Design + Plan Session Prompt

В отдельной сессии подготовь спецификацию и implementation plan для
**Increment 2: RuntimeGateway**.

Прочитай сначала:

- `docs/superpowers/specs/2026-09-01-colui-2-design.md`
- `docs/superpowers/plans/2026-09-02-colui-increment-1.md`
- `CoLUI_architecture_audit_v2.md`
- текущие commits и состояние worktree

Используй `superpowers:brainstorming`.

Процесс обязателен:

1. Классифицируй задачу как architectural.
2. Проведи уточняющие вопросы по одному за сообщение.
3. Предложи 2-3 подхода с trade-offs и рекомендацией.
4. Представь дизайн секциями и получи approval каждой секции.
5. После полного approval запиши spec в:
   `docs/superpowers/specs/2026-09-02-colui-increment-2-runtime-design.md`
6. Проведи self-review spec: placeholders, противоречия, scope,
   неоднозначности.
7. Закоммить только утверждённую spec.
8. Попроси пользователя проверить spec и остановись до явного approval.
9. После approval используй `superpowers:writing-plans` и создай:
   `docs/superpowers/plans/2026-09-02-colui-increment-2-runtime.md`
10. Проведи self-review plan и передай пользователю выбор execution mode.

Scope Increment 2:

- `RuntimeGateway` и `RuntimeSession`;
- endpoint resolution для одного локального Docker context;
- Bollard `DockerApiAdapter`;
- Compose CLI adapter;
- daemon fingerprint API ↔ CLI и `ContextMismatch` gate;
- reconnect без restart приложения;
- `ComposeProcessRunner`;
- argv без shell interpolation;
- backend-resolved cwd и explicit CLI environment;
- process group termination: SIGTERM → 5s grace → SIGKILL;
- mandatory wait/reap;
- bounded stdout/stderr: independent 64 KiB byte ring buffers;
- hermetic fake-executable tests;
- real Docker tests за `docker-tests` feature на Docker Desktop и Colima.

Не включай:

- frontend или Tauri IPC;
- InventoryCoordinator;
- Compose definition cache;
- Discovery;
- lifecycle UI;
- Docker Events;
- remote/multi-context support;
- реализацию кода в этой design/plan сессии.

Сохрани решения из approved design:

- `colui-domain` не знает о Bollard, Tauri, Tokio, fs или process APIs;
- `colui-app` использует порты и не знает Bollard/Tauri;
- adapters не зависят от Tauri;
- RuntimeGateway единолично владеет API и CLI control planes;
- `Ready` только если API и CLI fingerprints совпадают;
- mismatch блокирует Compose operations, но не profile access;
- обычное окружение (`PATH`, locale, `HOME`) сохраняется, context-affecting
  Docker variables явно очищаются или переопределяются;
- process output хранится как newest bytes, старые байты ring buffer
  отбрасываются; UTF-8 декодируется после сбора с replacement characters;
- container logs относятся к Increment 5, не добавляй их сюда;
- никаких новых backward-compatibility или speculative abstractions.

Plan должен содержать точные файлы, интерфейсы, зависимости, TDD-шаги,
команды тестов, fake executable protocol, feature-gated real-Docker tests,
commit steps и acceptance checklist. Не используй `TBD`, `TODO`, vague
инструкции или «реализовать аналогично».

Не запускай `subagent-driven-development` и не реализуй Increment 2 в этой
сессии. Нужны только spec и plan; реализация будет отдельной сессией.
