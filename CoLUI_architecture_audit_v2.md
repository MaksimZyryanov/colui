# CoLUI — архитектурный аудит и концепция версии 2.0

**Статус:** архитектурный анализ  
**Дата:** 31 августа 2026  
**Область:** CoLUI 0.1.0, база знаний и исходный код  
**Цель:** снизить концептуальную сложность и сформировать реалистичный путь к 2.0 без big-bang rewrite.

---

## Executive summary

CoLUI — локальный desktop-инструмент разработчика для управления Docker containers и Docker Compose projects. Frontend реализован на React/TypeScript, desktop shell — Tauri, backend — Rust. Контейнеры управляются через Bollard/Docker API, а Compose lifecycle — через внешний `docker compose` CLI.

Главная проблема проекта не в стеке. Она в **семантическом сжатии разных сущностей в одну модель**. Текущий `ComposeProject` одновременно играет роль сохранённого профиля, описания Compose configuration, runtime project, набора контейнеров, UI card и command target. Поле `name` одновременно является display label, persistent identity, Compose namespace, merge key и frontend pending key.

Из этого следуют основные сложности: name-only merge, скрытые записи во время catalog refresh, неоднозначный `source`, смешение service definition и container instance, неявный lifecycle, path-bearing IPC и рассинхронизация Docker API с Compose CLI.

> **CoLUI 2.0 — локальный модульный монолит, в котором стабильный ProjectProfile владеет Compose invocation, RuntimeGateway владеет одной проверенной Docker session, inventory является read-only snapshot, lifecycle-команды адресуются по ID, а frontend отображает derived projections и не является источником authority.**

Три изменения с максимальным эффектом:

1. Стабильный `profile_id` и backend-resolved lifecycle-команды.
2. Единый `RuntimeGateway`, подтверждающий один Docker daemon для Bollard и Compose CLI.
3. Разделение persisted profile, Compose definition и runtime inventory.

---

# 1. Краткое понимание проекта

## Назначение

CoLUI предоставляет графический интерфейс для локальной работы с Docker без постоянного перехода в CLI. Подтверждённые сценарии: просмотр standalone containers, start/stop/restart, logs, discovery Compose projects по Docker labels, сохранение каталога, ручное добавление проекта, Compose lifecycle, просмотр остановленного проекта через `docker compose config`, открытие published TCP ports и macOS tray.

## Пользователи

Основная аудитория — разработчики и DevOps-инженеры, работающие с локальным Docker runtime. Проверенный native path сейчас — macOS с Docker Desktop или Colima. Linux и Windows не подтверждены текущей test/release matrix.

## Компоненты

Frontend: `src/App.tsx`, `useContainers.ts`, `useComposeProjects.ts`, `components/containers/*`, `components/compose/*`, `lib/invoke.ts`, `lib/mock-backend.ts`, `types/docker.ts`.

Backend: `commands.rs`, `docker.rs`, `compose.rs`, `compose_cli.rs`, `projects.rs`, `catalog.rs`, `types.rs`, `main.rs`, `tray.rs`.

## Источники истины

| Вид данных | Source of truth |
|---|---|
| Docker runtime | Docker daemon через Bollard |
| Compose runtime membership | Docker labels `com.docker.compose.*` |
| Saved paths | `~/.colui/projects.json` |
| Compose desired definition | project files через `docker compose config` |
| Compose execution semantics | внешний Docker Compose CLI |
| Frontend transient state | React hooks |
| Browser dev state | mutable in-memory mock |

Проблема не в наличии нескольких источников, а в том, что они сливаются в одну сущность без явного ownership.

---

# 2. Карта текущих концепций

| Концепция | Текущее значение | Проблема | Значение в 2.0 |
|---|---|---|---|
| `ComposeProject` | Runtime + saved config + services + status | Смешивает lifecycles | Только UI projection |
| `SavedProject` | Persisted paths + name | Имя = identity | `ProjectProfile` + stable ID |
| `name` | label + namespace + key + target | Перегруженная identity | `id`, `displayName`, `composeProjectName` |
| `ProjectSource` | discovered/saved | После merge неоднозначен | `registrationOrigin` только у profile |
| `ProjectStatus` | running/partial/stopped/exited/not-started | Смешивает разные оси | runtime/definition/operation states |
| `ComposeService` | definition + runtime instance | Пустые runtime поля | `ServiceDefinition` + `ContainerInstance` |
| Catalog | persistence + discovery + config expansion | Query имеет side effects | `ProfileRegistry` отдельно от inventory |
| Discovery | labels + auto-save | Observation меняет durable state | `DiscoveryCandidate` |
| Compose lifecycle payload | name + paths + env files | UI повторно передаёт authority | Только `profileId` |
| Docker connection | Bollard client | Не владеет CLI context | `RuntimeSession` |
| Polling | независимые hooks | snapshots разных поколений | один coordinator |
| Stop | UI intent | фактически `compose down` | Stop и Tear down отдельно |
| Error | String | нет machine-readable recovery | `AppError` |
| Images | navigation placeholder | неподтверждённый workflow | убрать до реальной потребности |

---

# 3. Главные концептуальные ошибки

