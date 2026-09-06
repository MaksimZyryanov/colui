#![cfg(feature = "test-support")]

use colui_adapters::runtime::ComposeExecutionGate;
use colui_adapters::runtime::{
    bollard_fingerprint, build_cli_environment, parse_cli_fingerprint, resolve_endpoint,
    EndpointPreference,
};
use colui_adapters::runtime::{ComposeOperation, RuntimeGateway};
use colui_adapters::runtime::{ComposeProcessRunner, TerminationConfig};
use colui_adapters::{DefinitionCache, InventoryCoordinator, OperationLockManager};
use colui_app::{
    Clock, ComposeInvocation, ComposeProcessResult, ComposeRunner, DefinitionRefresher, DockerApi,
    LifecycleOperation, LifecycleRuntime, ProfileReader, RegistrySnapshot, RuntimeConnector,
    RuntimeDiagnosticsReader, RuntimeStateReader,
};
use colui_domain::AppErrorCode;
use colui_domain::{
    AppError, ContainerDetails, ContainerId, ContainerObservation, DaemonFingerprint, ProfileDraft,
    ProfileId, ProjectProfile, RegistrationOrigin, RuntimeSessionState,
};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc, OnceLock,
};
use std::time::{Duration, Instant};
use tempfile::TempDir;

fn inherited_fixture_env() -> BTreeMap<String, String> {
    BTreeMap::from([
        ("DOCKER_HOST".into(), "unix:///inherited".into()),
        ("DOCKER_CONTEXT".into(), "desktop-linux".into()),
        ("DOCKER_CONFIG".into(), "/tmp/docker-config".into()),
        ("DOCKER_TLS_VERIFY".into(), "1".into()),
        ("COMPOSE_FILE".into(), "compose.yml".into()),
        ("DOCKER_BUILDKIT".into(), "1".into()),
        ("PATH".into(), "/usr/bin".into()),
    ])
}

#[test]
fn endpoint_resolution_prefers_explicit_then_docker_host_then_context_then_socket() {
    assert_eq!(
        resolve_endpoint(
            Some("unix:///explicit"),
            Some("unix:///env"),
            Some("unix:///context"),
        )
        .unwrap()
        .as_str(),
        "unix:///explicit"
    );
    assert_eq!(
        resolve_endpoint(None, Some("unix:///env"), Some("unix:///context"))
            .unwrap()
            .as_str(),
        "unix:///env"
    );
    assert_eq!(
        resolve_endpoint(None, None, Some("unix:///context"))
            .unwrap()
            .as_str(),
        "unix:///context"
    );
    assert_eq!(
        resolve_endpoint(None, None, None).unwrap().as_str(),
        "unix:///var/run/docker.sock"
    );
}

#[test]
fn endpoint_preference_wraps_explicit_endpoint() {
    let preference = EndpointPreference::try_from("unix:///explicit").unwrap();
    assert_eq!(preference.as_str(), "unix:///explicit");
}

#[test]
fn cli_environment_clears_context_and_compose_selection() {
    let env = build_cli_environment("unix:///resolved", inherited_fixture_env());
    assert_eq!(env.get("DOCKER_HOST").unwrap(), "unix:///resolved");
    assert!(!env.contains_key("DOCKER_CONTEXT"));
    assert!(!env.contains_key("COMPOSE_FILE"));
    assert!(!env.contains_key("DOCKER_TLS_VERIFY"));
    assert_eq!(env.get("DOCKER_CONFIG").unwrap(), "/tmp/docker-config");
    assert_eq!(env.get("DOCKER_BUILDKIT").unwrap(), "1");
    assert_eq!(env.get("PATH").unwrap(), "/usr/bin");
}

#[test]
fn cli_info_parser_extracts_exact_server_fingerprint() {
    let output = "ID: abc\nServer Version: 20.10.17\nOSType: linux\nArchitecture: aarch64\n";
    assert_eq!(
        parse_cli_fingerprint(output).unwrap().server_version(),
        "20.10.17"
    );
}

#[test]
fn cli_info_parser_rejects_missing_or_ambiguous_server_fields() {
    let missing = "ID: abc\nOSType: linux\nArchitecture: aarch64\n";
    assert!(parse_cli_fingerprint(missing).is_err());

    let ambiguous = "ID: abc\nServer Version: 20.10.17\nServer Version: 20.10.18\nOSType: linux\nArchitecture: aarch64\n";
    assert!(parse_cli_fingerprint(ambiguous).is_err());
}

#[test]
fn bollard_info_maps_four_fingerprint_fields() {
    let info: bollard::models::SystemInfo = serde_json::from_str(
        r#"{"ID":"abc","ServerVersion":"20.10.17","OSType":"linux","Architecture":"aarch64"}"#,
    )
    .unwrap();
    let fingerprint = bollard_fingerprint(&info).unwrap();
    assert_eq!(fingerprint.server_version(), "20.10.17");
    assert_eq!(fingerprint.daemon_id, "abc");
    assert_eq!(fingerprint.os_type, "linux");
    assert_eq!(fingerprint.architecture, "aarch64");
}

#[test]
fn bollard_info_rejects_incomplete_fingerprint_fields() {
    let info: bollard::models::SystemInfo =
        serde_json::from_str(r#"{"ID":"abc","ServerVersion":"20.10.17","OSType":"linux"}"#)
            .unwrap();
    let error = bollard_fingerprint(&info).unwrap_err();
    assert_eq!(error.code, AppErrorCode::RuntimeConnectionFailed);
}

fn fixture() -> &'static Path {
    static PATH: OnceLock<PathBuf> = OnceLock::new();
    PATH.get_or_init(|| {
        let dir = tempfile::tempdir().unwrap();
        let output = dir.path().join("fake-compose");
        let status = std::process::Command::new("rustc")
            .args([
                "--edition",
                "2021",
                concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/fixtures/fake_compose.rs"
                ),
                "-o",
            ])
            .arg(&output)
            .status()
            .unwrap();
        assert!(status.success());
        std::mem::forget(dir);
        output
    })
}

fn runner(grace: Duration) -> ComposeProcessRunner {
    ComposeProcessRunner::new(TerminationConfig {
        grace_period: grace,
    })
}

fn invocation(mode: &str, env: BTreeMap<String, String>, deadline: Instant) -> ComposeInvocation {
    ComposeInvocation {
        executable: fixture().to_path_buf(),
        args: vec!["literal;$(touch SHOULD_NOT_EXIST)".into()],
        working_directory: std::env::current_dir().unwrap(),
        environment: BTreeMap::from([(String::from("FAKE_EXEC_MODE"), mode.into())])
            .into_iter()
            .chain(env)
            .collect(),
        deadline,
    }
}

