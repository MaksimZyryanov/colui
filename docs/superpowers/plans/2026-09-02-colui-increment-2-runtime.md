# CoLUI Increment 2 RuntimeGateway Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build one verified local Docker runtime session with coordinated Bollard and Compose CLI control planes, hard-owned child processes, and tested reconnect/mismatch behavior.

**Architecture:** `colui-domain` contains runtime-independent endpoint, fingerprint, session, and process-result values. `colui-app` defines async runtime ports and use-case-facing invocation contracts. `colui-adapters` owns endpoint resolution, Bollard, Compose CLI, `ComposeProcessRunner`, and concrete `RuntimeGateway`; one gateway owns the verified client, CLI environment, process runner, and global Compose semaphore.

**Tech Stack:** Rust stable, Cargo workspace, Tokio, Bollard, nix, serde, serde_json, uuid, tempfile; compiled Rust fake executable; Docker Desktop and Colima on macOS for feature-gated tests.

**Spec:** `docs/superpowers/specs/2026-09-02-colui-increment-2-runtime-design.md`

## Global Constraints

- `colui-domain` must not depend on Bollard, Tokio, Tauri, filesystem, or process APIs.
- `colui-app` must not depend on Bollard or Tauri and must define async ports with `&self`.
- `colui-adapters` must not depend on Tauri.
- `RuntimeGateway` is sole owner of API and CLI control planes.
- `Ready` requires exact equality of canonical `daemon_id`, `server_version`, `os_type`, and `architecture` strings.
- `ContextMismatch` preserves both fingerprints, allows only current API-verified read-only API access, and blocks Compose operations.
- `connect_runtime()` always reruns endpoint resolution and both fingerprint checks; it is reconnect and needs no separate reconnect port.
- Endpoint priority is explicit preference, inherited `DOCKER_HOST`, then `unix:///var/run/docker.sock`.
- Ordinary inherited environment is preserved; context-control variables are explicitly cleared or set. `DOCKER_CONFIG` and `DOCKER_BUILDKIT` are preserved; `DOCKER_HOST` is explicit and `DOCKER_CONTEXT` is cleared.
- Compose invocation uses separate executable and argv, backend-resolved cwd, explicit environment, and an absolute monotonic deadline; shell interpolation is forbidden.
- stdout and stderr use independent newest-byte 64 KiB rings; retained payload is at most 128 KiB total, with per-stream truncation flags and post-collection UTF-8 replacement decoding.
- Timeout handling is SIGTERM, configurable grace period defaulting to 5 seconds, SIGKILL escalation, bounded reap confirmation, and blocking wait fallback; no successful result is returned while the direct child remains unreaped.
- Global Compose concurrency starts at one permit.
- Fake executable tests are hermetic and do not require Docker or network access.
- Real Docker tests use the `docker-tests` feature and target Docker Desktop and Colima on macOS.
- Increment 2 does not add frontend, Tauri IPC, InventoryCoordinator, definition cache, Discovery, lifecycle UI, Docker Events, container logs, remote/multi-context support, or speculative compatibility abstractions.

---

## File Map

- Leave `Cargo.toml` unchanged: runtime dependencies remain crate-local.
- Leave `crates/colui-domain/Cargo.toml` unchanged: existing serde and UUID dependencies are sufficient.
- Modify `crates/colui-domain/src/lib.rs`: export runtime values.
- Create `crates/colui-domain/src/runtime.rs`: endpoint, fingerprint, session, invocation/result values and state enums with no runtime APIs.
- Leave `crates/colui-app/Cargo.toml` unchanged: existing domain dependency and dev-only Tokio are sufficient.
- Modify `crates/colui-app/src/lib.rs`: export runtime ports and contracts.
- Create `crates/colui-app/src/runtime.rs`: `DockerApi`, `ComposeRunner`, runtime use-case-facing contracts and error gates.
- Create `crates/colui-app/tests/runtime.rs`: fake-port tests for port semantics and readiness decisions.
- Modify `crates/colui-adapters/Cargo.toml`: Bollard, nix signal/process-group support, Tokio process/io/sync features, and `docker-tests` feature.
- Modify `crates/colui-adapters/src/lib.rs`: export runtime adapter module.
- Create `crates/colui-adapters/src/runtime/mod.rs`: concrete gateway wiring and public adapter exports.
- Create `crates/colui-adapters/src/runtime/endpoint.rs`: explicit/env/default endpoint resolver and CLI environment builder.
- Create `crates/colui-adapters/src/runtime/fingerprint.rs`: Bollard and `docker info` fingerprint extraction.
- Create `crates/colui-adapters/src/runtime/docker_api.rs`: Bollard adapter mapping into domain values.
- Create `crates/colui-adapters/src/runtime/compose.rs`: backend argv construction and Compose adapter.
- Create `crates/colui-adapters/src/runtime/process.rs`: process-group runner, rings, timeout, termination, and reap.
- Create `crates/colui-adapters/tests/runtime.rs`: hermetic endpoint, gateway, adapter, and process tests.
- Create `crates/colui-adapters/tests/fixtures/fake_compose.rs`: compiled fake executable protocol.
- Create `crates/colui-adapters/tests/docker.rs`: feature-gated disposable real-Docker tests.
- Leave `scripts/check-boundaries.sh` unchanged; run its existing manifest dependency-key checks after each crate dependency change.

