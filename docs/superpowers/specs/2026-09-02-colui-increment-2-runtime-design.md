# CoLUI Increment 2: RuntimeGateway Design

**Status:** approved conversational design; written spec awaiting user review
**Date:** 2026-09-02
**Source:** `docs/superpowers/specs/2026-09-01-colui-2-design.md`, `CoLUI_architecture_audit_v2.md`

## 1. Scope and decisions

Increment 2 adds the runtime boundary for one local Docker context. It covers `RuntimeSession`, concrete `RuntimeGateway`, endpoint resolution, Bollard API access, Docker Compose CLI invocation, process ownership, bounded output, reconnect, and hermetic and feature-gated real-Docker tests.

It excludes frontend, Tauri IPC, `InventoryCoordinator`, definition cache, Discovery, lifecycle UI, Docker Events, container logs, remote or multi-context support, and implementation of later-increment use cases.

Decisions fixed for this increment:

- `RuntimeGateway` is the sole owner of API and CLI control planes.
- `connect_runtime()` is idempotent and is also reconnect; no duplicate `reconnect()` port is added.
- `ContextMismatch` is a distinct recoverable session state.
- Fingerprints compare `daemon_id`, `server_version`, `os_type`, and `architecture` exactly.
- A long-lived Bollard client is reused until disconnect or reconnect.
- `ComposeProcessRunner` has configurable SIGTERM grace, default five seconds.
- Compose concurrency starts with one global permit.
- Process output exposes per-stream truncation flags.
- Fake process tests use a compiled Rust executable, not a shell script.
- All runtime ports and adapters are async.

The dependency direction remains `colui-domain <- colui-app <- colui-adapters`. `colui-app` defines runtime port traits and application-facing value contracts. The concrete gateway, Bollard client, Compose adapter, endpoint resolver, and process runner are implemented in `colui-adapters`; this preserves the rule that application code knows neither Bollard nor process APIs while retaining one concrete owner for both control planes.

## 2. Runtime model

### Domain values

`colui-domain` owns runtime-independent values:

```rust
pub struct DockerEndpoint(String);

pub struct DaemonFingerprint {
    pub daemon_id: String,
    pub server_version: String,
    pub os_type: String,
    pub architecture: String,
}

pub struct RuntimeSessionId(Uuid);
```

The domain may serialize and validate these values but must not import Bollard, Tokio, Tauri, filesystem, or process APIs. Existing domain `ContainerInstance` and related snapshot types remain the application contract for normalized API results.

### Session state

```rust
pub enum RuntimeSessionState {
    Disconnected,
    Connecting,
    Ready(SessionContext),
    ContextMismatch(MismatchDetails),
    Failed(AppError),
}

pub struct SessionContext {
    pub session_id: RuntimeSessionId,
    pub endpoint: DockerEndpoint,
    pub daemon_fingerprint: DaemonFingerprint,
    pub connected_at: Timestamp,
}

pub struct MismatchDetails {
    pub endpoint: DockerEndpoint,
    pub api_fingerprint: DaemonFingerprint,
    pub cli_fingerprint: DaemonFingerprint,
}
```

The concrete Bollard client and CLI environment are private adapter state associated with `Ready`; they are not serialized into `SessionContext` or exposed through application ports.

Transitions:

- `Disconnected -> Connecting` on `connect_runtime()`.
- `Connecting -> Ready` when API and CLI fingerprints match.
- `Connecting -> ContextMismatch` when both respond but fingerprints differ.
- `Connecting -> Failed` on endpoint, API, CLI, parsing, or fingerprint failure.
- `Ready -> Disconnected` on `disconnect_runtime()`.
- `ContextMismatch -> Connecting` and `Failed -> Connecting` on a later `connect_runtime()`.

Profile access does not depend on session state. API reads require an available client and may be allowed for `Ready` and `ContextMismatch`; Compose operations require `Ready` exactly. No operation creates a second client or invokes Docker directly.

## 3. Endpoint and connection flow

Endpoint resolution has this priority:

1. Explicit endpoint preference supplied by the application.
2. `DOCKER_HOST` from the inherited environment.
3. macOS default Unix socket `unix:///var/run/docker.sock`.

One resolved endpoint configures both control planes. The gateway connects Bollard, calls the API `info` endpoint, builds the CLI environment, invokes `docker info`, parses the CLI fingerprint, then compares all four fingerprint fields. `Ready` is published only after exact equality.

`connect_runtime()` always reruns resolution and both fingerprint checks. It replaces old adapter state only after the new connection attempt has reached a terminal result. A successful reconnect can transition `ContextMismatch` or `Failed` to `Ready` without restarting the application.

Fingerprint comparison uses canonical trimmed strings returned by the two control planes; no semver comparison or suffix stripping is performed. Thus `20.10.17` and `20.10.17-ce` do not match. The CLI adapter requests the server version from the daemon rather than comparing client versions, so equivalent API and CLI observations from one daemon must produce the same canonical value.