## Ошибка 1: имя проекта используется как identity

**Статус:** подтверждённый факт.

`SavedProject` удаляется по имени, runtime и saved catalog merge выполняется по project name, frontend pending state также опирается на имя.

```text
name используется повсюду
→ нет stable identifier
→ display label, Compose namespace и persistent identity становятся одним понятием
→ collision/rename затрагивает persistence, merge, commands и UI state
→ нужен immutable ProfileId
```

**Решение 2.0:** `ProfileId` — единственный внутренний identity; отдельно `displayName` и `composeProjectName`.

## Ошибка 2: `ComposeProject` смешивает несколько сущностей

**Статус:** подтверждённый факт.

В DTO одновременно находятся persisted paths, runtime services, status, source и configuration error. `ComposeService` одновременно описывает service definition и container instance.

**Нарушенный принцип:** одна сущность — один lifecycle и один владелец данных.

**Решение:** разделить `ProjectProfile`, `ProjectDefinition`, `ProjectRuntimeSnapshot`, `ServiceDefinition`, `ContainerInstance`, `ProjectSummary`.

## Ошибка 3: query изменяет durable state

**Статус:** подтверждённый факт.

`list_compose_projects` выполняет discovery и внутри `ProjectsConfig::transaction` вызывает `sync_discovered`, поэтому polling каталога может записывать `projects.json`.

```text
auto-registration встроена в refresh
→ query становится writer
→ чтение UI меняет durable state
→ ошибки чтения и persistence смешиваются
→ discovery и registry mutation надо разделить
```

**Решение:** `list_project_summaries` read-only; discovery возвращает candidates; регистрация — отдельный use case или явная auto-registration policy.

## Ошибка 4: frontend передаёт backend authority обратно в lifecycle command

**Статус:** подтверждённый факт.

Compose lifecycle IPC принимает project name, working directory, config files и environment files, хотя эти данные уже хранятся в backend catalog.

**Решение:** lifecycle-команды принимают только `profileId`, backend сам разрешает current paths из registry.

## Ошибка 5: два Docker control planes не имеют общего owner

**Статус:** подтверждённый факт.

Bollard и Docker CLI выбирают runtime независимо. UI трактует их как один Docker runtime, хотя они могут смотреть на разные daemons.

**Решение:** `RuntimeGateway` с одной `RuntimeSession` и проверкой daemon fingerprint API ↔ CLI.

## Ошибка 6: Stop и Tear down смешаны

**Статус:** подтверждённый факт.

Текущий Compose lifecycle использует `up -d`, `down`, `restart`; действие остановки фактически разрушает Compose runtime resources сильнее, чем обычный `compose stop`.

**Решение:** `stop_project` → `docker compose stop`; `tear_down_project` → `docker compose down`; `remove_profile` → только registry mutation.

## Ошибка 7: `ProjectStatus` выражает несколько независимых измерений

**Статус:** сильная гипотеза, подтверждённая формой модели.

`running`, `partial`, `stopped`, `exited`, `not-started` смешивают runtime presence, activity и наличие saved configuration.

**Решение:** независимые `DefinitionState`, `RuntimePresence`, `RuntimeActivity`, `OperationState`, `Issues[]`; пользовательская подпись — derived projection.

## Ошибка 8: polling принадлежит component lifecycle

**Статус:** подтверждённый факт.

Containers и Compose имеют независимые polling loops и разный lifecycle. Это усложняет freshness, race handling и командные refreshes.

**Решение:** один backend inventory coordinator с generation numbers.

## Ошибка 9: timeout future не равен timeout процесса

**Статус:** подтверждённый факт.

`tokio::time::timeout` ограничивает ожидание future, но child process `docker compose` не имеет гарантированной termination-on-drop. Timed-out children могут продолжать работу и overlap.

**Решение:** `ComposeProcessRunner` владеет spawn/wait/terminate/reap lifecycle.

## Ошибка 10: строковые ошибки являются application protocol

**Статус:** подтверждённый факт.

Многие Tauri commands возвращают `Result<_, String>`.

**Решение:** typed `AppError` с `code`, `message`, `details`, `retryable`, `operation`, optional `subjectId`.

## Ошибка 11: TypeScript IPC contract не декодируется runtime

**Статус:** подтверждённый факт.

`invoke<T>` даёт compile-time ожидание, но не runtime schema guarantee.

**Решение:** один IPC decoder boundary и contract tests Rust ↔ TypeScript.

## Ошибка 12: Images существует без рабочего пользовательского сценария

**Статус:** подтверждённый факт.

**Решение:** убрать navigation item до появления подтверждённой потребности.

---

# 4. Противоречия и пробелы

| Тема | Противоречие/пробел | Влияние | Default для 2.0 |
|---|---|---|---|
| Linux support | intent шире verified docs | release scope | macOS only до matrix |
| Stop semantics | UI intent ≠ `down` | safety | разделить Stop/Tear down |
| Auto-registration | ценная функция встроена в query | boundaries | explicit policy |
| Docker context | UI считает runtime единым | correctness | fingerprint gate |
| Timeout | future timeout без hard child kill | reliability | process owner |
| Port URL | TCP трактуется как web endpoint | UX correctness | хранить полный binding |
| Registry recovery | corrupt JSON не overwrite, но repair owner нет | data safety | backup/recovery flow |
| Product IA | текущие views равноправны, project-first — гипотеза | UX | проверить исследованием |
| Multi-context | нет подтверждённого use case | scope | один context |
| Live logs | только finite tail | scope | отложить |