### Task 1: Add runtime-independent domain contracts

**Files:**
- Modify: `crates/colui-domain/src/lib.rs`
- Create: `crates/colui-domain/src/runtime.rs`
- Test: `crates/colui-domain/tests/runtime.rs`

**Interfaces:**
- Consumes: existing `AppError`, `ContainerId`, `ContainerInstance`, `ProfileId`, `Timestamp`, and serde/UUID conventions.
- Produces: `DockerEndpoint`, `DaemonFingerprint`, `RuntimeSessionId`, `RuntimeSessionState`, `SessionContext`, `MismatchDetails`, and `ContainerDetails`.

- [ ] **Step 1: Write failing value and invariant tests**

```rust
#[test]
fn endpoint_and_fingerprint_round_trip_without_runtime_dependencies() {
    let endpoint = DockerEndpoint::try_from("unix:///var/run/docker.sock").unwrap();
    let fingerprint = DaemonFingerprint::new("id", "20.10.17", "linux", "aarch64");
    assert_eq!(endpoint.as_str(), "unix:///var/run/docker.sock");
    assert_eq!(fingerprint.server_version(), "20.10.17");
}

#[test]
fn fingerprint_versions_are_canonical_exact_values() {
    assert_ne!(
        DaemonFingerprint::new("id", "20.10.17", "linux", "arm64"),
        DaemonFingerprint::new("id", "20.10.17-ce", "linux", "arm64"),
    );
}

#[test]
fn mismatch_keeps_both_observed_fingerprints() {
    let mismatch = MismatchDetails::new(
        DockerEndpoint::try_from("unix:///var/run/docker.sock").unwrap(),
        DaemonFingerprint::new("api", "20.10.17", "linux", "arm64"),
        DaemonFingerprint::new("cli", "20.10.17", "linux", "arm64"),
    );
    assert_ne!(mismatch.api_fingerprint(), mismatch.cli_fingerprint());
}
```

- [ ] **Step 2: Run focused test and verify failure**

Run: `cargo test -p colui-domain --test runtime`

Expected: FAIL because runtime module and types do not exist.

- [ ] **Step 3: Implement validated runtime values**

Implement constructors/accessors with `Clone`, `Debug`, `Eq`, `Hash`, `Serialize`, and `Deserialize` where valid. `DockerEndpoint` rejects empty values. `DaemonFingerprint` stores four trimmed strings and compares exact strings. `RuntimeSessionState` has `Disconnected`, `Connecting`, `Ready(SessionContext)`, `ContextMismatch(MismatchDetails)`, and `Failed(AppError)`. Define `ContainerDetails { instance: ContainerInstance, labels: BTreeMap<String, String> }` as the normalized inspect result. Do not add `Instant`, process paths, process configuration, or runtime dependencies to domain.

- [ ] **Step 4: Run domain tests and boundary gate**

Run: `cargo test -p colui-domain --test runtime && bash scripts/check-boundaries.sh`

Expected: PASS; domain manifest contains no forbidden runtime dependency.

- [ ] **Step 5: Commit**

