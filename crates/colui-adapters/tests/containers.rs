use colui_adapters::runtime::{DockerApiAdapter, DockerControl, RuntimeGateway};
use colui_adapters::{InventoryCoordinator, OperationLockManager};
use colui_app::*;
use colui_domain::*;
use std::sync::{
    atomic::{AtomicBool, AtomicUsize, Ordering},
    Arc, Mutex,
};
use std::time::Duration;
use tokio::sync::{Notify, Semaphore};

async fn wire_logs(body: Vec<u8>, status: &str) -> (Result<ContainerLogs, AppError>, String) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let status = status.to_owned();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
        }
        let kind = if body.first().is_some_and(|b| *b <= 2) {
            "multiplexed"
        } else {
            "raw"
        };
        write!(stream,"HTTP/1.1 {status}\r\nContent-Type: application/vnd.docker.{kind}-stream\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",body.len()).unwrap();
        let _ = stream.write_all(&body);
        String::from_utf8(request).unwrap()
    });
    let docker = bollard::Docker::connect_with_http(
        &format!("http://{address}"),
        30,
        bollard::API_DEFAULT_VERSION,
    )
    .unwrap();
    let result = DockerApiAdapter::new(
        docker,
        DockerEndpoint::try_from(format!("http://{address}").as_str()).unwrap(),
    )
    .logs(ContainerId("abc".into()))
    .await;
    (result, server.join().unwrap())
}
fn frame(stream: u8, bytes: &[u8]) -> Vec<u8> {
    let mut result = vec![stream, 0, 0, 0];
    result.extend_from_slice(&(bytes.len() as u32).to_be_bytes());
    result.extend_from_slice(bytes);
    result
}
#[tokio::test]
async fn logs_raw_http_options_framing_order_tty_and_retention() {
    for size in [0_usize, 262_144, 262_145, 700_000] {
        let payload: Vec<u8> = (0..size).map(|n| b'a' + (n % 26) as u8).collect();
        for tty in [false, true] {
            let wire = if tty {
                payload.clone()
            } else {
                payload
                    .chunks(1031)
                    .enumerate()
                    .flat_map(|(n, c)| frame(1 + (n % 2) as u8, c))
                    .collect()
            };
            let (result, request) = wire_logs(wire, "200 OK").await;
            let logs = result.unwrap();
            assert_eq!(
                logs.text.as_bytes(),
                &payload[size.saturating_sub(262_144)..]
            );
            assert_eq!(logs.retained_bytes as usize, size.min(262_144));
            assert_eq!(logs.truncated, size > 262_144);
            let line = request.lines().next().unwrap();
            assert!(
                line.starts_with("GET /v1.47/containers/abc/logs?"),
                "{line}"
            );
            for option in [
                "stdout=true",
                "stderr=true",
                "follow=false",
                "timestamps=false",
                "since=0",
                "until=0",
                "tail=4096",
            ] {
                assert!(line.contains(option), "{line}");
            }
        }
    }
    let mut wire = frame(1, b"out");
    wire.extend(frame(2, b"err"));
    wire.extend(frame(1, &[0xe2, 0x82]));
    assert_eq!(
        wire_logs(wire, "200 OK").await.0.unwrap().text,
        "outerr\u{fffd}"
    );
}
#[tokio::test]
async fn logs_raw_http_transfer_limit_and_disappearance_are_sanitized() {
    let below_cap = vec![b'x'; 8 * 1024 * 1024 - 1];
    let result = wire_logs(below_cap, "200 OK").await.0.unwrap();
    assert_eq!(result.retained_bytes, 262_144);
    assert!(result.truncated);
    let at_cap = frame(1, &vec![b'x'; 8 * 1024 * 1024]);
    let error = wire_logs(at_cap, "200 OK").await.0.unwrap_err();
    assert_eq!(error.code, AppErrorCode::ContainerOperationFailed);
    assert!(error.retryable);
    for status in ["404 Not Found", "500 Internal Server Error"] {
        let error = wire_logs(br#"{"message":"SECRET output"}"#.to_vec(), status)
            .await
            .0
            .unwrap_err();
        assert_eq!(error.code, AppErrorCode::ContainerOperationFailed);
        assert_eq!(error.subject.unwrap().id, "abc");
        assert!(!error.message.contains("SECRET"));
    }
}

#[tokio::test]
async fn logs_transfer_cap_applies_before_large_multiplexed_frame_finishes() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            request.push(stream.read_u8().await.unwrap());
        }
        stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/vnd.docker.multiplexed-stream\r\nContent-Length: 4294967303\r\n\r\n\x01\0\0\0\xff\xff\xff\xff").await.unwrap();
        let chunk = [b'x'; 8192];
        for _ in 0..1024 {
            if stream.write_all(&chunk).await.is_err() {
                return;
            }
        }
        // Never finish the announced frame or response. The cap must close it first.
        match stream.read(&mut [0]).await {
            Ok(0) => (),
            Err(e) if e.kind() == std::io::ErrorKind::ConnectionReset => (),
            other => panic!("expected client closure: {other:?}"),
        }
    });
    let endpoint = DockerEndpoint::try_from(format!("http://{address}").as_str()).unwrap();
    let docker =
        bollard::Docker::connect_with_http(endpoint.as_str(), 30, bollard::API_DEFAULT_VERSION)
            .unwrap();
    let adapter = DockerApiAdapter::new(docker, endpoint);
    let error = tokio::time::timeout(
        Duration::from_secs(2),
        adapter.logs(ContainerId("abc".into())),
    )
    .await
    .expect("cap must precede ten-second deadline")
    .unwrap_err();
    assert!(error.retryable);
    assert_eq!(error.code, AppErrorCode::ContainerOperationFailed);
    server.await.unwrap();
}

