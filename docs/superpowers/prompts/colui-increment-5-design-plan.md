# CoLUI Increment 5: Design + Plan Session Prompt

В отдельной сессии подготовь спецификацию и implementation plan для
**Increment 5: Discovery + Diagnostics + Recovery**.

Это финальная продуктовая итерация CoLUI 2.0 baseline. Четвёртая итерация
уже выполнена. Не считай старые планы точным описанием кода: сначала изучи
фактические interfaces, tests, commits и worktree.

Прочитай:

- `docs/superpowers/specs/2026-09-01-colui-2-design.md`;
- specs Increment 2, 3 и 4 в `docs/superpowers/specs/`;
- plans Increment 3 и 4 в `docs/superpowers/plans/`;
- `CoLUI_architecture_audit_v2.md`;
- commits после Increment 4, включая финальные fix/review commits;
- текущие domain/app/adapter/Tauri/IPC/frontend interfaces и tests.

Используй `superpowers:brainstorming`.

## Обязательный процесс

1. Классифицируй задачу как architectural.
2. Исследуй фактическую реализацию первых четырёх итераций.
3. Задавай уточняющие вопросы по одному за сообщение.
4. Предложи 2-3 подхода с trade-offs и рекомендацией.
5. Представь дизайн секциями и получи approval каждой секции.
6. После полного approval запиши spec:
   `docs/superpowers/specs/2026-09-04-colui-increment-5-discovery-design.md`
7. Проведи self-review spec: placeholders, противоречия, scope,
   неоднозначности, data safety и совместимость с фактическим кодом.
8. Закоммить только утверждённую spec.
9. Попроси пользователя проверить spec и остановись до явного approval.
10. После approval используй `superpowers:writing-plans`.
11. Создай plan:
    `docs/superpowers/plans/2026-09-04-colui-increment-5-discovery.md`
12. Проведи self-review plan и предложи execution mode.

## Scope Increment 5

### Discovery

- derive `DiscoveryCandidate` from current `RuntimeInventory` observations;
- stable session-local `candidateId` derived from normalized Compose project
  name and working directory;
- classifications: new unambiguous, name conflict, incomplete metadata,
  already registered;
- explicit conflict for same Compose name with different paths;
- existing profile is never silently updated from Docker labels;
- read-only candidate listing with no registry writes;
- manual register candidate command;
- optional auto-registration only for new unambiguous valid candidates;
- auto-registration through separate write use case, never refresh side effect;
- session-local candidate ignore;
- idempotency and cross-instance lost-race handling via
  `profile_already_registered`;
- bounded session journal for registration and operational events.

### Diagnostics and recovery

- Diagnostics navigation/view;
- RuntimeSession state, resolved endpoint and API/CLI fingerprints;
- connect, disconnect and explicit reconnect controls;
- clear ContextMismatch explanation and blocked Compose operations;
- registry path, revision and health;
- corrupt/locked/write-failure presentation;
- explicit restore from `.bak`, never silent repair;
- legacy v1 import result and recovery guidance;
- current operations, inventory freshness, last successful observation,
  definition issues and bounded session events;
- offline shell/profile access remains available.

### Standalone containers, logs and ports

- Other containers projection from existing coordinated inventory;
- standalone start/stop/restart via Bollard and typed errors;
- bounded on-demand container logs: newest 256 KiB per request/container,
  no persistence, no live-follow;
- exact log truncation indicator and UTF-8 replacement behavior;
- full port binding semantics: host IP, host port, container port, transport;
- copy binding always available;
- browser open only for recognized web TCP bindings;
- UDP never becomes URL;
- wildcard bind may map to loopback only for browser URL, never display/copy;
- accessible responsive UI and keyboard behavior.

## Не включай

- Docker Events;
- live-follow logs or persisted log history;
- remote/multi-context Docker;
- Images view;
- SQLite, RBAC, plugin architecture;
- Kubernetes/Podman abstraction;
- event sourcing or generic resource framework;
- automatic mutation of existing profiles from runtime observations;
- permanent ignored-candidate storage;
- second InventoryCoordinator, DefinitionCache, generation counter,
  operation-lock owner, RuntimeGateway or lifecycle implementation;
- broad visual redesign unrelated to final workflows;
- реализацию кода в этой design/plan сессии.

## Сохранить решения approved design и Increment 4

- `InventoryCoordinator` остаётся единственным owner fast refresh,
  generations, coalescing, retention и backoff.
- Discovery consumes immutable inventory; it does not call Docker for a
  second container listing.
- Official labels already mapped in `ContainerObservation`; reuse them.
- Queries never write `ProfileRegistry`.
- Auto-registration is a separate command/use case with explicit logging.
- Profile ID immutable; display name and Compose namespace distinct.
- Same-name profiles never merge automatically.
- `RuntimeGateway` remains sole API/CLI session owner.
- `OperationLockManager` remains sole per-profile operation-lock owner.
- Frontend receives derived projections and is never source of authority.
- Existing generation guard semantics remain: lower discarded, equal no-op.
- Definition and runtime errors remain independent.
- Stop/Tear down/Remove semantics and ID-only lifecycle IPC do not change.
- Application remains usable without Docker.

## Обязательные race и safety решения

Design должен явно определить:

- candidate disappears between list and register;
- metadata changes while registration is pending;
- two app instances register same candidate;
- auto-registration and manual registration race;
- candidate Compose name collides with one or several profiles;
- registry becomes corrupt/locked during registration;
- restore from backup while profile mutations or operations are active;
- runtime reconnect invalidates candidate IDs/session journal references;
- container disappears between inventory display and action/log request;
- logs exceed 256 KiB or end in partial UTF-8;
- port has wildcard/IPv6/UDP/unknown application protocol;
- Diagnostics reads state while refresh or lifecycle operation is active.

## Требования к plan

Plan должен использовать точные file paths из фактического worktree и
определить domain/application contracts, ownership, DTO/Zod schemas, Tauri
commands, TanStack Query keys/invalidation, discovery state machine,
registration policy, recovery flow, journal bounds, container action/log
ports, URL rules, component structure, accessibility behavior, TDD steps,
test commands, commit steps и acceptance checklist.

Обязательны hermetic tests для discovery classification, query purity,
idempotent/lost-race registration, no-update-existing policy, restore safety,
session-local ignore/journal bounds, disappearing containers, 256 KiB newest
log retention, truncation/UTF-8 behavior, URL/copy rules и typed UI recovery.
Real-Docker fixture расширяй только минимально: discovery from official
labels, standalone action, bounded logs и published binding. Не дублируй
apply-stop-apply-tear-down fixture без необходимости.

Добавь final CoLUI 2.0 baseline acceptance pass по всем 22 критериям master
spec и quality gates: нет hidden registry writes, name-only merge,
path-bearing lifecycle IPC, duplicate owners, unsupported platform claims и
navigation items без рабочего view.

Не используй `TBD`, `TODO`, vague инструкции или «реализовать аналогично».
Не запускай `subagent-driven-development` и не реализуй Increment 5 в этой
сессии. Нужны только spec и plan; реализация будет отдельной сессией.