---

# 5. Что сохранить, изменить и удалить

## Сохранить

- Rust + Tauri + React: стек адекватен desktop utility.
- Bollard для container inventory/lifecycle.
- Внешний Docker Compose CLI как owner Compose semantics.
- Persisted profiles, чтобы project оставался управляемым после `down`.
- Docker label discovery.
- Browser mock как frontend development path.
- macOS tray.
- Separate argv construction без shell interpolation.

## Изменить

- Catalog → `ProfileRegistry`.
- `SavedProject` → `ProjectProfile`.
- `ComposeProject` → derived projection.
- Polling → coordinated inventory refresh.
- Auto-registration → явная policy.
- String errors → typed errors.
- Startup Docker gate → recoverable `RuntimeSession`.

## Удалить

- name-only identity;
- path-bearing lifecycle IPC;
- hidden writes из list query;
- merged `ProjectSource`;
- hybrid `ComposeService`;
- `not-started` как доменный status;
- Images placeholder;
- абстракции для гипотетического будущего.

---

# 6. Целевая концепция версии 2.0

## Основная модель

Для Compose environment существуют три разных утверждения:

- `ProjectProfile`: что пользователь зарегистрировал и хочет уметь запускать снова;
- `ProjectDefinition`: что описывают Compose files;
- `ProjectRuntimeSnapshot`: что реально существует в Docker daemon сейчас.

Они не должны сливаться в одну entity.

## Ключевые сущности

```text
ProjectProfile
ProjectDefinition
RuntimeSession
RuntimeInventory
ProjectRuntimeSnapshot
ServiceDefinition
ContainerInstance
DiscoveryCandidate
Operation
AppError
```

## Source of truth

| Данные | Owner |
|---|---|
| Profile identity и paths | ProfileRegistry |
| Desired Compose definition | Compose files |
| Runtime containers | Docker daemon |
| Compose execution semantics | Docker Compose CLI |
| Runtime identity | RuntimeSession |
| UI status | Derived projection |
| Discovery candidates | Current runtime observation |

## Бизнес-инварианты

1. `ProfileId` стабилен и не зависит от имени/path.
2. Display name не является Compose namespace.
3. Query не изменяет registry.
4. Runtime observation не обновляет existing profile автоматически.
5. Lifecycle command принимает `ProfileId`.
6. Backend сам разрешает paths.
7. Bollard и CLI должны указывать на один daemon.
8. Stop, Tear down и Remove profile — разные операции.
9. Definition state и runtime state независимы.
10. Invalid profile остаётся видимым.
11. Docker outage не делает registry недоступным.
12. Один service definition может иметь 0..N container instances.
13. Frontend state не является authority.
14. Ошибка имеет stable code.

---

# 7. Минимальная архитектура 2.0

## Стиль

**Модульный монолит внутри одного Tauri desktop application.** Распределённость не требуется; главный риск — semantics и boundaries, а не scale.

```text
Пользователь
    ↓
React UI
    ↓ typed Tauri IPC
Application Use Cases
    ├── Profiles
    ├── Projects
    ├── Containers
    └── Runtime
    ↓
Domain models
    ↓
Adapters
    ├── ProfileRegistry
    ├── DockerApiAdapter (Bollard)
    └── ComposeAdapter (docker compose)
    ↓
Filesystem + Docker daemon + Docker CLI
```

## Модули

### Runtime
Connection lifecycle, endpoint resolution, daemon fingerprint, Bollard adapter, CLI environment.

### Profiles
Registry schema, CRUD, revisions, validation, migrations, durable writes.

### Definitions
`docker compose config`, desired services, definition cache, configuration issues.

### Inventory
Единый Docker snapshot, Compose association, standalone containers, generation/freshness.

### Operations
Apply/Stop/TearDown/Restart, operation locks, timeouts, process ownership, refresh after command.

### Discovery
Docker Compose metadata, candidates, conflict detection, auto-registration policy.

### IPC
Transport boundary и DTO mapping без domain rules.

---

# 8. Новая модель предметной области

## ProjectProfile

```text
id: ProfileId
revision: u64
display_name: String
compose_project_name: String
working_directory: Path
compose_files: Vec<Path>
environment_files: Vec<Path>
registration_origin: Manual | Discovered | Migrated
```

Инварианты: ID immutable, revision monotonic, порядок files сохраняется, paths не являются identity, invalid definition не удаляет profile.

Не содержит runtime state, container IDs и transient Docker errors.

## ProjectDefinition

```text
profile_id
definition_revision
loaded_at
services[]
issues[]
```

States: `Unchecked | Valid | Invalid | Stale`.

## ServiceDefinition

```text
name
image?
build_context?
declared_ports[]
```

## ContainerInstance