#[tokio::test]
async fn runner_preserves_newest_bytes_per_stream_and_marks_truncation() {
    let result = runner(Duration::from_millis(100))
        .invoke(invocation(
            "success",
            BTreeMap::from([
                ("FAKE_STDOUT_BYTES".into(), "100000".into()),
                ("FAKE_STDERR_BYTES".into(), "90000".into()),
            ]),
            Instant::now() + Duration::from_secs(5),
        ))
        .await
        .unwrap();
    assert_eq!(result.stdout().len(), 64 * 1024);
    assert_eq!(result.stderr().len(), 64 * 1024);
    assert!(
        result.stdout_truncated(),
        "stdout len={} truncated={} stderr len={} truncated={}",
        result.stdout().len(),
        result.stdout_truncated(),
        result.stderr().len(),
        result.stderr_truncated()
    );
    assert!(result.stderr_truncated());
    assert!(result.stdout().ends_with("Z"));
}

#[tokio::test]
async fn runner_uses_argv_cwd_and_env_without_shell_interpolation() {
    let dir = TempDir::new().unwrap();
    let args = dir.path().join("args");
    let cwd = dir.path().join("cwd");
    let env = dir.path().join("env");
    let result = runner(Duration::from_millis(100))
        .invoke(invocation(
            "success",
            BTreeMap::from([
                ("FAKE_ARGS_FILE".into(), args.to_string_lossy().into()),
                ("FAKE_CWD_FILE".into(), cwd.to_string_lossy().into()),
                ("FAKE_ENV_FILE".into(), env.to_string_lossy().into()),
                ("SELECTED_ENV".into(), "kept".into()),
            ]),
            Instant::now() + Duration::from_secs(5),
        ))
        .await
        .unwrap();
    assert_eq!(result.exit_code(), Some(0));
    let recorded = std::fs::read_to_string(args).unwrap();
    assert_eq!(
        recorded.lines().next().unwrap(),
        "literal;$(touch SHOULD_NOT_EXIST)"
    );
    assert_eq!(
        std::fs::read_to_string(cwd).unwrap(),
        std::env::current_dir().unwrap().to_string_lossy()
    );
    assert!(std::fs::read_to_string(env)
        .unwrap()
        .contains("SELECTED_ENV=kept"));
    assert!(!Path::new("SHOULD_NOT_EXIST").exists());
}

#[tokio::test]
async fn runner_decodes_invalid_utf8_and_maps_failure() {
    let decoded = runner(Duration::from_millis(100))
        .invoke(invocation(
            "success",
            BTreeMap::from([("FAKE_INVALID_UTF8".into(), "1".into())]),
            Instant::now() + Duration::from_secs(5),
        ))
        .await
        .unwrap();
    assert!(decoded.stdout().contains('\u{FFFD}'));
    let error = runner(Duration::from_millis(100))
        .invoke(invocation(
            "failure",
            BTreeMap::from([
                ("FAKE_EXIT_CODE".into(), "7".into()),
                ("FAKE_INVALID_UTF8".into(), "1".into()),
            ]),
            Instant::now() + Duration::from_secs(5),
        ))
        .await
        .unwrap_err();
    assert_eq!(error.code, AppErrorCode::ComposeFailed);
}

