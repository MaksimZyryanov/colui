# Design decisions

These decisions converted findings from original architectural audit into CoLUI 2.0 constraints.

## Identity and authority

- `ProfileId` is immutable and independent from display name, Compose namespace, and paths.
- Lifecycle IPC accepts profile ID only. Backend resolves current profile data before execution.
- Frontend renders projections and never acts as authority for registry or runtime state.

## Separate lifecycles

- `ProjectProfile`, `ProjectDefinition`, and runtime inventory are separate models.
- Service definitions and container instances are separate.
- Definition validity, runtime presence, runtime activity, and active operation remain independent state axes.

## Runtime ownership

- One `RuntimeGateway` owns Docker API and Compose CLI control planes.
- Compose actions require a verified shared daemon context.
- Compose child processes use explicit argv, bounded output, deadlines, termination, and reap.
- Stop, Tear down, and Remove profile are distinct operations.

## Persistence and discovery

- Registry remains versioned JSON with locking, atomic replacement, backup, and explicit recovery.
- Queries never mutate registry.
- Discovery candidates are runtime observations, not durable profiles.
- Existing profiles are never silently updated from discovery.

## Inventory and errors

- One coordinator publishes immutable, generation-numbered inventory snapshots.
- Failed refresh retains last successful snapshot with freshness metadata.
- IPC uses typed errors and runtime-validated DTOs.

## Deliberate non-goals

Version 2 baseline excludes remote or multiple Docker contexts, Docker Events, live-follow logs, Images workflow, SQLite, RBAC, plugins, Kubernetes/Podman abstraction, and custom Compose orchestration. macOS with Docker Desktop or Colima is verified target.