CLI environment construction begins with inherited `std::env::vars()`. Ordinary environment such as `PATH`, `HOME`, locale, user variables, and credential-helper variables remains available. The context-control allowlist is explicit: `DOCKER_HOST`, `DOCKER_CONTEXT`, `DOCKER_TLS_VERIFY`, `DOCKER_CERT_PATH`, `DOCKER_API_VERSION`, `DOCKER_CLI_EXPERIMENTAL`, `DOCKER_CONFIG`, `COMPOSE_FILE`, `COMPOSE_PROJECT_NAME`, `COMPOSE_PROFILES`, `COMPOSE_PROJECT_DIRECTORY`, `COMPOSE_PATH_SEPARATOR`, `COMPOSE_ENV_FILES`, `COMPOSE_DISABLE_ENV_FILE`, `COMPOSE_PARALLEL_LIMIT`, and `COMPOSE_MENU`. `COMPOSE_MENU` is a documented Docker Compose V2 UX variable controlling its Docker Desktop/watch/debug navigation menu; it is included to prevent inherited interactive behavior in non-interactive commands. `DOCKER_HOST` is set to the resolved endpoint; `DOCKER_CONTEXT` is cleared; TLS, certificate, API-version, and experimental settings are cleared or set by the session; Compose file, project, profile, directory, env-file, parallelism, and menu settings are cleared from the inherited environment and supplied only by backend invocation policy. `DOCKER_CONFIG` is preserved as the inherited Docker config path so credential helpers continue to work; it cannot select an endpoint because `DOCKER_CONTEXT` is cleared and `DOCKER_HOST` is explicit. `DOCKER_BUILDKIT` is explicitly preserved as a build-behavior setting and is not treated as endpoint identity. Variables outside this allowlist are ordinary inherited environment and are not interpreted as runtime context. The full environment is never included in diagnostic details. Reference: Docker Compose predefined environment variables, https://docs.docker.com/compose/how-tos/environment-variables/envvars/.

Connection failures map to `runtime_unavailable` for unavailable local runtime and `runtime_connection_failed` for connection, API fingerprint, CLI execution, or CLI parsing failures. A fingerprint mismatch is not collapsed into either error: it enters `ContextMismatch` and retains both fingerprints for Diagnostics.

## 4. Application ports

`colui-app` defines async ports using domain/application types only. Ports use `&self`; no runtime port requires a mutable borrow across an await, and callers cannot hold a blocking lock through a port call:

```rust
#[async_trait]
pub trait DockerApi: Send + Sync {
    async fn list_containers(&self) -> Result<Vec<ContainerInstance>, AppError>;
    async fn inspect_container(
        &self,
        container_id: &ContainerId,
    ) -> Result<ContainerDetails, AppError>;
}

#[async_trait]
pub trait ComposeRunner: Send + Sync {
    async fn invoke(
        &self,
        invocation: ComposeInvocation,
    ) -> Result<ComposeProcessResult, AppError>;
}
```

`ComposeInvocation` contains executable, separate argv, backend-resolved working directory, explicit environment, and an absolute monotonic `deadline: Instant` measured by the runner's clock. It is a data contract, not a shell command string. `ComposeProcessResult` contains exit status, decoded stdout/stderr tails, per-stream truncation flags, timeout state, and duration.

The gateway implements these ports in `colui-adapters`. Application use cases receive port references and do not construct Bollard clients, child processes, shell commands, endpoint environment, or profile paths from IPC input. Future lifecycle use cases perform profile lookup before constructing backend-resolved invocations.

## 5. RuntimeGateway ownership

The concrete adapter has one long-lived owner:

```rust
pub struct RuntimeGateway {
    session: RuntimeSession,
    client: Option<Docker>,
    compose_runner: ComposeProcessRunner,
    compose_gate: Arc<Semaphore>,
}
```

`client` is present only for a verified usable session. `compose_runner` owns process execution configuration. `compose_gate` starts with one permit and serializes all Compose CLI calls globally. The gateway alone decides readiness and enforces gates through `require_ready_client()` and `require_ready_context()`.

API operations use the API-verified client captured during the current connection attempt. Read-only API inventory can continue in `ContextMismatch` only when that client has completed the API fingerprint call successfully and remains bound to the mismatch's endpoint and API fingerprint; each read is tagged with the current session ID and API fingerprint, and a reconnect/disconnect invalidates that client before publication. No CLI-derived data is used to authorize these reads. Compose operations return `runtime_context_mismatch` before spawn. Disconnected and failed sessions return the corresponding runtime error. State synchronization uses async coordination and short state transitions; it must not hold a blocking lock or an exclusive mutable borrow across long network/process awaits.

## 6. ComposeProcessRunner