#[cfg(unix)]
#[tokio::test]
async fn logs_unix_transport_preserves_tty_control_bytes_and_unterminated_utf8() {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    for multiplexed in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("docker.sock");
        let listener = tokio::net::UnixListener::bind(&socket).unwrap();
        let server = tokio::spawn(async move {
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = Vec::new();
            while !request.ends_with(b"\r\n\r\n") {
                request.push(stream.read_u8().await.unwrap());
            }
            let kind = if multiplexed { "multiplexed" } else { "raw" };
            stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/vnd.docker.{kind}-stream\r\nTransfer-Encoding: chunked\r\nConnection: close\r\n\r\n").as_bytes()).await.unwrap();
            let payload = [0, 1, 2, b'x', 0xe2, 0x82];
            let bytes = if multiplexed {
                let mut bytes = frame(1, &payload[..3]);
                bytes.extend(frame(2, &payload[3..]));
                bytes
            } else {
                payload.to_vec()
            };
            for byte in bytes {
                stream
                    .write_all(&[b'1', b'\r', b'\n', byte, b'\r', b'\n'])
                    .await
                    .unwrap();
            }
            stream.write_all(b"0\r\n\r\n").await.unwrap();
        });
        let endpoint =
            DockerEndpoint::try_from(format!("unix://{}", socket.display()).as_str()).unwrap();
        let docker =
            bollard::Docker::connect_with_unix(endpoint.as_str(), 30, bollard::API_DEFAULT_VERSION)
                .unwrap();
        let logs = DockerApiAdapter::new(docker, endpoint)
            .logs(ContainerId("abc".into()))
            .await
            .unwrap();
        assert_eq!(logs.text, "\0\u{1}\u{2}x\u{fffd}");
        assert_eq!(logs.retained_bytes, 6);
        assert!(!logs.truncated);
        server.await.unwrap();
    }
}

