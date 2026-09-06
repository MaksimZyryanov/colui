# Implementation history

CoLUI 2.0 was delivered headless-first through five vertical iterations. Each iteration extended existing owners instead of replacing them.

## 1. Domain and registry

Established Rust workspace boundaries and core domain vocabulary. Added immutable profile IDs, revisions, validation, versioned JSON registry, atomic writes, lock handling, backup/recovery foundations, and one-time legacy import.

Result: durable project identity no longer depended on name or path.

## 2. Runtime gateway

Added one owner for Docker API and Compose CLI context. Implemented endpoint resolution, daemon fingerprint comparison, reconnect behavior, bounded process output, timeout termination, and process reap.

Result: Compose commands and inventory could no longer silently target different Docker daemons.

## 3. Typed IPC and usable UI

Added Tauri commands, Rust DTO schemas, TypeScript/Zod validation, profile forms, project lifecycle actions, typed errors, and explicit destructive confirmations.

Result: first runnable desktop slice used profile-ID-only lifecycle commands and worked with manually registered projects.

## 4. Coordinated inventory

Added immutable inventory generations, refresh coalescing, stale-data retention, backoff, definition cache, project projections, and operation locks. Frontend polling became one generation-aware data flow.

Result: runtime status became coherent and resistant to stale asynchronous responses.

## 5. Discovery and recovery

Added Compose discovery candidates and conflicts, optional auto-registration, diagnostics, reconnect/disconnect controls, explicit registry backup and restore, standalone container actions, bounded logs, and safe port actions.

Result: current baseline covers project registration, lifecycle, runtime observation, diagnostics, recovery, and standalone-container workflows.

Detailed design records remain in [`docs/design`](../design/).