#[cfg(unix)]
#[tokio::test]
async fn runner_terminates_process_group_and_reaps_after_timeout() {
    let dir = TempDir::new().unwrap();
    let child_pid = dir.path().join("child-pid");
    let result = runner(Duration::from_millis(50))
        .invoke(invocation(
            "signal-aware",
            BTreeMap::from([
                (
                    "FAKE_CHILD_PID_FILE".into(),
                    child_pid.to_string_lossy().into(),
                ),
                ("FAKE_SLEEP_MS".into(), "10000".into()),
            ]),
            Instant::now() + Duration::from_secs(1),
        ))
        .await
        .unwrap();
    assert!(result.timed_out());
    tokio::time::timeout(Duration::from_secs(1), async {
        while !child_pid.is_file() {
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await
    .expect("fake child PID file was not created");
    let pid: i32 = std::fs::read_to_string(child_pid).unwrap().parse().unwrap();
    for _ in 0..50 {
        if !process_exists(pid) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("child process remains");
}

#[cfg(unix)]
fn process_exists(pid: i32) -> bool {
    unsafe {
        if libc::kill(pid, 0) == 0 {
            true
        } else {
            errno() != libc::ESRCH
        }
    }
}

#[cfg(target_os = "macos")]
unsafe fn errno() -> i32 {
    *libc::__error()
}

#[cfg(all(unix, not(target_os = "macos")))]
unsafe fn errno() -> i32 {
    *libc::__errno_location()
}

#[cfg(not(unix))]
#[tokio::test]
async fn runner_reports_unsupported_process_groups_without_indefinite_wait() {
    let error = runner(Duration::from_millis(1))
        .run(invocation("sleep", BTreeMap::new(), Instant::now()))
        .await
        .unwrap_err();
    assert_eq!(error.code, AppErrorCode::ComposeFailed);
    assert!(error.message.contains("process groups unsupported"));
}

#[test]
fn compose_argv_comes_only_from_profile_and_operation() {
    let profile = ProjectProfile::from_draft(
        colui_domain::ProfileId::new(uuid::Uuid::from_u128(1)),
        ProfileDraft {
            display_name: "Demo".try_into().unwrap(),
            compose_project_name: "demo".try_into().unwrap(),
            working_directory: PathBuf::from("/workspace"),
            compose_files: vec![PathBuf::from("a.yml"), PathBuf::from("b.yml")],
            environment_files: vec![PathBuf::from("one.env"), PathBuf::from("two.env")],
            registration_origin: RegistrationOrigin::Manual,
        },
    )
    .unwrap();
    assert_eq!(
        colui_adapters::runtime::compose_args(&profile, ComposeOperation::Up),
        vec![
            "compose",
            "-f",
            "/workspace/a.yml",
            "-f",
            "/workspace/b.yml",
            "--project-name",
            "demo",
            "--env-file",
            "/workspace/one.env",
            "--env-file",
            "/workspace/two.env",
            "up",
            "-d"
        ]
    );
}

#[tokio::test]
async fn gateway_missing_session_blocks_api_and_compose() {
    let gateway = RuntimeGateway::new_for_tests(
        Box::new(FakeDocker::new("same")),
        Box::new(FakeRunner::new("same")),
    );
    assert_eq!(
        gateway.list_containers().await.unwrap_err().code,
        AppErrorCode::RuntimeUnavailable
    );
    assert_eq!(
        gateway
            .invoke_backend_for_tests(fake_invocation())
            .await
            .unwrap_err()
            .code,
        AppErrorCode::RuntimeUnavailable
    );
}

#[tokio::test]
async fn gateway_matching_fingerprints_reuses_one_client_and_allows_reads() {
    let gateway = RuntimeGateway::new_for_tests(
        Box::new(FakeDocker::new("same")),
        Box::new(FakeRunner::new("same")),
    );
    assert!(matches!(
        gateway.connect_runtime(None).await.unwrap(),
        colui_domain::RuntimeSessionState::Ready(_)
    ));
    assert_eq!(gateway.created_client_count(), 1);
    assert!(gateway.list_containers().await.is_ok());
}

#[tokio::test]
async fn gateway_without_environment_uses_active_docker_context_endpoint() {
    let gateway = RuntimeGateway::new_for_tests_with_environment(
        Box::new(FakeDocker::new("same")),
        Box::new(ContextRunner),
        BTreeMap::new(),
    );

    let state = gateway.connect_runtime(None).await.unwrap();
    let RuntimeSessionState::Ready(context) = state else {
        panic!("expected ready runtime state");
    };
    assert_eq!(context.endpoint.as_str(), "unix:///context/docker.sock");
}

#[tokio::test]
async fn gateway_mismatch_allows_api_reads_but_blocks_compose() {
    let gateway = RuntimeGateway::new_for_tests(
        Box::new(FakeDocker::new("mismatch")),
        Box::new(FakeRunner::new("same")),
    );
    assert!(matches!(
        gateway.connect_runtime(None).await.unwrap(),
        colui_domain::RuntimeSessionState::ContextMismatch(_)
    ));
    assert!(gateway.list_containers().await.is_ok());
    assert_eq!(
        gateway
            .invoke_backend_for_tests(fake_invocation())
            .await
            .unwrap_err()
            .code,
        AppErrorCode::RuntimeContextMismatch
    );
}

#[tokio::test]
async fn gateway_reconnect_replaces_mismatch_without_restart() {
    let gateway = RuntimeGateway::new_for_tests(
        Box::new(FakeDocker::new("same")),
        Box::new(FakeRunner::new("mismatch")),
    );
    assert!(matches!(
        gateway.connect_runtime(None).await.unwrap(),
        colui_domain::RuntimeSessionState::ContextMismatch(_)
    ));
    gateway.replace_runner_for_tests(Box::new(FakeRunner::new("same")));
    assert!(matches!(
        gateway.connect_runtime(None).await.unwrap(),
        colui_domain::RuntimeSessionState::Ready(_)
    ));
}

#[tokio::test]
async fn lifecycle_runtime_maps_all_operations_to_distinct_compose_verbs() {
    let last = Arc::new(std::sync::Mutex::new(None));
    let gateway = RuntimeGateway::new_for_tests(
        Box::new(FakeDocker::new("same")),
        Box::new(RecordingRunner { last: last.clone() }),
    );
    gateway.connect_runtime(None).await.unwrap();
    let profile = ProjectProfile::from_draft(
        ProfileId::new(uuid::Uuid::from_u128(2)),
        ProfileDraft {
            display_name: "Profile".try_into().unwrap(),
            compose_project_name: "project".try_into().unwrap(),
            working_directory: PathBuf::from("/workspace"),
            compose_files: vec![PathBuf::from("compose.yml")],
            environment_files: vec![],
            registration_origin: RegistrationOrigin::Manual,
        },
    )
    .unwrap();
    for (operation, verb) in [
        (LifecycleOperation::Apply, "up"),
        (LifecycleOperation::Stop, "stop"),
        (LifecycleOperation::TearDown, "down"),
        (LifecycleOperation::Restart, "restart"),
    ] {
        gateway
            .run_profile(profile.clone(), operation)
            .await
            .unwrap();
        let invocation = last.lock().unwrap().clone().unwrap();
        assert_eq!(invocation.executable, PathBuf::from("docker"));
        assert!(invocation.args.iter().any(|arg| arg == verb));
        assert_eq!(invocation.working_directory, profile.working_directory);
    }
}

#[tokio::test]
async fn gateway_profile_route_returns_typed_error_when_not_ready() {
    let last = Arc::new(std::sync::Mutex::new(None));
    let gateway = RuntimeGateway::new_for_tests(
        Box::new(FakeDocker::new("same")),
        Box::new(RecordingRunner { last: last.clone() }),
    );
    let profile = test_profile();
    let error = gateway
        .run_profile(profile, LifecycleOperation::Stop)
        .await
        .unwrap_err();

    assert_eq!(error.code, AppErrorCode::RuntimeUnavailable);
    assert_eq!(error.operation, "compose");
    assert!(last.lock().unwrap().is_none());
}

#[tokio::test]
async fn lifecycle_runtime_mismatch_does_not_invoke_runner() {
    let last = Arc::new(std::sync::Mutex::new(None));
    let gateway = RuntimeGateway::new_for_tests(
        Box::new(FakeDocker::new("mismatch")),
        Box::new(RecordingRunner { last: last.clone() }),
    );
    gateway.connect_runtime(None).await.unwrap();

    let error = gateway
        .run_profile(test_profile(), LifecycleOperation::Apply)
        .await
        .unwrap_err();

    assert_eq!(error.code, AppErrorCode::RuntimeContextMismatch);
    assert!(last.lock().unwrap().is_none());
}

#[tokio::test]
async fn lifecycle_runtime_maps_nonzero_and_timeout_results() {
    for (result, code) in [
        (
            ComposeProcessResult::completed(7, "", "", Duration::ZERO),
            AppErrorCode::ComposeFailed,
        ),
        (
            ComposeProcessResult::from_timeout("", "", false, false, Duration::ZERO),
            AppErrorCode::OperationTimeout,
        ),
    ] {
        let gateway = RuntimeGateway::new_for_tests(
            Box::new(FakeDocker::new("same")),
            Box::new(LifecycleStatusRunner { result }),
        );
        gateway.connect_runtime(None).await.unwrap();
        let error = gateway
            .run_profile(test_profile(), LifecycleOperation::Stop)
            .await
            .unwrap_err();
        assert_eq!(error.code, code);
    }
}

#[tokio::test]
async fn gateway_profile_lookup_waits_for_connect_operation_gate() {
    let runner = Arc::new(BlockingInfoRunner::default());
    let gateway = Arc::new(RuntimeGateway::new_for_tests(
        Box::new(FakeDocker::new("same")),
        Box::new(BlockingInfoRunner {
            started: runner.started.clone(),
            release: runner.release.clone(),
            calls: runner.calls.clone(),
        }),
    ));
    let connect = tokio::spawn({
        let gateway = gateway.clone();
        async move { gateway.connect_runtime(None).await }
    });
    tokio::time::timeout(Duration::from_secs(1), runner.started.notified())
        .await
        .unwrap();

    let profile = test_profile();
    let invoke = tokio::spawn({
        let gateway = gateway.clone();
        async move { gateway.run_profile(profile, LifecycleOperation::Stop).await }
    });
    tokio::task::yield_now().await;
    assert!(!invoke.is_finished());

    runner.release.notify_one();
    assert!(connect.await.unwrap().is_ok());
    assert!(invoke.await.unwrap().is_ok());
}

#[tokio::test]
async fn concurrent_connect_joins_one_transition_and_one_session() {
    let runner = Arc::new(BlockingInfoRunner::default());
    let gateway = Arc::new(RuntimeGateway::new_for_tests(
        Box::new(FakeDocker::new("same")),
        Box::new(BlockingInfoRunner {
            started: runner.started.clone(),
            release: runner.release.clone(),
            calls: runner.calls.clone(),
        }),
    ));
    let first = tokio::spawn({
        let gateway = gateway.clone();
        async move { gateway.connect_runtime(None).await }
    });
    tokio::time::timeout(Duration::from_secs(1), runner.started.notified())
        .await
        .unwrap();
    let second = tokio::spawn({
        let gateway = gateway.clone();
        async move { gateway.connect_runtime(None).await }
    });
    tokio::task::yield_now().await;
    runner.release.notify_one();

    let first = first.await.unwrap().unwrap();
    let second = second.await.unwrap().unwrap();
    assert_eq!(first, second);
    assert_eq!(gateway.created_client_count(), 1);
}

#[tokio::test]
async fn disconnect_cancels_active_connect_without_publishing_ready() {
    let runner = Arc::new(BlockingInfoRunner::default());
    let gateway = Arc::new(RuntimeGateway::new_for_tests(
        Box::new(FakeDocker::new("same")),
        Box::new(BlockingInfoRunner {
            started: runner.started.clone(),
            release: runner.release.clone(),
            calls: runner.calls.clone(),
        }),
    ));
    let connect = tokio::spawn({
        let gateway = gateway.clone();
        async move { gateway.connect_runtime(None).await }
    });
    tokio::time::timeout(Duration::from_secs(1), runner.started.notified())
        .await
        .unwrap();
    let disconnect = tokio::spawn({
        let gateway = gateway.clone();
        async move { gateway.disconnect_runtime().await }
    });
    tokio::task::yield_now().await;
    runner.release.notify_one();

    assert_eq!(
        connect.await.unwrap().unwrap(),
        RuntimeSessionState::Disconnected
    );
    disconnect.await.unwrap().unwrap();
    assert_eq!(
        gateway.session_state().await.unwrap(),
        RuntimeSessionState::Disconnected
    );
}

#[tokio::test]
async fn reconnect_cancels_active_connect_then_owns_one_fresh_connect() {
    let runner = Arc::new(BlockingInfoRunner::default());
    let gateway = Arc::new(RuntimeGateway::new_for_tests(
        Box::new(FakeDocker::new("same")),
        Box::new(BlockingInfoRunner {
            started: runner.started.clone(),
            release: runner.release.clone(),
            calls: runner.calls.clone(),
        }),
    ));
    let connect = tokio::spawn({
        let gateway = gateway.clone();
        async move { gateway.connect_runtime(None).await }
    });
    tokio::time::timeout(Duration::from_secs(1), runner.started.notified())
        .await
        .unwrap();
    let reconnect = tokio::spawn({
        let gateway = gateway.clone();
        async move { gateway.reconnect_runtime(None).await }
    });
    tokio::task::yield_now().await;
    runner.release.notify_one();

    assert_eq!(
        connect.await.unwrap().unwrap(),
        RuntimeSessionState::Disconnected
    );
    assert!(matches!(
        reconnect.await.unwrap().unwrap(),
        RuntimeSessionState::Ready(_)
    ));
    assert_eq!(gateway.created_client_count(), 2);
}

#[tokio::test]
async fn late_connect_during_reconnect_takeover_joins_cancelled_connect_result() {
    let runner = Arc::new(BlockingInfoRunner::default());
    let gateway = Arc::new(RuntimeGateway::new_for_tests(
        Box::new(FakeDocker::new("same")),
        Box::new(BlockingInfoRunner {
            started: runner.started.clone(),
            release: runner.release.clone(),
            calls: runner.calls.clone(),
        }),
    ));
    let connect = tokio::spawn({
        let gateway = gateway.clone();
        async move { gateway.connect_runtime(None).await }
    });
    tokio::time::timeout(Duration::from_secs(1), runner.started.notified())
        .await
        .unwrap();
    let reconnect = tokio::spawn({
        let gateway = gateway.clone();
        async move { gateway.reconnect_runtime(None).await }
    });
    tokio::task::yield_now().await;
    let late_connect = tokio::spawn({
        let gateway = gateway.clone();
        async move { gateway.connect_runtime(None).await }
    });
    tokio::task::yield_now().await;
    runner.release.notify_one();

    assert_eq!(
        connect.await.unwrap().unwrap(),
        RuntimeSessionState::Disconnected
    );
    assert_eq!(
        late_connect.await.unwrap().unwrap(),
        RuntimeSessionState::Disconnected
    );
    assert!(matches!(
        reconnect.await.unwrap().unwrap(),
        RuntimeSessionState::Ready(_)
    ));
}

#[tokio::test]
async fn reconnect_active_joins_connect_and_reconnect_but_rejects_disconnect() {
    let gateway = Arc::new(RuntimeGateway::new_for_tests(
        Box::new(FakeDocker::new("same")),
        Box::new(FakeRunner::new("same")),
    ));
    gateway.connect_runtime(None).await.unwrap();
    let runner = Arc::new(BlockingInfoRunner::default());
    gateway.replace_runner_for_tests(Box::new(BlockingInfoRunner {
        started: runner.started.clone(),
        release: runner.release.clone(),
        calls: runner.calls.clone(),
    }));
    let reconnect = tokio::spawn({
        let gateway = gateway.clone();
        async move { gateway.reconnect_runtime(None).await }
    });
    tokio::time::timeout(Duration::from_secs(1), runner.started.notified())
        .await
        .unwrap();
    let connect = tokio::spawn({
        let gateway = gateway.clone();
        async move { gateway.connect_runtime(None).await }
    });
    let joined_reconnect = tokio::spawn({
        let gateway = gateway.clone();
        async move { gateway.reconnect_runtime(None).await }
    });
    let disconnect = tokio::time::timeout(Duration::from_millis(100), gateway.disconnect_runtime())
        .await
        .expect("disconnect conflict must not wait")
        .unwrap_err();
    assert_eq!(disconnect.code, AppErrorCode::OperationConflict);
    runner.release.notify_one();

    let owner = reconnect.await.unwrap().unwrap();
    assert_eq!(connect.await.unwrap().unwrap(), owner);
    assert_eq!(joined_reconnect.await.unwrap().unwrap(), owner);
    assert_eq!(gateway.created_client_count(), 2);
}

#[tokio::test]
async fn reconnect_invalidates_api_context_before_fresh_connect_completes() {
    let gateway = Arc::new(RuntimeGateway::new_for_tests(
        Box::new(FakeDocker::new("same")),
        Box::new(FakeRunner::new("same")),
    ));
    gateway.connect_runtime(None).await.unwrap();
    let runner = Arc::new(BlockingInfoRunner::default());
    gateway.replace_runner_for_tests(Box::new(BlockingInfoRunner {
        started: runner.started.clone(),
        release: runner.release.clone(),
        calls: runner.calls.clone(),
    }));
    let reconnect = tokio::spawn({
        let gateway = gateway.clone();
        async move { gateway.reconnect_runtime(None).await }
    });
    tokio::time::timeout(Duration::from_secs(1), runner.started.notified())
        .await
        .unwrap();

    assert_eq!(
        gateway.list_containers().await.unwrap_err().code,
        AppErrorCode::RuntimeUnavailable
    );
    runner.release.notify_one();
    assert!(matches!(
        reconnect.await.unwrap().unwrap(),
        RuntimeSessionState::Ready(_)
    ));
}

#[tokio::test]
async fn diagnostics_and_inventory_refresh_read_during_active_lifecycle() {
    let runner = LifecycleBlockingRunner::default();
    let gateway = Arc::new(RuntimeGateway::new_for_tests(
        Box::new(FakeDocker::new("same")),
        Box::new(runner.clone()),
    ));
    gateway.connect_runtime(None).await.unwrap();
    let lifecycle = tokio::spawn({
        let gateway = gateway.clone();
        async move {
            gateway
                .run_profile(test_profile(), LifecycleOperation::Stop)
                .await
        }
    });
    tokio::time::timeout(Duration::from_secs(1), runner.started.notified())
        .await
        .unwrap();
    let inventory = InventoryCoordinator::new(gateway.clone(), Arc::new(FixedClock));

    let diagnostics =
        tokio::time::timeout(Duration::from_millis(100), gateway.runtime_diagnostics())
            .await
            .expect("diagnostics read must not join lifecycle")
            .unwrap();
    assert!(matches!(diagnostics.state, RuntimeSessionState::Ready(_)));
    tokio::time::timeout(Duration::from_millis(100), inventory.refresh())
        .await
        .expect("inventory refresh must not join lifecycle")
        .unwrap();

    runner.release.notify_one();
    lifecycle.await.unwrap().unwrap();
}

#[tokio::test]
async fn idle_state_transition_table_rows() {
    let disconnected_reconnect = RuntimeGateway::new_for_tests(
        Box::new(FakeDocker::new("same")),
        Box::new(FakeRunner::new("same")),
    );
    assert!(matches!(
        disconnected_reconnect
            .reconnect_runtime(None)
            .await
            .unwrap(),
        RuntimeSessionState::Ready(_)
    ));

    let ready = RuntimeGateway::new_for_tests(
        Box::new(FakeDocker::new("same")),
        Box::new(FakeRunner::new("same")),
    );
    ready.disconnect_runtime().await.unwrap();
    let first = ready.connect_runtime(None).await.unwrap();
    assert_eq!(ready.connect_runtime(None).await.unwrap(), first);
    assert_eq!(ready.created_client_count(), 1);
    let reconnected = ready.reconnect_runtime(None).await.unwrap();
    assert_ne!(ready_session(&first), ready_session(&reconnected));
    ready.disconnect_runtime().await.unwrap();
    assert_eq!(
        ready.session_state().await.unwrap(),
        RuntimeSessionState::Disconnected
    );

    let mismatch = RuntimeGateway::new_for_tests(
        Box::new(FakeDocker::new("api")),
        Box::new(FakeRunner::new("cli")),
    );
    assert!(matches!(
        mismatch.connect_runtime(None).await.unwrap(),
        RuntimeSessionState::ContextMismatch(_)
    ));
    assert!(matches!(
        mismatch.connect_runtime(None).await.unwrap(),
        RuntimeSessionState::ContextMismatch(_)
    ));
    assert_eq!(mismatch.created_client_count(), 2);
    mismatch.disconnect_runtime().await.unwrap();

    let mismatch_reconnect = RuntimeGateway::new_for_tests(
        Box::new(FakeDocker::new("api")),
        Box::new(FakeRunner::new("cli")),
    );
    mismatch_reconnect.connect_runtime(None).await.unwrap();
    assert!(matches!(
        mismatch_reconnect.reconnect_runtime(None).await.unwrap(),
        RuntimeSessionState::ContextMismatch(_)
    ));

    let failed = RuntimeGateway::new_for_tests(
        Box::new(FakeDocker::new("same")),
        Box::new(StatusRunner {
            result: ComposeProcessResult::completed(1, "", "", Duration::ZERO),
        }),
    );
    assert!(matches!(
        failed.connect_runtime(None).await.unwrap(),
        RuntimeSessionState::Failed(_)
    ));
    failed.disconnect_runtime().await.unwrap();

    let failed_connect = RuntimeGateway::new_for_tests(
        Box::new(FakeDocker::new("same")),
        Box::new(StatusRunner {
            result: ComposeProcessResult::completed(1, "", "", Duration::ZERO),
        }),
    );
    failed_connect.connect_runtime(None).await.unwrap();
    failed_connect.replace_runner_for_tests(Box::new(FakeRunner::new("same")));
    assert!(matches!(
        failed_connect.connect_runtime(None).await.unwrap(),
        RuntimeSessionState::Ready(_)
    ));

    let failed_reconnect = RuntimeGateway::new_for_tests(
        Box::new(FakeDocker::new("same")),
        Box::new(StatusRunner {
            result: ComposeProcessResult::completed(1, "", "", Duration::ZERO),
        }),
    );
    assert!(matches!(
        failed_reconnect.connect_runtime(None).await.unwrap(),
        RuntimeSessionState::Failed(_)
    ));
    failed_reconnect.replace_runner_for_tests(Box::new(FakeRunner::new("same")));
    assert!(matches!(
        failed_reconnect.reconnect_runtime(None).await.unwrap(),
        RuntimeSessionState::Ready(_)
    ));
}

fn ready_session(state: &RuntimeSessionState) -> uuid::Uuid {
    match state {
        RuntimeSessionState::Ready(context) => *context.session_id.as_uuid(),
        state => panic!("expected ready state, got {state:?}"),
    }
}

#[tokio::test]
async fn connect_and_reconnect_wait_for_active_disconnect_then_start_fresh_connect() {
    for reconnect in [false, true] {
        let compose = LifecycleBlockingRunner::default();
        let gateway = Arc::new(RuntimeGateway::new_for_tests(
            Box::new(FakeDocker::new("same")),
            Box::new(compose.clone()),
        ));
        gateway.connect_runtime(None).await.unwrap();
        let invocation = tokio::spawn({
            let gateway = gateway.clone();
            async move { gateway.invoke_backend_for_tests(fake_invocation()).await }
        });
        tokio::time::timeout(Duration::from_secs(1), compose.started.notified())
            .await
            .unwrap();
        let disconnect = tokio::spawn({
            let gateway = gateway.clone();
            async move { gateway.disconnect_runtime().await }
        });
        let joined_disconnect = tokio::spawn({
            let gateway = gateway.clone();
            async move { gateway.disconnect_runtime().await }
        });
        tokio::task::yield_now().await;
        let next = tokio::spawn({
            let gateway = gateway.clone();
            async move {
                if reconnect {
                    gateway.reconnect_runtime(None).await
                } else {
                    gateway.connect_runtime(None).await
                }
            }
        });
        tokio::task::yield_now().await;
        assert!(!next.is_finished());
        compose.release.notify_one();
        invocation.await.unwrap().unwrap();
        disconnect.await.unwrap().unwrap();
        joined_disconnect.await.unwrap().unwrap();
        assert!(matches!(
            next.await.unwrap().unwrap(),
            RuntimeSessionState::Ready(_)
        ));
        assert_eq!(gateway.created_client_count(), 2);
    }
}

fn test_profile() -> ProjectProfile {
    ProjectProfile::from_draft(
        ProfileId::new(uuid::Uuid::from_u128(3)),
        ProfileDraft {
            display_name: "Profile".try_into().unwrap(),
            compose_project_name: "project".try_into().unwrap(),
            working_directory: PathBuf::from("/workspace"),
            compose_files: vec![PathBuf::from("compose.yml")],
            environment_files: vec![],
            registration_origin: RegistrationOrigin::Manual,
        },
    )
    .unwrap()
}

struct SingleProfile(ProjectProfile);
impl ProfileReader for SingleProfile {
    fn load(&self) -> colui_app::StoreFuture<'_, RegistrySnapshot> {
        let profile = self.0.clone();
        Box::pin(async move {
            Ok(RegistrySnapshot {
                registry_revision: 1,
                profiles: vec![profile],
            })
        })
    }
}

#[tokio::test]
async fn gateway_rejects_unsuccessful_or_timed_out_cli_info() {
    for result in [
        ComposeProcessResult::completed(1, "", "", Duration::ZERO),
        ComposeProcessResult::from_timeout("", "", false, false, Duration::ZERO),
    ] {
        let gateway = RuntimeGateway::new_for_tests(
            Box::new(FakeDocker::new("same")),
            Box::new(StatusRunner { result }),
        );
        let state = gateway.connect_runtime(None).await.unwrap();
        assert!(matches!(
            state,
            colui_domain::RuntimeSessionState::Failed(_)
        ));
        assert_eq!(
            gateway.list_containers().await.unwrap_err().code,
            AppErrorCode::RuntimeUnavailable
        );
    }
}

#[tokio::test]
async fn gateway_disconnect_waits_for_active_compose_operation() {
    let runner = Arc::new(BlockingRunner::default());
    let gateway = Arc::new(RuntimeGateway::new_for_tests(
        Box::new(FakeDocker::new("same")),
        Box::new(BlockingRunner {
            started: runner.started.clone(),
            release: runner.release.clone(),
            calls: runner.calls.clone(),
        }),
    ));
    assert!(matches!(
        gateway.connect_runtime(None).await.unwrap(),
        colui_domain::RuntimeSessionState::Ready(_)
    ));

    let invoke = tokio::spawn({
        let gateway = gateway.clone();
        async move { gateway.invoke_backend_for_tests(fake_invocation()).await }
    });
    tokio::time::timeout(Duration::from_secs(1), runner.started.notified())
        .await
        .unwrap();

    let disconnect = tokio::spawn({
        let gateway = gateway.clone();
        async move { gateway.disconnect_runtime().await }
    });
    tokio::task::yield_now().await;
    assert!(!disconnect.is_finished());

    runner.release.notify_one();
    assert!(invoke.await.unwrap().is_ok());
    assert!(disconnect.await.unwrap().is_ok());
}

#[tokio::test]
async fn compose_gate_serializes_two_profiles() {
    let active = Arc::new(AtomicUsize::new(0));
    let maximum_active = Arc::new(AtomicUsize::new(0));
    let observed_args = Arc::new(std::sync::Mutex::new(Vec::new()));
    let gateway = Arc::new(RuntimeGateway::new_for_tests(
        Box::new(FakeDocker::new("same")),
        Box::new(CountingRunner {
            active,
            maximum_active: maximum_active.clone(),
            observed_args: observed_args.clone(),
        }),
    ));
    gateway.connect_runtime(None).await.unwrap();

    let first_gateway = gateway.clone();
    let second_gateway = gateway.clone();
    let first = tokio::spawn(async move {
        first_gateway
            .invoke_backend_for_tests(invocation_with_args("profile-a"))
            .await
    });
    let second = tokio::spawn(async move {
        second_gateway
            .invoke_backend_for_tests(invocation_with_args("profile-b"))
            .await
    });
    assert!(first.await.unwrap().is_ok());
    assert!(second.await.unwrap().is_ok());
    assert_eq!(maximum_active.load(Ordering::Acquire), 1);
    let observed_args = observed_args.lock().unwrap();
    assert!(observed_args
        .iter()
        .any(|args| args == &vec![String::from("profile-a")]));
    assert!(observed_args
        .iter()
        .any(|args| args == &vec![String::from("profile-b")]));
}

#[derive(Clone)]
struct FixedClock;

impl Clock for FixedClock {
    fn now(&self) -> colui_domain::Timestamp {
        colui_domain::Timestamp("2026-09-03T00:00:00Z".into())
    }

    fn monotonic(&self) -> Duration {
        Duration::ZERO
    }
}

#[derive(Clone, Default)]
struct LifecycleBlockingRunner {
    started: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}

impl ComposeRunner for LifecycleBlockingRunner {
    fn invoke(
        &self,
        invocation: ComposeInvocation,
    ) -> colui_app::RuntimeFuture<'_, ComposeProcessResult> {
        let started = self.started.clone();
        let release = self.release.clone();
        Box::pin(async move {
            if invocation.args == vec!["info"] {
                return Ok(ComposeProcessResult::completed(
                    0,
                    "ID: same\nServer Version: 1\nOSType: linux\nArchitecture: x86_64\n",
                    "",
                    Duration::ZERO,
                ));
            }
            started.notify_one();
            release.notified().await;
            Ok(ComposeProcessResult::completed(0, "", "", Duration::ZERO))
        })
    }
}

struct DefinitionCountingRunner(Arc<AtomicUsize>);

impl ComposeRunner for DefinitionCountingRunner {
    fn invoke(&self, _: ComposeInvocation) -> colui_app::RuntimeFuture<'_, ComposeProcessResult> {
        self.0.fetch_add(1, Ordering::AcqRel);
        Box::pin(async {
            Ok(ComposeProcessResult::completed(
                0,
                r#"{"services":{}}"#,
                "",
                Duration::ZERO,
            ))
        })
    }
}

#[tokio::test]
async fn shared_compose_gate_serializes_lifecycle_and_definition_across_profiles() {
    let gate = Arc::new(ComposeExecutionGate::new());
    let lifecycle_runner = LifecycleBlockingRunner::default();
    let gateway = Arc::new(RuntimeGateway::new_for_tests_with_gate(
        Box::new(FakeDocker::new("same")),
        Box::new(lifecycle_runner.clone()),
        gate.clone(),
    ));
    gateway.connect_runtime(None).await.unwrap();
    let definition_calls = Arc::new(AtomicUsize::new(0));
    let definitions = Arc::new(DefinitionCache::new(
        Arc::new(DefinitionCountingRunner(definition_calls.clone())),
        gateway.clone(),
        Arc::new(FixedClock),
        Arc::new(OperationLockManager::new(
            std::env::temp_dir().join(format!("colui-{}.lock", uuid::Uuid::new_v4())),
        )),
        gate,
        Arc::new(SingleProfile(test_profile())),
    ));

    let lifecycle = tokio::spawn({
        let gateway = gateway.clone();
        async move {
            gateway
                .run_profile(test_profile(), LifecycleOperation::Stop)
                .await
        }
    });
    tokio::time::timeout(Duration::from_secs(1), lifecycle_runner.started.notified())
        .await
        .unwrap();
    let definition = tokio::spawn(async move {
        let mut profile = test_profile();
        profile.id = ProfileId::new(uuid::Uuid::from_u128(4));
        definitions.refresh_definition(profile).await
    });
    tokio::task::yield_now().await;
    assert_eq!(definition_calls.load(Ordering::Acquire), 0);

    lifecycle_runner.release.notify_one();
    assert!(lifecycle.await.unwrap().is_ok());
    assert!(definition.await.unwrap().is_ok());
    assert_eq!(definition_calls.load(Ordering::Acquire), 1);
}

#[tokio::test]
async fn reconnect_invalidates_old_api_observation() {
    let docker = Arc::new(StaleDocker::default());
    let started = docker.started.clone();
    let release = docker.release.clone();
    let gateway = Arc::new(RuntimeGateway::new_for_tests(
        Box::new(StaleDocker {
            started,
            release,
            lists: AtomicUsize::new(0),
        }),
        Box::new(FakeRunner::new("same")),
    ));
    gateway.connect_runtime(None).await.unwrap();
    let old_session = ready_session_id(&gateway).await;

    let list = tokio::spawn({
        let gateway = gateway.clone();
        async move { gateway.list_containers().await }
    });
    tokio::time::timeout(Duration::from_secs(1), docker.started.notified())
        .await
        .unwrap();
    gateway.reconnect_runtime(None).await.unwrap();
    let new_session = ready_session_id(&gateway).await;
    docker.release.notify_one();

    let error = list.await.unwrap().unwrap_err();
    assert_ne!(old_session, new_session);
    assert_eq!(error.code, AppErrorCode::RuntimeUnavailable);
    assert!(matches!(
        gateway.session_state().await.unwrap(),
        colui_domain::RuntimeSessionState::Ready(_)
    ));
    assert!(gateway.list_containers().await.is_ok());
}

#[derive(Clone)]
struct FakeDocker {
    fingerprint: String,
}
impl FakeDocker {
    fn new(fingerprint: &str) -> Self {
        Self {
            fingerprint: fingerprint.into(),
        }
    }
}
impl colui_adapters::runtime::DockerControl for FakeDocker {
    fn logs(
        &self,
        _: colui_domain::ContainerId,
    ) -> colui_app::RuntimeFuture<'_, colui_app::ContainerLogs> {
        panic!("no logs")
    }
    fn action(
        &self,
        _: colui_domain::ContainerId,
        _: colui_app::ContainerAction,
    ) -> colui_app::RuntimeFuture<'_, ()> {
        panic!("not a container action test")
    }
    fn info(&self) -> colui_app::RuntimeFuture<'_, DaemonFingerprint> {
        let fp = self.fingerprint.clone();
        Box::pin(async move { Ok(fp_for(&fp)) })
    }
    fn list(&self) -> colui_app::RuntimeFuture<'_, Vec<ContainerObservation>> {
        Box::pin(async { Ok(vec![]) })
    }
    fn inspect(&self, _: &ContainerId) -> colui_app::RuntimeFuture<'_, ContainerDetails> {
        Box::pin(async {
            Err(AppError::new(
                AppErrorCode::RuntimeUnavailable,
                "inspect",
                None,
                "missing",
            ))
        })
    }
}