#[tokio::test]
async fn logs_gateway_checks_before_capture_and_after_completion_and_drops_cancelled_future() {
    let (docker, gateway, _, session) = setup().await;
    let request = ContainerLogsRequest {
        container_id: ContainerId("abc".into()),
        runtime_session_id: session,
    };
    let task = tokio::spawn({
        let gateway = gateway.clone();
        let request = request.clone();
        async move { gateway.read_logs(request).await }
    });
    docker.action_started.notified().await;
    gateway.disconnect_runtime().await.unwrap();
    docker.action_release.add_permits(1);
    let error = task.await.unwrap().unwrap_err();
    assert!(!error.retryable);
    assert!(!format!("{error:?}").contains("SECRET"));
    assert!(gateway.read_logs(request).await.is_err());
    assert_eq!(docker.actions.load(Ordering::SeqCst), 1);
    gateway.connect_runtime(None).await.unwrap();
    let request = ContainerLogsRequest {
        container_id: ContainerId("abc".into()),
        runtime_session_id: gateway.api_read_context().await.unwrap().session_id,
    };
    let task = tokio::spawn({
        let gateway = gateway.clone();
        async move { gateway.read_logs(request).await }
    });
    docker.action_started.notified().await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(docker.action_release.available_permits(), 0);
    assert_eq!(gateway.created_client_count(), 2);
}

async fn stalled_log_request(
    send_body: bool,
) -> (
    tokio::task::JoinHandle<Result<ContainerLogs, AppError>>,
    tokio::task::JoinHandle<usize>,
) {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let address = listener.local_addr().unwrap();
    let (started, received) = tokio::sync::oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await.unwrap();
        let mut request = Vec::new();
        while !request.ends_with(b"\r\n\r\n") {
            request.push(stream.read_u8().await.unwrap());
        }
        if send_body {
            stream.write_all(b"HTTP/1.1 200 OK\r\nContent-Type: application/vnd.docker.raw-stream\r\nContent-Length: 1000\r\n\r\nSECRET").await.unwrap();
        }
        started.send(()).unwrap();
        match stream.read(&mut [0]).await {
            Ok(n) => n,
            Err(error) if error.kind() == std::io::ErrorKind::ConnectionReset => 0,
            Err(error) => panic!("unexpected transport error: {error}"),
        }
    });
    let task = tokio::spawn(async move {
        let endpoint = DockerEndpoint::try_from(format!("http://{address}").as_str()).unwrap();
        let docker =
            bollard::Docker::connect_with_http(endpoint.as_str(), 30, bollard::API_DEFAULT_VERSION)
                .unwrap();
        DockerApiAdapter::new(docker, endpoint)
            .logs(ContainerId("abc".into()))
            .await
    });
    received.await.unwrap();
    (task, server)
}

#[tokio::test]
async fn logs_deadline_is_ten_monotonic_seconds_including_headers_and_body() {
    for send_body in [false, true] {
        let (task, server) = stalled_log_request(send_body).await;
        tokio::time::pause();
        tokio::time::advance(Duration::from_secs(9)).await;
        assert!(!task.is_finished());
        tokio::time::advance(Duration::from_secs(1)).await;
        let error = task.await.unwrap().unwrap_err();
        assert_eq!(error.code, AppErrorCode::ContainerOperationFailed);
        assert!(error.retryable);
        assert!(!format!("{error:?}").contains("SECRET"));
        tokio::time::resume();
        assert_eq!(
            tokio::time::timeout(Duration::from_secs(2), server)
                .await
                .unwrap()
                .unwrap(),
            0
        );
    }
}

#[tokio::test]
async fn logs_cancellation_closes_transport_without_returning_partial_text() {
    let (task, server) = stalled_log_request(true).await;
    task.abort();
    assert!(task.await.unwrap_err().is_cancelled());
    assert_eq!(
        tokio::time::timeout(Duration::from_secs(2), server)
            .await
            .unwrap()
            .unwrap(),
        0
    );
}