```bash
git add crates/colui-domain
git commit -m "feat(domain): add runtime contracts"
```

### Task 2: Define application runtime ports

**Files:**
- Modify: `crates/colui-app/Cargo.toml`
- Modify: `crates/colui-app/src/lib.rs`
- Create: `crates/colui-app/src/runtime.rs`
- Create: `crates/colui-app/tests/runtime.rs`

**Interfaces:**
- Consumes: Task 1 domain contracts and existing typed errors.
- Produces: `ComposeInvocation`, `ComposeProcessResult`, async `DockerApi`, async `ComposeRunner`, `RuntimeConnector`, `RuntimeStateReader`, and port-level readiness/error behavior.

- [ ] **Step 1: Write failing fake-port tests**

```rust
#[tokio::test]
async fn ports_use_shared_borrow_and_do_not_construct_runtime_clients() {
    let fake = FakeRuntime::ready();
    let containers = DockerApi::list_containers(&fake).await.unwrap();
    assert_eq!(containers.len(), 1);
}

#[tokio::test]
async fn compose_runner_accepts_structured_invocation() {
    let fake = FakeRuntime::ready();
    let result = ComposeRunner::invoke(&fake, fake_invocation()).await.unwrap();
    assert_eq!(result.exit_code(), Some(0));
}

#[test]
fn process_result_exposes_independent_truncation_and_timeout() {
    let result = ComposeProcessResult::timed_out("tail", "error", true, false, Duration::from_secs(1));
    assert!(result.timed_out());
    assert!(result.stdout_truncated());
    assert!(!result.stderr_truncated());
}
```

- [ ] **Step 2: Run test and verify failure**

Run: `cargo test -p colui-app --test runtime`

Expected: FAIL because runtime ports and fake fixture do not exist.

- [ ] **Step 3: Implement async ports and contracts**

Define `ComposeInvocation { executable: PathBuf, args: Vec<String>, working_directory: PathBuf, environment: BTreeMap<String, String>, deadline: std::time::Instant }` and `ComposeProcessResult { exit_code: Option<i32>, stdout: String, stderr: String, stdout_truncated: bool, stderr_truncated: bool, timed_out: bool, duration: Duration }`. Define `DockerApi: Send + Sync` with `async fn list_containers(&self) -> Result<Vec<ContainerInstance>, AppError>` and `async fn inspect_container(&self, container_id: &ContainerId) -> Result<ContainerDetails, AppError>`. Define `ComposeRunner: Send + Sync` with `async fn invoke(&self, invocation: ComposeInvocation) -> Result<ComposeProcessResult, AppError>`. Define `RuntimeConnector` with async `connect_runtime(&self, preference: Option<DockerEndpoint>)` and `disconnect_runtime(&self)`, plus `RuntimeStateReader::session_state(&self)`. Use boxed `Future` aliases matching existing `ProfileReader` style instead of adding `async-trait`; every method uses `&self`. Keep Bollard and OS process types out of app.

- [ ] **Step 4: Run app tests and boundary gate**

Run: `cargo test -p colui-app --test runtime && bash scripts/check-boundaries.sh`

Expected: PASS; app manifest contains no Bollard or Tauri dependency.

- [ ] **Step 5: Commit**

```bash
git add crates/colui-app
git commit -m "feat(app): define runtime ports"
```

### Task 3: Implement endpoint resolution and fingerprint adapters

**Files:**
- Modify: `crates/colui-adapters/Cargo.toml`
- Modify: `crates/colui-adapters/src/lib.rs`
- Create: `crates/colui-adapters/src/runtime/mod.rs`
- Create: `crates/colui-adapters/src/runtime/endpoint.rs`
- Create: `crates/colui-adapters/src/runtime/fingerprint.rs`
- Test: `crates/colui-adapters/tests/runtime.rs`

**Interfaces:**
- Consumes: Task 1 domain values and Task 2 port contracts.
- Produces: `EndpointPreference`, `resolve_endpoint`, `build_cli_environment`, Bollard fingerprint mapping, and CLI `docker info` fingerprint parsing.

- [ ] **Step 1: Write resolver and parser tests**