impl colui_adapters::runtime::DockerControl for StaleDocker {
    fn logs(
        &self,
        _: colui_domain::ContainerId,
    ) -> colui_app::RuntimeFuture<'_, colui_app::ContainerLogs> {
        panic!("no logs")
    }
    fn action(
        &self,
        _: colui_domain::ContainerId,
        _: colui_app::ContainerAction,
    ) -> colui_app::RuntimeFuture<'_, ()> {
        panic!("not a container action test")
    }
    fn info(&self) -> colui_app::RuntimeFuture<'_, DaemonFingerprint> {
        Box::pin(async { Ok(fp_for("same")) })
    }
    fn list(&self) -> colui_app::RuntimeFuture<'_, Vec<ContainerObservation>> {
        let started = self.started.clone();
        let release = self.release.clone();
        let first = self.lists.fetch_add(1, Ordering::AcqRel) == 0;
        Box::pin(async move {
            if first {
                started.notify_one();
                tokio::time::timeout(Duration::from_secs(1), release.notified())
                    .await
                    .unwrap();
            }
            Ok(vec![])
        })
    }
    fn inspect(&self, _: &ContainerId) -> colui_app::RuntimeFuture<'_, ContainerDetails> {
        Box::pin(async {
            Err(AppError::new(
                AppErrorCode::RuntimeUnavailable,
                "inspect",
                None,
                "missing",
            ))
        })
    }
}

