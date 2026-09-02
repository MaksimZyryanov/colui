# CoLUI Increment 3: Design + Plan Session Prompt

В отдельной сессии подготовь спецификацию и implementation plan для
**Increment 3: Typed IPC + minimum usable UI**.

Прочитай сначала:

- `docs/superpowers/specs/2026-09-01-colui-2-design.md`
- `docs/superpowers/plans/2026-09-02-colui-increment-1.md`
- текущую spec/plan Increment 2, если они уже созданы;
- `CoLUI_architecture_audit_v2.md`;
- commits и состояние worktree.

Используй `superpowers:brainstorming`.

## Обязательный процесс

1. Классифицируй задачу как architectural.
2. Задавай уточняющие вопросы по одному за сообщение.
3. Предложи 2-3 подхода с trade-offs и рекомендацией.
4. Представь дизайн секциями и получи approval каждой секции.
5. После полного approval запиши spec:
   `docs/superpowers/specs/2026-09-02-colui-increment-3-ipc-ui-design.md`
6. Проведи self-review spec: placeholders, противоречия, scope,
   неоднозначности.
7. Закоммить только spec.
8. Попроси пользователя проверить spec и остановись до явного approval.
9. После approval используй `superpowers:writing-plans`.
10. Создай plan:
    `docs/superpowers/plans/2026-09-02-colui-increment-3-ipc-ui.md`
11. Проведи self-review plan и предложи execution mode.

## Scope Increment 3

- Tauri IPC commands поверх уже готовых `colui-app` use cases и
  `RuntimeGateway`;
- DTO mapping в `colui-tauri`, без domain rules;
- Rust DTO schemas через `schemars`;
- committed JSON schemas в `schemas/`;
- TypeScript Zod schemas и единый decoder boundary;
- `protocol_mismatch` при невалидном IPC response;
- запрет прямого `invoke` вне `src/ipc/`;
- React/Vite/TypeScript strict shell;
- TanStack Query foundation без полноценного InventoryCoordinator;
- Projects view для manually registered profiles;
- список профилей и profile create/edit forms;
- lifecycle actions по `profileId`: Apply, Stop, Tear down, Restart;
- typed error presentation;
- confirmation dialogs для Tear down и Remove profile;
- single-shot inventory status для профиля;
- Browser mock, использующий тот же IPC interface и Zod decoders;
- responsive macOS desktop UI с keyboard/accessibility behavior.

## Не включай

- InventoryCoordinator, polling generations, stale/backoff;
- Compose definition cache и `docker compose config` refresh policy;
- Discovery candidates, conflicts и auto-registration;
- Other containers, container logs и port actions;
- Diagnostics view beyond minimal runtime/error surface required by this slice;
- Docker Events, remote/multi-context support;
- Images navigation item;
- самостоятельную вторую реализацию lifecycle use cases;
- frontend authority над paths, names, runtime state или operation locks;
- реализацию кода в этой design/plan сессии.

## Сохранить решения approved design

- Lifecycle IPC принимает только `{ profileId }`.
- Frontend никогда не передаёт `workingDirectory`, `composeFiles`,
  `environmentFiles` или Compose name в lifecycle payload.
- Backend разрешает profile и paths через registry.
- `Stop` вызывает `compose stop`; `Tear down` вызывает `compose down`.
- `Remove profile` не вызывает Docker.
- Invalid profile остаётся видимым.
- Runtime/definition/operation errors не смешиваются.
- List/query не изменяет durable registry.
- React владеет только view/dialog/form state.
- Server projections принадлежат backend и приходят через typed IPC.
- List keys и pending state используют immutable IDs.
- `Tear down` destructive и требует явного подтверждения; profile остаётся.
- приложение открывается без Docker, profile list доступен offline.
- Context mismatch блокирует Compose operations, но не profile access.
- Images view отсутствует.

## Требования к plan

Plan должен содержать точные file paths, interfaces и DTO shapes, Tauri
command signatures, Rust↔TypeScript schema workflow, Zod decoder API,
TanStack Query boundaries, fake IPC protocol, component structure,
accessibility states, TDD steps, test commands, commit steps и acceptance
checklist. Каждая задача должна быть independently testable.

Не используй `TBD`, `TODO`, vague инструкции или «реализовать аналогично».
Не запускай `subagent-driven-development` и не реализуй Increment 3 в этой
сессии. Нужны только spec и plan; реализация будет отдельной сессией.