```text
id
name
image
state
status_text
service_name?
published_ports[]
```

## RuntimeSession

```text
Disconnected
Connecting
Ready
ContextMismatch
Failed
```

При `Ready`: API daemon fingerprint == CLI daemon fingerprint.

## RuntimeInventory

```text
generation
observed_at
runtime_session_id
daemon_fingerprint
containers[]
project_snapshots[]
standalone_containers[]
```

Immutable snapshot.

## DiscoveryCandidate

Runtime observation, который ещё не является durable registration.

## Operation

```text
subject_id
kind: Apply | Stop | TearDown | Restart
phase
started_at
deadline
```

---

# 9. API-контракты

## Queries

| Команда | Вход | Результат | Durable side effects |
|---|---|---|---|
| `get_application_state` | — | runtime/registry/capabilities | нет |
| `list_project_summaries` | — | summaries | нет |
| `get_project_details` | `profileId` | profile + definition + instances | нет |
| `list_discovery_candidates` | — | candidates | нет |
| `list_standalone_containers` | — | containers | нет |
| `get_container_logs` | `containerId`, options | bounded logs | нет |
| `inspect_profile_draft` | draft | validation + definition | нет |
| `refresh_inventory` | — | coherent snapshot | нет durable |
| `refresh_project_definition` | `profileId` | definition | нет durable |

## Commands

| Команда | Вход | Семантика |
|---|---|---|
| `connect_runtime` | optional preference | создать/проверить session |
| `disconnect_runtime` | — | закрыть session |
| `create_profile` | draft | validate + persist |
| `update_profile` | id + expected revision + patch | update |
| `remove_profile` | id + expected revision | registry only |
| `register_discovery_candidate` | candidateId | создать profile |
| `ignore_discovery_candidate` | candidateId | скрыть candidate |
| `apply_project` | profileId | `compose up -d` |
| `stop_project` | profileId | `compose stop` |
| `tear_down_project` | profileId | `compose down` |
| `restart_project` | profileId | `compose restart` |
| container lifecycle | containerId | Bollard operations |

---

# 10. UI projection

```ts
interface ProjectSummary {
  profile: {
    id: string;
    revision: number;
    displayName: string;
    composeProjectName: string;
    workingDirectory: string;
    registrationOrigin: "manual" | "discovered" | "migrated";
  };
  definition: {
    state: "unchecked" | "valid" | "invalid" | "stale";
    revision?: string;
    serviceCount?: number;
  };
  runtime: {
    presence: "unavailable" | "absent" | "present";
    activity?: "all-running" | "mixed" | "none-running";
    containerCount: number;
    runningContainerCount: number;
    observedAt?: string;
  };
  operation: {
    kind: "apply" | "stop" | "tear-down" | "restart";
    phase: "running";
    startedAt: string;
  } | null;
  issues: IssueSummary[];
}
```

| Definition | Runtime | UI label |
|---|---|---|
| Valid | Absent | Ready to start |
| Valid | All running | Running |
| Valid | Mixed | Partially running |
| Valid | None running | Stopped |
| Invalid | any | Configuration needs attention |
| any | unavailable | Docker unavailable |
| Stale | any | Definition may have changed |

Label остаётся presentation value, не доменным enum.

---

# 11. Typed error contract

```ts
interface AppError {
  code: AppErrorCode;
  operation: string;
  subjectId?: string;
  message: string;
  details?: string;
  retryable: boolean;
}
```

Минимальные codes:

```text
runtime_unavailable
runtime_connection_failed
runtime_context_mismatch
profile_not_found
profile_revision_conflict
profile_invalid
definition_failed
compose_failed
container_operation_failed
operation_conflict
operation_timeout
registry_corrupt
registry_locked
registry_write_failed
permission_denied
protocol_mismatch
```

`message` — UX, `details` — diagnostics, frontend decisions — по `code`.

---

# 12. RuntimeGateway

```text
RuntimeGateway
├── RuntimeSession
├── DockerApiAdapter
├── ComposeAdapter
└── ComposeProcessRunner
```

Connection flow:

```text
resolve Docker endpoint
→ connect Bollard
→ ping/API fingerprint
→ build CLI environment from same session
→ CLI fingerprint
→ compare
→ Ready only if equal
```

При `ContextMismatch`: inventory можно показать с warning, но Compose lifecycle блокируется.

Baseline 2.0: один локальный Docker endpoint. Remote Docker и multi-context отложить.

---

# 13. Управление дочерними процессами

`ComposeProcessRunner`:

```text
spawn
→ wait with deadline
→ collect bounded output
→ timeout? terminate
→ wait/reap
→ typed result
```

Обязательные свойства:

- executable + args раздельно;
- `cwd` только из backend-resolved profile;
- Docker vars из RuntimeSession;
- bounded stdout/stderr;
- hard timeout;
- termination подтверждается;
- exit code + stderr сохраняются в diagnostics.

---

# 14. Конкуренция

Baseline:

- один inventory refresh одновременно;
- одна lifecycle operation на profile;
- bounded/global gate для Compose CLI;
- lifecycle приоритетнее background definition refresh;
- повторная action по busy profile → `operation_conflict`;
- standalone container actions независимы;
- Docker mutex не удерживается через длинный network await.