struct FakeRunner {
    fingerprint: String,
}

struct ContextRunner;

impl ComposeRunner for ContextRunner {
    fn invoke(
        &self,
        invocation: ComposeInvocation,
    ) -> colui_app::RuntimeFuture<'_, ComposeProcessResult> {
        Box::pin(async move {
            let stdout = if invocation.args.first().map(String::as_str) == Some("context") {
                "unix:///context/docker.sock\n"
            } else {
                "ID: same\nServer Version: 1\nOSType: linux\nArchitecture: x86_64\n"
            };
            Ok(ComposeProcessResult::completed(
                0,
                stdout,
                "",
                Duration::from_millis(1),
            ))
        })
    }
}

#[derive(Default)]
struct RecordingRunner {
    last: Arc<std::sync::Mutex<Option<ComposeInvocation>>>,
}

#[derive(Clone, Default)]
struct BlockingInfoRunner {
    started: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
    calls: Arc<AtomicUsize>,
}

impl ComposeRunner for BlockingInfoRunner {
    fn invoke(&self, _: ComposeInvocation) -> colui_app::RuntimeFuture<'_, ComposeProcessResult> {
        let started = self.started.clone();
        let release = self.release.clone();
        let call = self.calls.fetch_add(1, Ordering::AcqRel);
        Box::pin(async move {
            if call > 0 {
                return Ok(ComposeProcessResult::completed(
                    0,
                    "ID: same\nServer Version: 1\nOSType: linux\nArchitecture: x86_64\n",
                    "",
                    Duration::ZERO,
                ));
            }
            started.notify_one();
            tokio::time::timeout(Duration::from_secs(1), release.notified())
                .await
                .unwrap();
            Ok(ComposeProcessResult::completed(
                0,
                "ID: same\nServer Version: 1\nOSType: linux\nArchitecture: x86_64\n",
                "",
                Duration::ZERO,
            ))
        })
    }
}