```rust
#[test]
fn endpoint_resolution_prefers_explicit_then_docker_host_then_socket() {
    assert_eq!(resolve_endpoint(Some("unix:///explicit"), Some("unix:///env")).unwrap().as_str(), "unix:///explicit");
    assert_eq!(resolve_endpoint(None, Some("unix:///env")).unwrap().as_str(), "unix:///env");
    assert_eq!(resolve_endpoint(None, None).unwrap().as_str(), "unix:///var/run/docker.sock");
}

#[test]
fn cli_environment_clears_context_and_compose_selection() {
    let env = build_cli_environment("unix:///resolved", inherited_fixture_env());
    assert_eq!(env.get("DOCKER_HOST").unwrap(), "unix:///resolved");
    assert!(!env.contains_key("DOCKER_CONTEXT"));
    assert!(!env.contains_key("COMPOSE_FILE"));
    assert_eq!(env.get("DOCKER_CONFIG").unwrap(), "/tmp/docker-config");
}

#[test]
fn cli_info_parser_extracts_exact_server_fingerprint() {
    let output = "ID: abc\nServer Version: 20.10.17\nOSType: linux\nArchitecture: aarch64\n";
    assert_eq!(parse_cli_fingerprint(output).unwrap().server_version(), "20.10.17");
}
```

- [ ] **Step 2: Run focused tests and verify failure**

Run: `cargo test -p colui-adapters --test runtime endpoint_resolution`

Expected: FAIL because runtime adapter modules do not exist.

- [ ] **Step 3: Implement endpoint and environment policy**

Resolve explicit preference, then supplied `DOCKER_HOST`, then macOS socket. Start environment from inherited variables. Set `DOCKER_HOST`; remove `DOCKER_CONTEXT`, TLS/certificate/API-version/experimental variables unless session explicitly supplies them; remove inherited Compose selection and behavior variables listed in spec; preserve `DOCKER_CONFIG` and `DOCKER_BUILDKIT`; preserve ordinary variables. Never log full environment.

- [ ] **Step 4: Implement fingerprint parsing and mapping**

Map Bollard `SystemInfo` values to the four domain fields. Parse CLI server-side fields, trim values, reject missing/ambiguous fields, and compare exact canonical strings. Do not strip suffixes or semver-normalize.

- [ ] **Step 5: Run adapter tests and boundary gate**

Run: `cargo test -p colui-adapters --test runtime endpoint_resolution && cargo test -p colui-adapters --test runtime cli_info_parser && bash scripts/check-boundaries.sh`

Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add crates/colui-adapters Cargo.lock
git commit -m "feat(runtime): resolve endpoint and fingerprints"
```

### Task 4: Implement ComposeProcessRunner

**Files:**
- Modify: `crates/colui-adapters/Cargo.toml`
- Create: `crates/colui-adapters/src/runtime/process.rs`
- Create: `crates/colui-adapters/tests/fixtures/fake_compose.rs`
- Test: `crates/colui-adapters/tests/runtime.rs`

**Interfaces:**
- Consumes: `ComposeInvocation`, `ComposeProcessResult`, `TerminationConfig`.
- Produces: `ComposeProcessRunner::run`, independent byte rings, compiled fake executable protocol, and typed process results.

- [ ] **Step 1: Write failing process tests**

```rust
#[tokio::test]
async fn runner_preserves_newest_bytes_per_stream_and_marks_truncation() {
    let result = runner().run(fake_invocation("success", 100_000, 90_000)).await.unwrap();
    assert_eq!(result.stdout().len(), 64 * 1024);
    assert_eq!(result.stderr().len(), 64 * 1024);
    assert!(result.stdout_truncated());
    assert!(result.stderr_truncated());
}

#[tokio::test]
async fn runner_uses_argv_cwd_and_env_without_shell_interpolation() {
    runner().run(fake_invocation_with_arg("literal;$(touch SHOULD_NOT_EXIST)")).await.unwrap();
    assert_eq!(read_recorded_args()[1], "literal;$(touch SHOULD_NOT_EXIST)");
    assert!(!Path::new("SHOULD_NOT_EXIST").exists());
}