Сначала допустима глобальная сериализация Compose CLI. Параллельность увеличивать только после измерений.

---

# 15. Inventory и синхронизация

## Один coherent snapshot

```text
Docker Engine
  ↓ list all containers once
RawContainer[]
  ↓ normalize
ContainerInstance[]
  ├── Compose associations
  ├── ProjectRuntimeSnapshot[]
  └── StandaloneContainer[]
```

Snapshot fields:

```text
generation
observed_at
runtime_session_id
daemon_fingerprint
```

Frontend не применяет generation старее текущего.

## Fast path

Container existence/state/status, association labels, published bindings.

## Slow path

Compose definition, build context, declared services/ports, validation issues.

`docker compose config` не запускается на каждом 3-second runtime poll.

## Refresh lifecycle

```text
Idle
→ Refreshing(N)
   ├── success → Publish N → Idle
   ├── failure → unavailable N → Backoff
   └── superseded → Discard N
```

Docker Events не обязательны для первого 2.0 release: coordinated polling проще и закрывает основную race complexity.

---

# 16. Registry и discovery

## Registry v2

```json
{
  "schemaVersion": 2,
  "registryRevision": 14,
  "profiles": [
    {
      "id": "f9df73ec-8d43-4ca3-b984-96de3e02a991",
      "revision": 3,
      "displayName": "Checkout",
      "composeProjectName": "checkout",
      "workingDirectory": "/workspace/checkout",
      "composeFiles": ["compose.yml", "compose.local.yml"],
      "environmentFiles": [".env.local"],
      "registrationOrigin": "manual"
    }
  ]
}
```

Invariants: ID immutable, order files preserved, paths не identity, invalid profile не auto-delete, runtime state не persist, discovery не auto-update existing profile, writes через temp/fsync/atomic rename, backup перед migration/repair, один writer.

## Discovery policy

1. Новый unambiguous valid project может auto-register.
2. Existing profile не обновляется автоматически.
3. Same Compose name + другие paths → conflict candidate.
4. Invalid metadata остаётся transient candidate.
5. Auto-registration логируется.
6. Queries не сохраняют registry.

---

# 17. Frontend 2.0

Рекомендуемый baseline:

```text
Projects
├── Registered environments
├── Discovered conflicts
└── Other containers

Diagnostics
└── Runtime, registry and operations
```

Project-first IA — продуктовая гипотеза. Даже если Containers и Compose останутся равноправными views, backend/domain redesign остаётся актуальным.

`Tear down` должен находиться в overflow/destructive context, иметь confirmation и не удалять profile.

State ownership:

| State | Owner |
|---|---|
| View/dialog state | React |
| Registry | Rust Profiles |
| Runtime connection | RuntimeGateway |
| Inventory | Inventory coordinator |
| Operation locks | Operations |
| Definition cache | Definitions |
| Error classification | backend `AppError` |
| Human-readable layout | React |

IPC boundary:

```text
Tauri response
→ runtime validation
→ typed DTO
→ React state
```

---

# 18. ADR

## ADR-1: Stable ProjectProfile ID

**Решение:** immutable `ProfileId`.  
**Почему:** имя/path меняются и не должны менять identity.  
**Цена:** schema migration.

## ADR-2: Lifecycle IPC принимает только ProfileId

**Решение:** backend lookup profile перед execution.  
**Почему:** один source of authority.  
**Цена:** registry становится обязательным owner.

## ADR-3: Profile, Definition и Runtime разделены

**Решение:** независимые модели + derived projection.  
**Почему:** разные sources of truth и lifecycles.  
**Цена:** больше типов, меньше условной логики.

## ADR-4: Один RuntimeGateway

**Решение:** session owner + API/CLI daemon identity check.  
**Почему:** исключить action в другом Docker context.  
**Цена:** немного сложнее connection flow.

## ADR-5: Polling остаётся, но имеет одного owner

**Решение:** coordinated inventory refresh.  
**Почему:** проще Docker Events на первом этапе.  
**Пересмотр:** при измеримой freshness/performance проблеме.

## ADR-6: Registry остаётся JSON

**Решение:** versioned JSON + atomic writes + backup + single writer.  
**Почему:** SQLite не решает domain ambiguity.  
**Пересмотр:** history, complex queries, multi-writer.

## ADR-7: Docker Compose CLI остаётся owner semantics

**Решение:** не писать собственный Compose orchestrator.  
**Почему:** уменьшение scope и риска.

## ADR-8: Stop, Tear down и Remove profile разделены

**Решение:** разные commands и UX intents.  
**Почему:** разные последствия для runtime и registry.

---

# 19. План миграции

## Этап 0: снизить риск 0.1.x

1. Переименовать current Stop в Tear down/Down, пока backend вызывает `down`.
2. Добавить confirmation.
3. Hard-terminate Compose child по timeout.
4. Не переписывать `projects.json`, если content не изменился.

## Этап 1: ProjectProfile + registry v2

Добавить `ProfileId`, `ProjectProfile`, `ProfileDraft`, `ProfileRevision`, `ProfileRegistry`. Reader временно поддерживает v1 и v2.