impl ComposeRunner for RecordingRunner {
    fn invoke(
        &self,
        invocation: ComposeInvocation,
    ) -> colui_app::RuntimeFuture<'_, ComposeProcessResult> {
        if invocation.args == vec!["info"] {
            return Box::pin(async {
                Ok(ComposeProcessResult::completed(
                    0,
                    "ID: same\nServer Version: 1\nOSType: linux\nArchitecture: x86_64\n",
                    "",
                    Duration::ZERO,
                ))
            });
        }
        *self.last.lock().unwrap() = Some(invocation);
        Box::pin(async { Ok(ComposeProcessResult::completed(0, "", "", Duration::ZERO)) })
    }
}

struct StatusRunner {
    result: ComposeProcessResult,
}

struct LifecycleStatusRunner {
    result: ComposeProcessResult,
}

#[derive(Default)]
struct CountingRunner {
    active: Arc<AtomicUsize>,
    maximum_active: Arc<AtomicUsize>,
    observed_args: Arc<std::sync::Mutex<Vec<Vec<String>>>>,
}

#[derive(Default)]
struct StaleDocker {
    started: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
    lists: AtomicUsize,
}

async fn ready_session_id(gateway: &RuntimeGateway) -> uuid::Uuid {
    match gateway.session_state().await.unwrap() {
        colui_domain::RuntimeSessionState::Ready(context) => *context.session_id.as_uuid(),
        state => panic!("expected ready state, got {state:?}"),
    }
}

