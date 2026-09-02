use colui_adapters::runtime::{
    bollard_fingerprint, build_cli_environment, parse_cli_fingerprint, resolve_endpoint,
    EndpointPreference,
};
use colui_adapters::runtime::{ComposeProcessRunner, TerminationConfig};
use colui_app::{ComposeInvocation, ComposeRunner};
use colui_domain::AppErrorCode;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
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
fn endpoint_resolution_prefers_explicit_then_docker_host_then_socket() {
    assert_eq!(
        resolve_endpoint(Some("unix:///explicit"), Some("unix:///env"))
            .unwrap()
            .as_str(),
        "unix:///explicit"
    );
    assert_eq!(
        resolve_endpoint(None, Some("unix:///env"))
            .unwrap()
            .as_str(),
        "unix:///env"
    );
    assert_eq!(
        resolve_endpoint(None, None).unwrap().as_str(),
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
            .args(["--edition", "2021", "tests/fixtures/fake_compose.rs", "-o"])
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
    let pid: i32 = std::fs::read_to_string(child_pid).unwrap().parse().unwrap();
    for _ in 0..50 {
        if !process_exists(pid) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("child process remains");
}

fn process_exists(pid: i32) -> bool {
    unsafe { libc::kill(pid, 0) == 0 }
}
