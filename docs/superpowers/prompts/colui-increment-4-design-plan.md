# CoLUI Increment 4: Design + Plan Session Prompt

В отдельной сессии подготовь спецификацию и implementation plan для
**Increment 4: InventoryCoordinator + Definitions**.

Третья итерация уже выполнена. Не предполагай её состояние по плану:
проверь фактические файлы, commits и worktree, особенно существующие
TanStack Query hooks, single-shot status IPC и lifecycle invalidation.

Прочитай сначала:

- `docs/superpowers/specs/2026-09-01-colui-2-design.md`;
- `docs/superpowers/specs/2026-09-02-colui-increment-2-runtime-design.md`;
- `docs/superpowers/specs/2026-09-02-colui-increment-3-ipc-ui-design.md`;
- `docs/superpowers/plans/2026-09-02-colui-increment-3-ipc-ui.md`;
- `docs/superpowers/plans/2026-09-02-runtime-query-foundation.md`;
- `CoLUI_architecture_audit_v2.md`;
- commits после завершения Increment 3 и текущее состояние worktree.

Используй `superpowers:brainstorming`.

## Обязательный процесс

1. Классифицируй задачу как architectural.
2. Проведи context exploration по фактической реализации Increment 3.
3. Задавай уточняющие вопросы по одному за сообщение.
4. Предложи 2-3 подхода с trade-offs и рекомендацией.
5. Представь дизайн секциями и получи approval каждой секции.
6. После полного approval запиши spec:
   `docs/superpowers/specs/2026-09-03-colui-increment-4-inventory-design.md`
7. Проведи self-review spec: placeholders, противоречия, scope,
   неоднозначности и несовместимость с уже выполненным кодом.
8. Закоммить только spec.
9. Попроси пользователя проверить spec и остановись до явного approval.
10. После approval используй `superpowers:writing-plans`.
11. Создай plan:
    `docs/superpowers/plans/2026-09-03-colui-increment-4-inventory.md`
12. Проведи self-review plan и предложи execution mode.

## Scope Increment 4

- единый backend `InventoryCoordinator`;
- один owner refresh вместо независимых frontend polling loops;
- at most one in-flight inventory refresh;
- concurrent callers share current refresh result;
- immutable `RuntimeInventory` snapshots;
- monotonic generation numbers;
- backend publication rejects older generations;
- frontend generation guard: lower generation discarded, equal generation
  is idempotent no-op without cache replacement or subscriber notification;
- one Docker `list all containers` call per fast refresh;
- normalize raw containers into `ContainerInstance`;
- official Compose labels for service/project association;
- derive project runtime snapshots and standalone-container data needed by
  current projections, without implementing Discovery UI;
- last successful snapshot remains visible after refresh failure;
- stale/unavailable state, observed timestamp and bounded backoff;
- slow definition path via `docker compose config`;
- `DefinitionCache` keyed by profile ID and profile revision;
- definition invalidation after profile changes, foreground details,
  explicit refresh and 60-second stale expiry;
- no `compose config` on each three-second runtime poll;
- `DefinitionState`: unchecked, valid, invalid, stale;
- operation locks: one lifecycle operation per profile;
- duplicate action returns `operation_conflict`;
- background definition refresh yields to lifecycle operation;
- global Compose CLI concurrency remains one;
- successful lifecycle operation produces exactly one inventory refresh;
- TanStack Query polling/invalidation integration over existing Increment 3
  foundation;
- tests for generations, refresh coalescing, failure/backoff, fast/slow
  separation, cache invalidation and operation locks.

## Не включай

- Discovery candidates UI, conflict resolution или auto-registration;
- отдельную Increment 5 Diagnostics view;
- standalone-container lifecycle UI, bounded container logs и port actions;
- Docker Events;
- remote или multi-context Docker;
- live-follow logs;
- Images view;
- новый lifecycle implementation;
- name-based React keys или frontend authority над runtime state;
- SQLite, generic resource framework или speculative abstractions;
- визуальный redesign вне изменений, нужных для отображения inventory
  freshness/error state;
- реализацию кода в этой design/plan сессии.

## Сохранить решения approved design

- `RuntimeInventory` immutable и содержит `generation`, `observed_at`,
  `runtime_session_id` и daemon fingerprint.
- Fast path использует один Docker container listing.
- Runtime observation не изменяет durable profiles.
- `list_project_summaries` и прочие queries не пишут registry.
- `compose config` отделён от fast runtime polling.
- Profile, definition и runtime остаются разными сущностями.
- Definition errors и runtime errors не смешиваются.
- Invalid profile остаётся видимым.
- Docker outage не удаляет snapshot/profile access.
- Frontend отображает derived projections и не является authority.
- Polling pause при hidden window и backoff для runtime unavailable должны
  быть совместимы с уже принятыми QueryClient defaults Increment 3.
- Lifecycle IPC остаётся ID-based; frontend не передаёт paths.
- Stop/Tear down/Remove semantics не меняются.
- Compose CLI и Bollard используют один `RuntimeGateway`.

## Обязательная проверка границ

Во время exploration выясни, какие части из scope уже случайно появились в
Increment 3. Если найдёшь пересечение, в design явно опиши migration path:
что переиспользуется, что переносится, а что удаляется. Не создавай второй
refresh API, второй generation counter, второй cache или второй lifecycle
lock owner.

Отдельно зафиксируй поведение при следующих гонках:

- два одновременных `refresh_inventory`;
- lifecycle success во время background refresh;
- lifecycle action во время definition refresh;
- reconnect/disconnect во время inventory request;
- old response после нового published generation;
- profile revision change во время definition load;
- runtime outage после успешного snapshot.

## Требования к plan

Plan должен содержать точные file paths по фактическому worktree, ownership
и interfaces coordinator/cache/ports, DTO shapes, IPC command signatures,
TanStack Query integration, refresh state machine, generation semantics,
backoff policy, definition-cache invalidation rules, operation-lock API,
fake Docker API protocol, fake Compose runner expectations, TDD steps,
test commands, commit steps и acceptance checklist. Каждая задача должна
быть independently testable.

Добавь hermetic tests без Docker для refresh coalescing, old-generation
rejection, failure retention/backoff, no-config-on-fast-poll, cache TTL,
profile revision invalidation и per-profile operation conflicts. Добавь
real-Docker smoke scope только если фактический Increment 3/2 contract
делает его необходимым; иначе явно обоснуй отсутствие нового fixture.

Не используй `TBD`, `TODO`, vague инструкции или «реализовать аналогично».
Не запускай `subagent-driven-development` и не реализуй Increment 4 в этой
сессии. Нужны только spec и plan; реализация будет отдельной сессией.