#[derive(Clone)]
struct BlockingRunner {
    started: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
    calls: Arc<AtomicUsize>,
}
impl Default for BlockingRunner {
    fn default() -> Self {
        Self {
            started: Arc::new(tokio::sync::Notify::new()),
            release: Arc::new(tokio::sync::Notify::new()),
            calls: Arc::new(AtomicUsize::new(0)),
        }
    }
}
impl colui_app::ComposeRunner for BlockingRunner {
    fn invoke(&self, _: ComposeInvocation) -> colui_app::RuntimeFuture<'_, ComposeProcessResult> {
        let started = self.started.clone();
        let release = self.release.clone();
        let calls = self.calls.fetch_add(1, Ordering::Relaxed);
        Box::pin(async move {
            if calls == 0 {
                return Ok(ComposeProcessResult::completed(
                    0,
                    "ID: same\nServer Version: 1\nOSType: linux\nArchitecture: x86_64\n",
                    "",
                    Duration::ZERO,
                ));
            }
            started.notify_one();
            tokio::time::timeout(Duration::from_secs(1), release.notified())
                .await
                .unwrap();
            Ok(ComposeProcessResult::completed(0, "", "", Duration::ZERO))
        })
    }
}
impl colui_app::ComposeRunner for StatusRunner {
    fn invoke(&self, _: ComposeInvocation) -> colui_app::RuntimeFuture<'_, ComposeProcessResult> {
        let result = self.result.clone();
        Box::pin(async move { Ok(result) })
    }
}