#[tokio::test]
async fn runner_terminates_process_group_and_reaps_after_timeout() {
    let result = runner_with_grace(Duration::from_millis(50))
        .run(fake_invocation_mode("signal-aware"))
        .await
        .unwrap();
    assert!(result.timed_out());
    assert!(recorded_child_is_gone());
}
```

- [ ] **Step 2: Run test and verify failure**

Run: `cargo test -p colui-adapters --test runtime runner_`

Expected: FAIL because runner and fake executable do not exist.

- [ ] **Step 3: Implement compiled fake executable protocol**

The fixture reads `FAKE_EXEC_MODE=success|failure|sleep|spawn-child|signal-aware`, `FAKE_STDOUT_BYTES`, `FAKE_STDERR_BYTES`, `FAKE_EXIT_CODE`, `FAKE_SLEEP_MS`, `FAKE_CHILD_PID_FILE`, `FAKE_ARGS_FILE`, `FAKE_CWD_FILE`, and `FAKE_ENV_FILE`. It records argv/cwd/selected environment, emits deterministic bytes, can sleep past deadline, spawns a child, and ignores SIGTERM in `signal-aware` mode. Compile it from the integration test with `rustc --edition 2021 tests/fixtures/fake_compose.rs -o <tempdir>/fake-compose`, cache the resulting path for that test process, and invoke that path directly; never build or invoke it through a shell.

- [ ] **Step 4: Implement byte ring and UTF-8 result handling**

Drain stdout and stderr concurrently. Each ring retains newest 64 KiB and independently marks discard. Decode only after collection with `String::from_utf8_lossy`; retain at most 128 KiB output payload before decoding.

- [ ] **Step 5: Implement process group lifecycle**

Spawn executable with separate args, cwd, env, piped streams, and a new process group. Use absolute monotonic deadline. On expiry send SIGTERM to the group, wait configured grace, send SIGKILL if still running, then `try_wait()` until a five-second reap confirmation deadline. If confirmation fails, perform blocking wait fallback and return typed termination diagnostics on OS wait failure. Always join drain tasks.

- [ ] **Step 6: Run process tests**

Run: `cargo test -p colui-adapters --test runtime runner_`

Expected: PASS, including argv/cwd/env, output limits, UTF-8 replacement, exit mapping, timeout, process group termination, escalation, and reap.

- [ ] **Step 7: Commit**

```bash
git add crates/colui-adapters/Cargo.toml crates/colui-adapters/src/runtime/process.rs crates/colui-adapters/tests
git commit -m "feat(runtime): own compose child processes"
```

### Task 5: Implement Bollard adapter and concrete RuntimeGateway

**Files:**
- Create: `crates/colui-adapters/src/runtime/docker_api.rs`
- Create: `crates/colui-adapters/src/runtime/compose.rs`
- Modify: `crates/colui-adapters/src/runtime/mod.rs`
- Test: `crates/colui-adapters/tests/runtime.rs`

**Interfaces:**
- Consumes: endpoint/fingerprint helpers, process runner, domain values, and app ports.
- Produces: `DockerApiAdapter`, Compose argv adapter, and `RuntimeGateway` implementing `DockerApi`, `ComposeRunner`, and state access.

- [ ] **Step 1: Write failing gateway tests**

```rust
#[tokio::test]
async fn matching_fingerprints_enter_ready_and_reuse_one_client() {
    let gateway = gateway_with_fake_control_planes("same");
    assert!(matches!(gateway.connect_runtime().await.unwrap(), RuntimeSessionState::Ready(_)));
    assert_eq!(gateway.created_client_count(), 1);
}

#[tokio::test]
async fn mismatch_allows_current_api_read_but_blocks_compose() {
    let gateway = gateway_with_fake_control_planes("mismatch");
    assert!(matches!(gateway.connect_runtime().await.unwrap(), RuntimeSessionState::ContextMismatch(_)));
    assert!(gateway.list_containers().await.is_ok());
    assert_eq!(gateway.invoke(fake_compose_invocation()).await.unwrap_err().code, AppErrorCode::RuntimeContextMismatch);
}