Migration:

```text
id ← UUID
revision ← 1
displayName ← old.name
composeProjectName ← old.name
workingDirectory ← old.working_dir
composeFiles ← old.config_files
environmentFiles ← old.environment_files
registrationOrigin ← migrated
```

Порядок: exclusive lock → read bytes → parse v1 → backup → build v2 → validate → temp write → fsync → atomic replace → reread/validate.

Invalid paths не удаляют profile; malformed JSON не перезаписывается автоматически.

## Этап 2: ID-based lifecycle

```text
profileId
→ registry lookup
→ current state/revision
→ validation
→ runtime session check
→ Compose invocation
→ inventory refresh
```

Legacy endpoints временно вызывают те же use cases через adapter. Второй реализации lifecycle быть не должно.

## Этап 3: RuntimeGateway

Единый endpoint, fingerprint, reconnect, mismatch state, hard cancellation, typed errors.

## Этап 4: Разделить catalog flow

Старый `list_compose_projects` заменить внутренними use cases:

```text
read_runtime_inventory
list_profiles
load_definitions
build_project_summaries
process_discovery_candidates
```

Критерий: `list_project_summaries` не пишет registry.

## Этап 5: Frontend migration

1. RuntimeSession header.
2. ProjectSummary.
3. Offline profile access.
4. Typed errors.
5. Definition + instances.
6. Other containers.
7. Discovery conflicts.
8. Remove legacy ComposeProject assumptions.
9. Remove Images placeholder.

## Этап 6: удалить legacy

Удалить path-bearing commands, name-only merge, `SavedProject`, merged `ProjectSource`, hybrid `ComposeService`, hidden write path, independent pollers и compatibility mapper.

---

# 20. Приоритеты

| Изменение | Ценность | Снижение сложности | Стоимость | Риск | Приоритет |
|---|---:|---:|---:|---:|---:|
| Stable profile ID | 5 | 5 | 3 | 2 | P0 |
| Backend-resolved lifecycle | 5 | 5 | 3 | 2 | P0 |
| Stop vs Tear down | 5 | 4 | 2 | 1 | P0 |
| RuntimeGateway | 5 | 4 | 4 | 3 | P0 |
| Hard child timeout | 5 | 3 | 3 | 2 | P0 |
| Read-only queries | 5 | 5 | 4 | 3 | P0 |
| Profile/Definition/Runtime split | 5 | 5 | 4 | 3 | P1 |
| Registry v2 migration | 4 | 4 | 4 | 3 | P1 |
| Typed errors | 4 | 4 | 3 | 2 | P1 |
| Offline shell + reconnect | 4 | 3 | 3 | 2 | P1 |
| Single refresh coordinator | 4 | 4 | 4 | 2 | P1 |
| Per-profile operation lock | 4 | 3 | 2 | 1 | P1 |
| Project-first workspace | 3 | 3 | 4 | 3 | P2 |
| Discovery conflicts | 4 | 4 | 3 | 2 | P2 |
| Remove Images placeholder | 2 | 2 | 1 | 1 | P2 |
| Docker Events | 2 | 1 | 4 | 3 | P3 |
| Multi-context Docker | 2 | -1 | 5 | 4 | P3 |
| SQLite | 1 | -1 | 4 | 3 | P3 |

Пять максимальных упрощений: stable ID; backend-resolved commands; разделение Profile/Definition/Runtime; read-only queries; единый RuntimeGateway.

---

# 21. Тестовая стратегия

## Domain

- ID стабилен после rename.
- Display name не меняет Compose namespace.
- Duplicate display names допустимы.
- Compose name collision → conflict, не merge.
- Runtime snapshot не меняет profile.
- Service definition и instances разделены.
- Scale: один service, несколько containers.
- Invalid profile остаётся видимым.
- Stop/Tear down/Remove — разные intents.

## Registry

- v1→v2 сохраняет entries и порядок files.
- interrupted write не повреждает canonical file.
- malformed JSON не overwrite.
- backup recovery.
- stale expected revision → conflict.
- второй writer блокируется.
- invalid path не удаляет profile.

## Runtime adapters

- Bollard и CLI используют одну session config.
- mismatch → `runtime_context_mismatch`.
- Compose operation блокируется.
- argv без shell interpolation.
- cwd из backend profile.
- timeout убивает child и reap подтверждён.
- output bounded.
- non-zero → `compose_failed`.

## Application

- `list_project_summaries` не сохраняет registry.
- lifecycle IPC содержит только profile ID.
- stale UI paths не влияют на execution.
- один profile не получает две lifecycle operations одновременно.
- command success создаёт один refresh generation.
- old generation не overwrites new.
- discovery не меняет existing profile.

## Frontend

- shell открывается без Docker.
- reconnect без process restart.
- typed error → правильный recovery.
- Tear down destructive, Stop non-destructive.
- path не входит в command payload.
- invalid definition + running runtime показываются одновременно.
- stale snapshot показывает timestamp.
- protocol mismatch не превращается в empty list.

## Real integration fixture