```rust
pub struct TerminationConfig {
    pub grace_period: Duration,
}

pub struct ComposeProcessRunner {
    pub termination: TerminationConfig,
}
```

The default grace period is five seconds; construction permits an explicit bounded configuration. Invocation uses executable plus `args`, `current_dir`, `envs`, piped stdout/stderr, and no shell. Backend constructs arguments such as `docker compose -f compose.yml -f compose.local.yml up -d`; values containing shell metacharacters remain ordinary argv values.

Execution sequence:

1. Spawn child in a new process group.
2. Concurrently drain stdout and stderr into independent 64 KiB byte ring buffers.
3. Wait until the invocation deadline.
4. On deadline, send SIGTERM to the process group.
5. Wait configured grace period.
6. Send SIGKILL to the process group if it has not exited.
7. Wait and reap the direct child before returning. After SIGKILL, reap first uses nonblocking `try_wait()` polling with a bounded five-second confirmation deadline. If confirmation does not complete, the runner records a termination diagnostic and performs a blocking OS wait as the mandatory reap fallback; it never returns a successful result while the direct child remains unreaped. An OS wait error returns a typed timeout/termination diagnostic after the reap attempt.
8. Join output drains, retain newest bytes, decode UTF-8 with replacement characters, and return typed result.

Each ring discards oldest bytes once full and sets its own `truncated` flag. The two buffers are independent and have a combined maximum retained payload of 128 KiB, excluding small fixed bookkeeping and decoder allocation. Buffering is byte-based, so UTF-8 boundaries are not assumed during collection. Spawn errors and termination failures become typed Compose/runtime diagnostics; non-zero exit becomes `compose_failed`; deadline expiration returns `operation_timeout` with `timed_out = true` after reap. A process-group signal failure must not skip the bounded wait/reap attempt or its blocking fallback. If OS reap itself fails, diagnostics identify that termination could not be confirmed; the implementation must still make the SIGKILL attempt and never silently report successful cleanup.

## 7. Test design

Hermetic adapter tests use a compiled Rust fake executable. Its protocol is controlled through environment variables:

- `FAKE_EXEC_MODE=success|failure|sleep|spawn-child|signal-aware`
- `FAKE_STDOUT_BYTES=<count>`
- `FAKE_STDERR_BYTES=<count>`
- `FAKE_EXIT_CODE=<integer>`
- `FAKE_SLEEP_MS=<milliseconds>`
- `FAKE_CHILD_PID_FILE=<path>`
- `FAKE_ARGS_FILE=<path>`
- `FAKE_CWD_FILE=<path>`
- `FAKE_ENV_FILE=<path>`

The fixture records argv, cwd, and selected environment, emits deterministic bytes, can delay beyond deadline, can create a child process, and can ignore SIGTERM to force SIGKILL escalation. Tests must verify endpoint priority; inherited and controlled environment; matching and mismatching fingerprints; reconnect; profile availability offline; API mapping; readiness gates; exact argv/cwd; shell metacharacter literalness; independent 64 KiB newest-byte retention; UTF-8 replacement decoding; exit mapping; process-group termination; mandatory reap; and semaphore serialization.

Real-Docker tests are behind the `docker-tests` feature and run separately with `cargo test -p colui-adapters --features docker-tests`. The disposable fixture creates a temporary Compose project, connects, verifies fingerprints, applies, observes containers through the API, stops, reapplies, tears down, and removes temporary resources. Matrix validation targets Docker Desktop and Colima on macOS. Tests must skip clearly when no Docker daemon is available rather than make hermetic workspace tests depend on Docker.

`bash scripts/check-boundaries.sh` is the repository dependency-boundary gate: it rejects forbidden Bollard/Tauri/Tokio/filesystem/process dependencies in `colui-domain`, Bollard/Tauri in `colui-app`, and Tauri in `colui-adapters`. It inspects Cargo manifest dependency keys, including aliased and dotted dependency tables, and is not a substitute for compilation.

Required verification commands:

```bash
cargo fmt --all -- --check
cargo test --workspace
cargo test -p colui-adapters --features docker-tests
bash scripts/check-boundaries.sh
git diff --check
```

## 8. Acceptance and non-goals

Increment 2 is accepted when one concrete gateway owns verified API and CLI control planes; exact four-field fingerprint equality gates `Ready`; mismatch is visible, recoverable, and blocks Compose; reconnect works without restart; endpoint, cwd, argv, and environment are backend-controlled; process groups terminate with configurable SIGTERM grace and mandatory reap; output is independently bounded to newest 64 KiB; hermetic fake-executable tests pass; and feature-gated real-Docker checks cover Docker Desktop and Colima.

No frontend or Tauri IPC is added. No InventoryCoordinator, definition cache, Discovery, lifecycle UI, Docker Events, remote/multi-context support, container log API, speculative generic runtime abstraction, or compatibility layer is added.