impl colui_app::ComposeRunner for LifecycleStatusRunner {
    fn invoke(
        &self,
        invocation: ComposeInvocation,
    ) -> colui_app::RuntimeFuture<'_, ComposeProcessResult> {
        let result = self.result.clone();
        Box::pin(async move {
            if invocation.args == vec!["info"] {
                return Ok(ComposeProcessResult::completed(
                    0,
                    "ID: same\nServer Version: 1\nOSType: linux\nArchitecture: x86_64\n",
                    "",
                    Duration::ZERO,
                ));
            }
            Ok(result)
        })
    }
}

impl colui_app::ComposeRunner for CountingRunner {
    fn invoke(
        &self,
        invocation: ComposeInvocation,
    ) -> colui_app::RuntimeFuture<'_, ComposeProcessResult> {
        if invocation.args == vec!["info"] {
            return Box::pin(async {
                Ok(ComposeProcessResult::completed(
                    0,
                    "ID: same\nServer Version: 1\nOSType: linux\nArchitecture: x86_64\n",
                    "",
                    Duration::ZERO,
                ))
            });
        }
        self.observed_args.lock().unwrap().push(invocation.args);
        let active = &self.active;
        let maximum_active = &self.maximum_active;
        Box::pin(async move {
            let current = active.fetch_add(1, Ordering::AcqRel) + 1;
            maximum_active.fetch_max(current, Ordering::AcqRel);
            tokio::time::sleep(Duration::from_millis(100)).await;
            active.fetch_sub(1, Ordering::AcqRel);
            Ok(ComposeProcessResult::completed(0, "", "", Duration::ZERO))
        })
    }
}
impl FakeRunner {
    fn new(fingerprint: &str) -> Self {
        Self {
            fingerprint: fingerprint.into(),
        }
    }
}
impl colui_app::ComposeRunner for FakeRunner {
    fn invoke(
        &self,
        invocation: ComposeInvocation,
    ) -> colui_app::RuntimeFuture<'_, ComposeProcessResult> {
        Box::pin(async move {
            if invocation.args == vec!["info"] {
                let fp = self.fingerprint.clone();
                Ok(ComposeProcessResult::completed(
                    0,
                    format!("ID: {fp}\nServer Version: 1\nOSType: linux\nArchitecture: x86_64\n"),
                    "",
                    Duration::ZERO,
                ))
            } else {
                Ok(ComposeProcessResult::completed(0, "", "", Duration::ZERO))
            }
        })
    }
}
fn fp_for(value: &str) -> DaemonFingerprint {
    DaemonFingerprint::new(value, "1", "linux", "x86_64")
}
fn fake_invocation() -> ComposeInvocation {
    ComposeInvocation {
        executable: PathBuf::from("docker"),
        args: vec![],
        working_directory: PathBuf::from("/tmp"),
        environment: BTreeMap::new(),
        deadline: Instant::now() + Duration::from_secs(1),
    }
}

fn invocation_with_args(profile: &str) -> ComposeInvocation {
    let mut invocation = fake_invocation();
    invocation.args = vec![profile.into()];
    invocation
}