#[tokio::test]
async fn logs_incomplete_frame_is_failure_not_partial_success() {
    let mut bytes = frame(1, b"SECRET");
    bytes.pop();
    let error = wire_logs(bytes, "200 OK").await.0.unwrap_err();
    assert_eq!(error.code, AppErrorCode::ContainerOperationFailed);
    assert!(!format!("{error:?}").contains("SECRET"));
}

#[test]
fn ports_list_keeps_partial_bindings() {
    let summary = serde_json::from_str(r#"{"Id":"abc","Ports":[{"IP":"::","PrivatePort":80,"Type":"tcp"},{"PublicPort":8080,"PrivatePort":80,"Type":"tcp"},{"PrivatePort":53,"Type":"udp"}]}"#).unwrap();
    let ports = colui_adapters::runtime::normalize_container_summary(summary)
        .instance
        .published_ports;
    assert_eq!(ports.len(), 3);
    assert_eq!(ports[0].host_ip.as_deref(), Some("::"));
    assert_eq!(ports[0].host_port, None);
    assert_eq!(ports[1].host_ip, None);
    assert_eq!(ports[1].host_port, Some(8080));
    assert_eq!(ports[2].host_ip, None);
    assert_eq!(ports[2].host_port, None);
}

struct TestClock;
impl Clock for TestClock {
    fn now(&self) -> Timestamp {
        Timestamp("now".into())
    }
    fn monotonic(&self) -> Duration {
        Duration::ZERO
    }
}
struct Runner;
impl ComposeRunner for Runner {
    fn invoke(&self, invocation: ComposeInvocation) -> RuntimeFuture<'_, ComposeProcessResult> {
        assert_eq!(invocation.args, vec!["info"]);
        Box::pin(async {
            Ok(ComposeProcessResult::completed(
                0,
                "ID: same\nServer Version: 1\nOSType: linux\nArchitecture: x86_64\n",
                "",
                Duration::ZERO,
            ))
        })
    }
}
#[derive(Clone)]
struct Docker {
    action_values: Arc<Mutex<Vec<(ContainerId, ContainerAction)>>>,
    lists: Arc<AtomicUsize>,
    actions: Arc<AtomicUsize>,
    list_started: Arc<Notify>,
    list_release: Arc<Semaphore>,
    action_started: Arc<Notify>,
    action_release: Arc<Semaphore>,
    fail_list: Arc<AtomicBool>,
    fail_action: Arc<AtomicBool>,
}
impl Docker {
    fn new() -> Self {
        Self {
            action_values: Arc::new(Mutex::new(vec![])),
            lists: Arc::new(AtomicUsize::new(0)),
            actions: Arc::new(AtomicUsize::new(0)),
            list_started: Arc::new(Notify::new()),
            list_release: Arc::new(Semaphore::new(0)),
            action_started: Arc::new(Notify::new()),
            action_release: Arc::new(Semaphore::new(0)),
            fail_list: Arc::new(AtomicBool::new(false)),
            fail_action: Arc::new(AtomicBool::new(false)),
        }
    }
}
fn observation(id: &str) -> ContainerObservation {
    ContainerObservation::new(
        ContainerInstance {
            id: ContainerId(id.into()),
            name: id.into(),
            image: "alpine".into(),
            state: ContainerState::Stopped,
            status_text: "exited".into(),
            service_name: None,
            published_ports: vec![],
        },
        None,
    )
}
impl DockerControl for Docker {
    fn logs(&self, id: ContainerId) -> RuntimeFuture<'_, ContainerLogs> {
        Box::pin(async move {
            self.actions.fetch_add(1, Ordering::SeqCst);
            self.action_started.notify_one();
            self.action_release.acquire().await.unwrap().forget();
            let mut ring = LogByteRing::default();
            ring.push(b"SECRET");
            Ok(ring.finish(id, Timestamp("now".into())))
        })
    }
    fn info(&self) -> RuntimeFuture<'_, DaemonFingerprint> {
        Box::pin(async { Ok(DaemonFingerprint::new("same", "1", "linux", "x86_64")) })
    }
    fn list(&self) -> RuntimeFuture<'_, Vec<ContainerObservation>> {
        Box::pin(async {
            let n = self.lists.fetch_add(1, Ordering::SeqCst) + 1;
            self.list_started.notify_one();
            self.list_release.acquire().await.unwrap().forget();
            if self.fail_list.load(Ordering::SeqCst) {
                return Err(AppError::new(
                    AppErrorCode::RuntimeUnavailable,
                    "list",
                    None,
                    "offline",
                ));
            }
            Ok(vec![
                observation("abc"),
                observation("def"),
                observation(&format!("list-{n}")),
            ])
        })
    }
    fn inspect(&self, _: &ContainerId) -> RuntimeFuture<'_, ContainerDetails> {
        panic!("no inspect")
    }
    fn action(&self, id: ContainerId, action: ContainerAction) -> RuntimeFuture<'_, ()> {
        Box::pin(async move {
            assert!(["abc", "def"].contains(&id.0.as_str()));
            self.action_values.lock().unwrap().push((id, action));
            self.actions.fetch_add(1, Ordering::SeqCst);
            self.action_started.notify_one();
            self.action_release.acquire().await.unwrap().forget();
            if self.fail_action.load(Ordering::SeqCst) {
                Err(AppError::new(
                    AppErrorCode::RuntimeUnavailable,
                    "daemon",
                    None,
                    "SECRET",
                ))
            } else {
                Ok(())
            }
        })
    }
}
async fn setup() -> (
    Docker,
    Arc<RuntimeGateway>,
    Arc<InventoryCoordinator>,
    RuntimeSessionId,
) {
    let docker = Docker::new();
    let gateway = Arc::new(RuntimeGateway::new_for_tests(
        Box::new(docker.clone()),
        Box::new(Runner),
    ));
    gateway.connect_runtime(None).await.unwrap();
    let session = gateway.api_read_context().await.unwrap().session_id;
    let inventory = Arc::new(InventoryCoordinator::new(
        gateway.clone(),
        Arc::new(TestClock),
    ));
    (docker, gateway, inventory, session)
}
fn refresh(
    inventory: &Arc<InventoryCoordinator>,
) -> tokio::task::JoinHandle<Result<RuntimeInventory, AppError>> {
    let inventory = inventory.clone();
    tokio::spawn(async move { inventory.refresh().await })
}