#[tokio::test]
async fn repeated_connect_replaces_mismatch_without_application_restart() {
    let gateway = reconnectable_gateway();
    assert!(matches!(gateway.connect_runtime().await.unwrap(), RuntimeSessionState::ContextMismatch(_)));
    gateway.set_fake_cli_fingerprint("matching");
    assert!(matches!(gateway.connect_runtime().await.unwrap(), RuntimeSessionState::Ready(_)));
}
```

- [ ] **Step 2: Run tests and verify failure**

Run: `cargo test -p colui-adapters --test runtime gateway_`

Expected: FAIL because gateway and Bollard mapping do not exist.

- [ ] **Step 3: Implement Bollard mapping**

Create one long-lived Bollard client per connection. Map `ContainerSummary` and inspect responses into `ContainerInstance`/application values without exposing Bollard types. Preserve container ID, name, image, state, status, service label when present, and published host/container/protocol bindings.

- [ ] **Step 4: Implement Compose argv adapter**

Build `docker compose` argv from backend-loaded `ProjectProfile` and an operation enum. Emit `-f <path>` for every stored Compose file in order, emit `--project-name <compose_project_name>`, emit one `--env-file <path>` for every stored environment file in order, then emit the operation-specific verb and flags: `up -d`, `stop`, `down`, or `restart`. Never accept frontend paths or a preformatted command string.

- [ ] **Step 5: Implement gateway ownership and session transitions**

Create `RuntimeGateway` with session, optional Bollard client, process runner, CLI environment, and `Arc<Semaphore>` initialized with one permit. `connect_runtime()` resolves endpoint, connects API, obtains API fingerprint, runs CLI `docker info` against the same endpoint, and atomically publishes Ready, ContextMismatch, or Failed. ContextMismatch retains API client only when API fingerprint call succeeded and tags reads with session ID/fingerprint. Disconnect invalidates client and session. Compose gate requires Ready and serializes invocation.

- [ ] **Step 6: Run gateway tests and boundary gate**

Run: `cargo test -p colui-adapters --test runtime gateway_ && cargo test --workspace && bash scripts/check-boundaries.sh`

Expected: PASS; no Bollard/process code leaks into domain/app and mismatch/readiness behavior is covered.

- [ ] **Step 7: Commit**

```bash
git add crates/colui-adapters
git commit -m "feat(runtime): add verified runtime gateway"
```

### Task 6: Add hermetic integration coverage and concurrency tests

**Files:**
- Modify: `crates/colui-adapters/tests/runtime.rs`
- Modify: `crates/colui-app/tests/runtime.rs`
- No script modification; verify with existing `scripts/check-boundaries.sh`

**Interfaces:**
- Consumes: Tasks 1-5 public runtime contracts and fake executable protocol.
- Produces: complete Docker-free Increment 2 regression suite.

- [ ] **Step 1: Add failing coverage for missing acceptance cases**

```rust
#[tokio::test]
async fn compose_gate_serializes_two_profiles() {
    let gateway = Arc::new(gateway_with_fake_runner());
    let first_gateway = Arc::clone(&gateway);
    let second_gateway = Arc::clone(&gateway);
    let first = tokio::spawn(async move { first_gateway.invoke(sleep_invocation(100)).await });
    let second = tokio::spawn(async move { second_gateway.invoke(sleep_invocation(100)).await });
    assert_maximum_active_fake_processes(1);
    first.await.unwrap().unwrap();
    second.await.unwrap().unwrap();
}