```text
1. Создать temp Compose project.
2. Connect RuntimeGateway.
3. Verify daemon fingerprints.
4. Register profile.
5. Apply.
6. Inventory sees containers.
7. Stop.
8. Containers still exist but are stopped.
9. Apply again.
10. Tear down.
11. Runtime absent, profile persists.
12. Change definition.
13. Refresh definition.
14. Verify revision.
15. Remove profile and fixture.
```

Дополнительные cases: same name/different directory, missing env file, process timeout, registry corruption, CLI context mismatch, scaled service, startup without Docker then reconnect.

---

# 22. Критерии приёмки 2.0

1. Persisted model имеет stable `profile_id`.
2. Rename display name не влияет на runtime namespace.
3. Lifecycle IPC не содержит paths.
4. Backend разрешает profile через registry.
5. Разные profiles не merge только по имени.
6. `list_project_summaries` не пишет registry.
7. Runtime refresh делает одно container listing.
8. Старые generations не применяются.
9. `compose config` не запускается на каждом runtime poll.
10. Bollard и CLI подтверждают один daemon fingerprint.
11. Compose actions блокируются при mismatch.
12. Timeout завершает OS child.
13. Stop вызывает `compose stop`.
14. Tear down вызывает `compose down`.
15. Remove profile не вызывает Docker.
16. Docker outage не закрывает profiles.
17. Reconnect не требует restart CoLUI.
18. Invalid profile не скрывает остальные.
19. Runtime и definition errors разделены.
20. Service definition и container instance — разные типы.
21. Migration v1→v2 сохраняет ordered paths.
22. Real fixture проходит apply–stop–apply–tear-down.

Quality gates: нет новых `Result<_, String>` на IPC boundary; нет frontend keys по Compose name; нет hidden registry mutations; нет двух lifecycle implementations; нет platform support promises без matrix tests; нет navigation item без рабочего view.

---

# 23. Product validation

Project-first interface — продуктовая гипотеза, не доказанный факт.

Нужно наблюдать Compose-first и container-first пользователей и фиксировать: с какого объекта начинается задача, сколько переключений между views, понимается ли Stop/Tear down, видны ли config errors, работает ли recovery после Docker outage, воспринимается ли auto-discovered project как trusted profile, нужен ли Images workflow.

Результат исследования может изменить navigation, но не core domain boundaries.

---

# 24. Риски 2.0

- **Слишком ручной discovery** может потерять ценность auto-registration. Default: auto-register только unambiguous valid candidates.
- **Слишком строгий fingerprint gate** нужно прототипировать на Docker Desktop и Colima.
- **Project-first IA** может ухудшить container-first workflow. Архитектура должна быть независима от IA.
- **Сложный revision protocol** не превращать в distributed transaction mechanism.
- **Definition cache** не усложнять file watchers/content hashing без потребности.
- **RuntimeGateway** не превращать в универсальную платформу для Kubernetes/Podman/remote contexts.

---

# 25. Открытые решения и defaults

| Вопрос | Рекомендуемый default |
|---|---|
| Главный UI object | Project profile, пока не опровергнуто исследованием |
| Standalone containers | Other containers / отдельная view |
| Auto-registration | Только новый unambiguous profile |
| Existing profile discovery | Никогда не auto-update |
| Compose name collision | Explicit conflict |
| Docker contexts | Один |
| Remote Docker | Не baseline |
| Verified platform | macOS |
| Linux/Windows | Не обещать до matrix |
| Missing explicit env file | Definition invalid |
| Registry writer | Один app instance |
| Docker Events | Отложить |
| Images | Убрать |
| Logs | Bounded snapshot first |
| Definition cache | profile change + foreground + manual + stale expiry |
| Database | Не вводить |
| RBAC | Не вводить |
| Plugin system | Не вводить |

---

# 26. Port semantics

В 2.0 binding должен хранить:

```text
host_ip
host_port
container_port
transport_protocol
```

Правила: всегда можно скопировать binding; auto-open только при известном web protocol; UDP не открывать как URL; non-loopback не подменять `127.0.0.1`; scheme override можно добавить позже. TCP сам по себе не означает HTTP.

---

# 27. Что не следует делать

- Не переносить name-only merge в новый слой типов.
- Не создавать универсальный `Resource<T>` framework.
- Не вводить event sourcing.
- Не переходить на SQLite только ради transactions.
- Не переписывать Docker Compose.
- Не начинать с visual redesign.

Сначала исправить identity, command authority, runtime context, hidden writes и lifecycle semantics.

---

# 28. Первый вертикальный срез

```text
Registry v2
+
stable profile ID
+
ID-based apply/stop/tear-down/restart
+
backend profile lookup
+
typed operation errors
```

Включает dual-reader v1/v2, lossless migration, новый `ProjectProfile`, frontend payload только с ID, pending state по ID, unit/application tests и real disposable Compose smoke test.

Это лучший первый slice, потому что он разрывает главный неправильный dependency: UI → persisted paths → Compose execution.

---

# 29. Вопросы к владельцам проекта