#[tokio::test]
async fn causal_refresh_waits_for_older_list_then_starts_new_observation_even_if_old_list_fails() {
    for old_fails in [false, true] {
        let (docker, _, inventory, _) = setup().await;
        let old = refresh(&inventory);
        docker.list_started.notified().await;
        let marker = inventory.observation_marker();
        let mut joins = inventory.subscribe_refresh_joins();
        let mut causal = tokio::spawn({
            let inventory = inventory.clone();
            async move { inventory.refresh_after(marker).await }
        });
        joins.recv().await.unwrap();
        assert!(!causal.is_finished());
        assert_eq!(docker.lists.load(Ordering::SeqCst), 1);
        docker.fail_list.store(old_fails, Ordering::SeqCst);
        docker.list_release.add_permits(1);
        assert_eq!(old.await.unwrap().is_err(), old_fails);
        tokio::select! {
            result = &mut causal => panic!("pre-action list satisfied causal refresh: {result:?}"),
            _ = docker.list_started.notified() => {}
        }
        docker.fail_list.store(false, Ordering::SeqCst);
        assert!(!causal.is_finished());
        docker.list_release.add_permits(1);
        let result = causal.await.unwrap().unwrap();
        assert_eq!(result.containers[2].id.0, "list-2");
        assert_eq!(docker.lists.load(Ordering::SeqCst), 2);
    }
}

