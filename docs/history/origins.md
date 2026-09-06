# Origins of CoLUI 2.0

## CoLUI 0.1

Original CoLUI was a local macOS desktop utility for Docker containers and Docker Compose projects. React and TypeScript provided UI, Tauri hosted desktop shell, Rust handled backend work, Bollard accessed Docker API, and external `docker compose` owned Compose semantics.

Stack was suitable. Domain model was not. One `ComposeProject` represented several concepts with different lifecycles:

- saved project configuration;
- Compose definition;
- observed runtime project;
- services and container instances;
- UI card and command target.

Project name also acted as display label, persistent identity, Compose namespace, merge key, and pending-action key. This made rename and collision behavior unsafe and forced frontend to resend backend-owned paths in lifecycle commands.

Other confirmed problems included queries writing discovered projects into durable storage, independent Docker API and Compose CLI contexts, conflated Stop and Tear down operations, string-only IPC errors, independent polling loops, and hidden or ambiguous project states.

## Architectural audit

Audit completed on 31 August 2026. Its central recommendation was not a big-bang technology rewrite, but a clean model built as a local modular monolith:

```text
ProjectProfile         -> durable registry
ProjectDefinition      -> Compose files
ProjectRuntimeSnapshot -> Docker daemon
RuntimeSession         -> verified Docker context
Operation              -> application use case
ProjectSummary         -> derived UI projection
```

Three changes carried most value:

1. Introduce immutable profile IDs and backend-resolved lifecycle commands.
2. Give one `RuntimeGateway` ownership of Docker API and Compose CLI context.
3. Separate persisted profiles, Compose definitions, and runtime inventory.

Project then restarted as CoLUI 2.0 and implemented this model through five vertical iterations. See [implementation history](iterations.md) and [original master design](../design/2026-09-01-colui-2-design.md).