1. Должен ли зарегистрированный Compose environment быть главным продуктовым объектом, или Containers и Compose остаются равноправными workflows?
2. Нужна ли auto-registration без явного действия пользователя?
3. Разрешены ли два profiles с одинаковым `composeProjectName`, если directories различаются?
4. Нужна ли multi-context Docker support?
5. Нужна ли remote Docker support в baseline?
6. Должен ли Stop точно соответствовать `docker compose stop`, а `down` быть отдельной destructive action?
7. Нужна ли официальная Linux support в первой 2.0 release?
8. Нужны ли live-follow logs как core workflow?
9. Есть ли подтверждённый Images workflow?
10. Должна ли история auto-registration быть видна пользователю?

---

# 30. Итог

## Главная концептуальная проблема

CoLUI использует одну модель и одно имя для нескольких понятий с разными sources of truth и lifecycles:

```text
profile
+ Compose definition
+ runtime project
+ service
+ container instance
+ UI card
= ComposeProject
```

и:

```text
display label
+ persistent identity
+ Compose namespace
+ merge key
+ command target
= project.name
```

## Основная идея 2.0

```text
ProjectProfile         → registry
ProjectDefinition      → Compose files
ProjectRuntimeSnapshot → Docker daemon
RuntimeSession         → RuntimeGateway
Operation              → application use case
ProjectSummary         → derived UI projection
```

## Что нужно перестать делать

Использовать имя как identity; передавать backend paths из frontend; изменять registry во время query; смешивать definition/runtime; считать Bollard и CLI одним runtime без проверки; маскировать `down` словом Stop; добавлять product surface без готового сценария.

## Что должно стать проще

Command contracts, status model, recovery, persistence, discovery, polling, testing и ответ на вопрос «кто владеет этими данными?».

## Первые три шага

1. Registry schema v2 + stable `ProfileId` + lossless migration.
2. ID-based backend-resolved lifecycle + Stop/Tear down split.
3. `RuntimeGateway` с единой Docker session и daemon identity check.

---

# Приложение A. Целевая архитектура

```text
┌──────────────────────────────────────────────┐
│                  React UI                    │
│ view state · dialogs · projections · UX      │
└──────────────────────┬───────────────────────┘
                       │ typed IPC
┌──────────────────────▼───────────────────────┐
│              Application Use Cases           │
│ profiles · projects · containers · runtime   │
└───────┬──────────────┬──────────────┬────────┘
        │              │              │
        ▼              ▼              ▼
 ProjectProfile   ProjectDefinition  Operation
        │              │              │
        └───────┬──────┴───────┬──────┘
                │              │
        ┌───────▼──────┐ ┌────▼────────────┐
        │ProfileRegistry│ │ RuntimeGateway  │
        │ versioned JSON│ │ session owner   │
        └───────┬──────┘ └────┬──────┬─────┘
                │             │      │
                ▼             ▼      ▼
           filesystem      Bollard  docker compose
                               \      /
                                \    /
                              same daemon
                                  │
                                  ▼
                         RuntimeInventory
                                  │
                                  ▼
                         ProjectSummary DTO
```

# Приложение B. Основные source surfaces

- `README.md`
- `INSTALL.md`
- `docs/knowledge-base/product.md`
- `docs/knowledge-base/architecture.md`
- `docs/knowledge-base/frontend.md`
- `docs/knowledge-base/backend.md`
- `docs/knowledge-base/compose-catalog.md`
- `docs/knowledge-base/security.md`
- `docs/knowledge-base/testing.md`
- `docs/knowledge-base/decisions-and-limitations.md`
- `src/App.tsx`
- `src/hooks/useContainers.ts`
- `src/hooks/useComposeProjects.ts`
- `src/lib/invoke.ts`
- `src/lib/compose-command.ts`
- `src/lib/compose-service.ts`
- `src/types/docker.ts`
- `src-tauri/src/main.rs`
- `src-tauri/src/commands.rs`
- `src-tauri/src/docker.rs`
- `src-tauri/src/compose.rs`
- `src-tauri/src/compose_cli.rs`
- `src-tauri/src/projects.rs`
- `src-tauri/src/catalog.rs`
- `src-tauri/src/types.rs`

# Приложение C. Классификация сложности

| Элемент | Тип сложности | Решение |
|---|---|---|
| Docker API + Compose CLI | необходимая | сохранить, координировать |
| Persisted profile after down | продуктовая | сохранить |
| Name-only merge | ошибка модели | удалить |
| Hidden persistence from query | ошибка boundary | удалить |
| Two frontend pollers | случайная | объединить |
| Config expansion | необходимая | отделить от fast path |
| ProjectSource merged enum | ошибка модели | удалить |
| Hybrid ComposeService | ошибка модели | разделить |
| Images placeholder | гипотетическая | удалить |
| Multi-context support | гипотетическая будущая | отложить |
| Docker Events | optimization | отложить |
| SQLite | infrastructure without need | не вводить |
| Plugin architecture | hypothetical extensibility | не вводить |

---

**Критерий успеха CoLUI 2.0:** не количество новых технологий, а возможность однозначно ответить на четыре вопроса:

1. Что это за сущность?
2. Кто владеет её данными?
3. Где source of truth?
4. Какая команда и какой state transition допустимы сейчас?
