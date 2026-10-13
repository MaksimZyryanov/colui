# Current architecture

CoLUI is a modular monolith inside one Tauri desktop application.

```text
React UI
  -> typed Tauri IPC
  -> application use cases and ports
  -> domain models
  -> filesystem, Docker API, and Docker Compose adapters
```

## Repository layers

| Path | Responsibility |
|---|---|
| `crates/colui-domain` | Pure domain values, validation, and invariants |
| `crates/colui-app` | Use cases and adapter ports |
| `crates/colui-adapters` | Registry, Docker API, Compose process, inventory, and recovery adapters |
| `src-tauri` | Dependency wiring, Tauri commands, DTO mapping, desktop shell |
| `src/ipc` | TypeScript command wrappers and Zod decoding boundary |
| `src/features` | React feature views, hooks, and projections |
| `schemas` | Committed IPC JSON Schemas generated from Rust DTOs |

Dependency direction is `colui-domain <- colui-app <- colui-adapters <- colui-tauri`. Boundary scripts prevent infrastructure dependencies from leaking inward.

## Ownership

| Data or behavior | Owner |
|---|---|
| Profile identity and paths | Profile registry |
| Compose definition | Compose files and definition cache |
| Docker connection and API/CLI identity | `RuntimeGateway` |
| Runtime containers and associations | `InventoryCoordinator` |
| Operation exclusion | `OperationLockManager` |
| Discovery session state | `DiscoverySession` |
| UI state and presentation | React |

## Main flows

Profile lifecycle starts with profile ID, rereads registry under operation protection, verifies runtime session, invokes `docker compose` with backend-resolved arguments, then publishes one post-action inventory snapshot.

The post-action refresh uses an observation marker so it cannot reuse a Docker list started before the action. Retained observations may become stale without changing generation; the UI accepts those health updates, but only a newer observation can restore freshness.

Inventory refresh lists containers once through Docker API, normalizes them, groups Compose observations, derives registered-project and standalone-container projections, and publishes a monotonic generation. Queries do not write registry.

All IPC responses cross Rust DTO mapping and TypeScript Zod validation. Protocol drift fails contract tests instead of becoming empty UI state.

The shell's diagnostics observer also invalidates a profile's definition when a completed backend load changes its diagnostics. This repairs transient operation conflicts without per-card polling or a loop on unchanged reads. Container logs use the full fresh inventory, with session checks before and after reading.

Command arguments retain the Rust parameter envelope (`request` or `draft`). Project details expose registry tuple uniqueness separately from runtime issues. The UI associates containers by Compose name, normalized working directory, and ordered config files, and reevaluates observation ambiguity from current inventory. Actions require the current ready session and fresh observations.

Registration publishes the returned profile and exact discovered metadata together with the discovery handoff. Resource expansion belongs to the list, so replacing a candidate with its profile preserves expansion and sorted child rows. Session changes clear discovery metadata while registered rows keep their profile identity.

Detailed rationale lives in [design decisions](history/design-decisions.md). Historical specs under [`design/`](design/) contain implementation-level constraints and may describe state at their original dates.