#[tokio::test]
async fn reconnect_invalidates_old_api_observation() {
    let gateway = reconnectable_gateway();
    gateway.connect_runtime().await.unwrap();
    let old_session = gateway.session_id().unwrap();
    gateway.connect_runtime().await.unwrap();
    assert_ne!(old_session, gateway.session_id().unwrap());
}
```

- [ ] **Step 2: Run focused tests and verify failure**

Run: `cargo test -p colui-adapters --test runtime -- --nocapture`

Expected: FAIL for any uncovered gate, stale-session, or concurrency behavior.

- [ ] **Step 3: Complete hermetic coverage**

Cover endpoint precedence, environment policy, canonical exact fingerprints, offline profile access, mismatch diagnostics, current-client API reads, Compose block, reconnect, one client lifetime, exact argv/cwd, shell metacharacters, output ring bounds and replacement decoding, non-zero exit, timeout/escalation/reap, and one-permit Compose serialization. Use deterministic fake executable fixtures; do not invoke Docker.

- [ ] **Step 4: Run complete hermetic verification**

Run:

```bash
cargo fmt --all -- --check
cargo test --workspace
bash scripts/check-boundaries.sh
git diff --check
```

Expected: all commands exit 0.

- [ ] **Step 5: Commit**

```bash
git add crates/colui-adapters/tests/runtime.rs crates/colui-app/tests/runtime.rs
git commit -m "test(runtime): cover gateway invariants"
```

### Task 7: Add feature-gated real Docker tests

**Files:**
- Modify: `crates/colui-adapters/Cargo.toml`
- Create: `crates/colui-adapters/tests/docker.rs`
- Modify: `crates/colui-adapters/tests/runtime.rs` only for shared fixture helpers

**Interfaces:**
- Consumes: concrete `RuntimeGateway`, real Bollard adapter, Compose adapter, and profile fixture helpers.
- Produces: `docker-tests` feature-gated disposable integration suite.

- [ ] **Step 1: Write feature-gated test skeleton**

```rust
#[cfg(feature = "docker-tests")]
#[tokio::test]
async fn disposable_compose_fixture_passes_apply_stop_apply_teardown() {
    if !docker_available().await {
        eprintln!("SKIP: local Docker daemon unavailable");
        return;
    }
    let fixture = TempComposeFixture::new();
    let gateway = RuntimeGateway::new(real_docker_config());
    assert!(matches!(gateway.connect_runtime(None).await.unwrap(), RuntimeSessionState::Ready(_)));
    fixture.apply(&gateway).await.unwrap();
    fixture.assert_containers_present(&gateway).await;
    fixture.stop(&gateway).await.unwrap();
    fixture.assert_containers_stopped(&gateway).await;
    fixture.apply(&gateway).await.unwrap();
    fixture.tear_down(&gateway).await.unwrap();
    fixture.assert_profile_survives();
}
```

- [ ] **Step 2: Run feature test and verify expected initial failure**

Run: `cargo test -p colui-adapters --features docker-tests --test docker`

Expected: FAIL until fixture, feature, and real gateway setup exist; when no daemon is present, final test behavior must be an explicit skip.

- [ ] **Step 3: Implement disposable fixture and feature**

Add empty `docker-tests = []` feature; existing runtime dependencies implement the test. Create temporary Compose files with an `alpine:3.20` service running `sleep 300`, connect and assert matching fingerprints, apply, list containers through Bollard, stop and verify resources remain stopped, apply again, tear down, verify the in-memory profile fixture remains unchanged, and clean resources through an explicit teardown guard. Add a scaled-service case with `--scale worker=2`. Keep missing-env-file validation for Increment 4 definition loading and synthetic context mismatch/timeout in hermetic tests because neither requires or benefits from mutating the developer's real Docker context.

- [ ] **Step 4: Run real-Docker verification**

Run on Docker Desktop and Colima separately:

```bash
cargo test -p colui-adapters --features docker-tests --test docker -- --nocapture
```

Expected: all fixture tests pass when daemon is available; tests report clear skips when unavailable.

- [ ] **Step 5: Commit**

```bash
git add crates/colui-adapters/Cargo.toml crates/colui-adapters/tests/docker.rs crates/colui-adapters/tests/runtime.rs Cargo.lock
git commit -m "test(runtime): add docker integration fixture"
```

## Final Verification

- [ ] Run `cargo fmt --all -- --check`.
- [ ] Run `cargo test --workspace` without Docker assumptions.
- [ ] Run `cargo test -p colui-adapters --features docker-tests --test docker -- --nocapture` on Docker Desktop.
- [ ] Repeat real-Docker command on Colima.
- [ ] Run `bash scripts/check-boundaries.sh`.
- [ ] Run `git diff --check`.
- [ ] Confirm no frontend/Tauri IPC, InventoryCoordinator, definition cache, Discovery, lifecycle UI, Docker Events, container logs, remote contexts, or speculative abstractions were added.
- [ ] Confirm Compose argv is built only from `ProjectProfile` loaded through `ProfileReader` plus an operation enum; no process starts from caller-supplied frontend paths or command strings.
- [ ] Confirm every timeout path attempts SIGTERM, configured grace, SIGKILL, bounded reap, and blocking fallback.
- [ ] Confirm final output payload is independently capped at 64 KiB per stream and 128 KiB combined.