#[tokio::test]
async fn causal_refresh_coalesces_only_with_list_started_after_marker() {
    let (docker, _, inventory, _) = setup().await;
    let marker = inventory.observation_marker();
    let new = refresh(&inventory);
    docker.list_started.notified().await;
    let mut joins = inventory.subscribe_refresh_joins();
    let causal = tokio::spawn({
        let inventory = inventory.clone();
        async move { inventory.refresh_after(marker).await }
    });
    joins.recv().await.unwrap();
    docker.list_release.add_permits(1);
    assert_eq!(causal.await.unwrap().unwrap(), new.await.unwrap().unwrap());
    assert_eq!(docker.lists.load(Ordering::SeqCst), 1);
}

fn action(
    gateway: &Arc<RuntimeGateway>,
    inventory: &Arc<InventoryCoordinator>,
    locks: &Arc<OperationLockManager>,
    session: RuntimeSessionId,
    id: &str,
) -> tokio::task::JoinHandle<Result<ContainerActionResult, AppError>> {
    let (gateway, inventory, locks, id) = (
        gateway.clone(),
        inventory.clone(),
        locks.clone(),
        ContainerId(id.into()),
    );
    tokio::spawn(async move {
        RunContainerAction::new(gateway.as_ref(), inventory.as_ref(), locks.as_ref())
            .execute(id, ContainerAction::Start, session)
            .await
    })
}

#[tokio::test]
async fn duplicate_id_conflicts_independent_ids_run_and_recovery_is_excluded_through_refresh() {
    let (docker, gateway, inventory, session) = setup().await;
    docker.list_release.add_permits(1);
    inventory.refresh().await.unwrap();
    docker.list_started.notified().await;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("registry.recovery.lock");
    let locks = Arc::new(OperationLockManager::new(path.clone()));
    let other_process = OperationLockManager::new(path);
    let first = action(&gateway, &inventory, &locks, session, "abc");
    docker.action_started.notified().await;
    assert_eq!(
        action(&gateway, &inventory, &locks, session, "abc")
            .await
            .unwrap()
            .unwrap_err()
            .code,
        AppErrorCode::OperationConflict
    );
    let second = action(&gateway, &inventory, &locks, session, "def");
    docker.action_started.notified().await;
    assert_eq!(docker.actions.load(Ordering::SeqCst), 2);
    assert!(locks.acquire_recovery().is_err());
    assert!(other_process.acquire_recovery().is_err());
    docker.action_release.add_permits(2);
    docker.list_started.notified().await;
    assert!(locks.acquire_container("abc").is_err());
    assert!(locks.acquire_container("def").is_err());
    assert!(other_process.acquire_recovery().is_err());
    docker.list_release.add_permits(2);
    first.await.unwrap().unwrap();
    second.await.unwrap().unwrap();
    assert!(locks.acquire_container("abc").is_ok());
    assert!(other_process.acquire_recovery().is_ok());
}

#[tokio::test]
async fn daemon_success_survives_disconnect_and_returns_retained_stale_inventory() {
    let (docker, gateway, inventory, session) = setup().await;
    docker.list_release.add_permits(1);
    let before = inventory.refresh().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let locks = Arc::new(OperationLockManager::new(
        dir.path().join("registry.recovery.lock"),
    ));
    let pending = action(&gateway, &inventory, &locks, session, "abc");
    docker.action_started.notified().await;
    gateway.disconnect_runtime().await.unwrap();
    docker.action_release.add_permits(1);
    let result = pending.await.unwrap().unwrap();
    assert_eq!(
        result.observation,
        ContainerActionObservation::IndeterminateAfterSessionChange
    );
    assert_eq!(result.inventory.generation, before.generation);
    assert_eq!(result.inventory.containers, before.containers);
    assert_eq!(result.inventory.freshness, InventoryFreshness::Stale);
    assert_eq!(
        result.inventory.error.unwrap().code,
        AppErrorCode::RuntimeUnavailable
    );
}

#[tokio::test]
async fn gateway_rechecks_expected_session_before_any_mutation() {
    let (docker, gateway, _, session) = setup().await;
    gateway.reconnect_runtime(None).await.unwrap();
    let error = gateway
        .run_container(ContainerId("abc".into()), ContainerAction::Stop, session)
        .await
        .unwrap_err();
    assert_eq!(error.code, AppErrorCode::ContainerOperationFailed);
    assert_eq!(docker.actions.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn all_action_verbs_pass_unchanged_through_use_case_and_gateway_without_compose() {
    let (docker, gateway, inventory, session) = setup().await;
    docker.list_release.add_permits(4);
    inventory.refresh().await.unwrap();
    docker.action_release.add_permits(3);
    let dir = tempfile::tempdir().unwrap();
    let locks = OperationLockManager::new(dir.path().join("registry.recovery.lock"));
    let use_case = RunContainerAction::new(gateway.as_ref(), inventory.as_ref(), &locks);
    for action in [
        ContainerAction::Start,
        ContainerAction::Stop,
        ContainerAction::Restart,
    ] {
        let result = use_case
            .execute(ContainerId("abc".into()), action, session)
            .await
            .unwrap();
        assert_eq!(result.action, action);
        assert_eq!(
            result.observation,
            ContainerActionObservation::ConfirmedInSession
        );
    }
    assert_eq!(
        *docker.action_values.lock().unwrap(),
        vec![
            (ContainerId("abc".into()), ContainerAction::Start),
            (ContainerId("abc".into()), ContainerAction::Stop),
            (ContainerId("abc".into()), ContainerAction::Restart),
        ]
    );
    assert_eq!(docker.lists.load(Ordering::SeqCst), 4);
}

#[tokio::test]
async fn failed_daemon_action_does_not_list_or_retain_operation_lease() {
    let (docker, gateway, inventory, session) = setup().await;
    docker.list_release.add_permits(1);
    let before = inventory.refresh().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let locks = Arc::new(OperationLockManager::new(
        dir.path().join("registry.recovery.lock"),
    ));
    docker.fail_action.store(true, Ordering::SeqCst);
    docker.action_release.add_permits(1);
    let error = action(&gateway, &inventory, &locks, session, "abc")
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, AppErrorCode::ContainerOperationFailed);
    assert_eq!(error.subject.unwrap().id, "abc");
    assert!(!error.message.contains("SECRET"));
    assert_eq!(docker.actions.load(Ordering::SeqCst), 1);
    assert_eq!(docker.lists.load(Ordering::SeqCst), 1);
    assert_eq!(inventory.current_inventory().await.unwrap(), before);
    assert!(locks.acquire_recovery().is_ok());
}

#[tokio::test]
async fn successful_action_never_returns_pre_action_list_and_refresh_failure_stays_successful() {
    for fail_refresh in [false, true] {
        let (docker, gateway, inventory, session) = setup().await;
        docker.list_release.add_permits(1);
        inventory.refresh().await.unwrap();
        docker.list_started.notified().await;
        let old = refresh(&inventory);
        docker.list_started.notified().await;
        let dir = tempfile::tempdir().unwrap();
        let locks = Arc::new(OperationLockManager::new(
            dir.path().join("registry.recovery.lock"),
        ));
        let mut joins = inventory.subscribe_refresh_joins();
        let pending = action(&gateway, &inventory, &locks, session, "abc");
        docker.action_started.notified().await;
        docker.action_release.add_permits(1);
        joins.recv().await.unwrap();
        docker.list_release.add_permits(1);
        let pre_action = old.await.unwrap().unwrap();
        docker.list_started.notified().await;
        assert!(!pending.is_finished());
        assert!(locks.acquire_container("abc").is_err());
        docker.fail_list.store(fail_refresh, Ordering::SeqCst);
        docker.list_release.add_permits(1);
        let result = pending.await.unwrap().unwrap();
        assert_eq!(
            result.observation,
            ContainerActionObservation::ConfirmedInSession
        );
        assert_eq!(docker.lists.load(Ordering::SeqCst), 3);
        if fail_refresh {
            assert_eq!(result.inventory.generation, pre_action.generation);
            assert_eq!(result.inventory.containers, pre_action.containers);
            assert_eq!(result.inventory.freshness, InventoryFreshness::Stale);
            assert_eq!(
                result.inventory.error.unwrap().code,
                AppErrorCode::RuntimeUnavailable
            );
        } else {
            assert_eq!(result.inventory.containers[2].id.0, "list-3");
            assert_eq!(result.inventory.generation, 3);
        }
    }
}

#[tokio::test]
async fn recovery_lease_blocks_action_before_docker_work() {
    let (docker, gateway, inventory, session) = setup().await;
    docker.list_release.add_permits(1);
    inventory.refresh().await.unwrap();
    let dir = tempfile::tempdir().unwrap();
    let locks = Arc::new(OperationLockManager::new(
        dir.path().join("registry.recovery.lock"),
    ));
    let _recovery = locks.acquire_recovery().unwrap();
    let error = action(&gateway, &inventory, &locks, session, "abc")
        .await
        .unwrap()
        .unwrap_err();
    assert_eq!(error.code, AppErrorCode::OperationConflict);
    assert_eq!(docker.actions.load(Ordering::SeqCst), 0);
    assert_eq!(docker.lists.load(Ordering::SeqCst), 1);
}

// Real Bollard client against a tiny HTTP daemon: verifies the wire operation, not a fake mapping.
async fn wire_action(action: ContainerAction, status: &str) -> (Result<(), AppError>, String) {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let status = status.to_owned();
    let server = std::thread::spawn(move || {
        let (mut stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        loop {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            request.push(byte[0]);
            if request.ends_with(b"\r\n\r\n") {
                break;
            }
        }
        let body = if status.starts_with("204") {
            ""
        } else {
            r#"{"message":"SECRET daemon path /private"}"#
        };
        write!(
            stream,
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .unwrap();
        String::from_utf8(request).unwrap()
    });
    let docker = bollard::Docker::connect_with_http(
        &format!("http://{address}"),
        5,
        bollard::API_DEFAULT_VERSION,
    )
    .unwrap();
    let result = DockerApiAdapter::new(
        docker,
        DockerEndpoint::try_from(format!("http://{address}").as_str()).unwrap(),
    )
    .action(ContainerId("abc".into()), action)
    .await;
    (result, server.join().unwrap())
}

#[tokio::test]
async fn exact_bollard_start_stop_restart_calls() {
    for (action, verb) in [
        (ContainerAction::Start, "start"),
        (ContainerAction::Stop, "stop"),
        (ContainerAction::Restart, "restart"),
    ] {
        let (result, request) = wire_action(action, "204 No Content").await;
        result.unwrap();
        let line = request.lines().next().unwrap();
        assert_eq!(line, format!("POST /containers/abc/{verb} HTTP/1.1"));
    }
}

#[tokio::test]
async fn bollard_disappearance_and_daemon_errors_have_sanitized_container_subject() {
    for status in ["404 Not Found", "500 Internal Server Error"] {
        let (result, _) = wire_action(ContainerAction::Start, status).await;
        let error = result.unwrap_err();
        assert_eq!(error.code, AppErrorCode::ContainerOperationFailed);
        assert_eq!(
            error.subject.as_ref().unwrap().kind,
            AppErrorSubjectKind::Container
        );
        assert_eq!(error.subject.as_ref().unwrap().id, "abc");
        assert!(!format!("{error:?}").contains("SECRET"));
        assert!(!error.retryable);
    }
}
