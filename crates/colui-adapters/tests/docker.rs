#![cfg(feature = "docker-tests")]

use colui_adapters::runtime::ComposeExecutionGate;
use colui_adapters::runtime::{
    compose_args, ComposeOperation, ComposeProcessRunner, RuntimeGateway,
};
use colui_adapters::{DefinitionCache, InventoryCoordinator, OperationLockManager};
use colui_app::{
    Clock, DefinitionRefresher, DockerApi, LifecycleOperation, LifecycleRuntime, ProfileReader,
    RegistrySnapshot, RuntimeConnector,
};
use colui_domain::{
    ProfileDraft, ProfileId, ProjectProfile, RegistrationOrigin, RuntimeSessionState,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};
use tempfile::TempDir;
use uuid::Uuid;

#[tokio::test]
async fn disposable_compose_fixture_passes_apply_stop_apply_teardown() {
    if !docker_available().await {
        eprintln!("SKIP: local Docker daemon unavailable");
        return;
    }

    let fixture = TempComposeFixture::new(false);
    let profile_before = fixture.profile.clone();
    let gateway = RuntimeGateway::new(Box::new(ComposeProcessRunner::default()));
    assert!(matches!(
        gateway.connect_runtime(None).await.unwrap(),
        RuntimeSessionState::Ready(_)
    ));

    let mut cleanup = CleanupGuard::new(&fixture.profile);
    fixture.apply(&gateway, false).await.unwrap();
    fixture.assert_containers_present(&gateway).await;
    fixture.stop(&gateway).await.unwrap();
    fixture.assert_containers_stopped(&gateway).await;
    fixture.apply(&gateway, false).await.unwrap();
    fixture.tear_down(&gateway).await.unwrap();
    cleanup.disarm();
    fixture.assert_project_resources_absent().await;
    fixture.assert_profile_survives(&profile_before);
}

#[tokio::test]
async fn disposable_compose_fixture_scales_worker_to_two_containers() {
    if !docker_available().await {
        eprintln!("SKIP: local Docker daemon unavailable");
        return;
    }

    let fixture = TempComposeFixture::new(true);
    let profile_before = fixture.profile.clone();
    let gateway = RuntimeGateway::new(Box::new(ComposeProcessRunner::default()));
    assert!(matches!(
        gateway.connect_runtime(None).await.unwrap(),
        RuntimeSessionState::Ready(_)
    ));

    let mut cleanup = CleanupGuard::new(&fixture.profile);
    fixture.apply(&gateway, true).await.unwrap();
    let workers = fixture.containers_with_service(&gateway, "worker").await;
    assert_eq!(workers, 2, "expected two scaled worker containers");
    fixture.stop(&gateway).await.unwrap();
    fixture.assert_containers_stopped(&gateway).await;
    fixture.tear_down(&gateway).await.unwrap();
    cleanup.disarm();
    fixture.assert_project_resources_absent().await;
    fixture.assert_profile_survives(&profile_before);
}

#[tokio::test]
async fn fixture_labels_feed_inventory_and_profile_change_updates_definition_revision() {
    if !docker_available().await {
        eprintln!("SKIP: local Docker daemon unavailable");
        return;
    }

    let fixture = TempComposeFixture::new(false);
    let gateway = Arc::new(RuntimeGateway::new(Box::new(
        ComposeProcessRunner::default(),
    )));
    gateway.connect_runtime(None).await.unwrap();
    let mut cleanup = CleanupGuard::new(&fixture.profile);
    fixture.apply(&gateway, false).await.unwrap();

    let coordinator = InventoryCoordinator::new(gateway.clone(), Arc::new(WallClock));
    let inventory = coordinator.refresh().await.unwrap();
    let project = inventory
        .project_snapshots
        .iter()
        .find(|project| project.compose_project_name == fixture.project_name)
        .expect("official Compose labels produce project snapshot");
    assert!(!project.containers.is_empty());
    assert_eq!(
        project.working_directory.as_deref(),
        fixture.directory.path().to_str()
    );
    assert!(project
        .config_files
        .iter()
        .any(|path| path.ends_with("compose.yml")));

    let cache = DefinitionCache::new(
        Arc::new(ComposeProcessRunner::default()),
        gateway.clone(),
        Arc::new(WallClock),
        Arc::new(OperationLockManager::new()),
        Arc::new(ComposeExecutionGate::new()),
    );
    let before = cache
        .refresh_definition(fixture.profile.clone())
        .await
        .unwrap();
    std::fs::write(
        &fixture.compose_path,
        "services:\n  app:\n    image: alpine:3.20\n    command: [\"sleep\", \"301\"]\n",
    )
    .unwrap();
    let changed = fixture
        .profile
        .clone()
        .with_display_name("Updated Docker fixture".try_into().unwrap())
        .unwrap();
    let after = cache.refresh_definition(changed).await.unwrap();
    assert_ne!(
        before.definition.definition_revision,
        after.definition.definition_revision
    );

    fixture.tear_down(&gateway).await.unwrap();
    cleanup.disarm();
}

struct WallClock;

impl Clock for WallClock {
    fn now(&self) -> colui_domain::Timestamp {
        colui_domain::Timestamp(chrono::Utc::now().to_rfc3339())
    }

    fn monotonic(&self) -> Duration {
        Instant::now().elapsed()
    }
}

async fn docker_available() -> bool {
    let endpoint =
        std::env::var("DOCKER_HOST").unwrap_or_else(|_| "unix:///var/run/docker.sock".into());
    let environment =
        colui_adapters::runtime::build_cli_environment(&endpoint, std::env::vars().collect());
    tokio::process::Command::new("docker")
        .args(["info", "--format", "{{.ServerVersion}}"])
        .env_clear()
        .envs(environment)
        .output()
        .await
        .map(|output| output.status.success())
        .unwrap_or(false)
}

struct TempComposeFixture {
    directory: TempDir,
    profile: ProjectProfile,
    compose_path: PathBuf,
    project_name: String,
}

impl ProfileReader for TempComposeFixture {
    fn load(&self) -> colui_app::StoreFuture<'_, RegistrySnapshot> {
        let profile = self.profile.clone();
        Box::pin(async move {
            Ok(RegistrySnapshot {
                registry_revision: 1,
                profiles: vec![profile],
            })
        })
    }
}

impl TempComposeFixture {
    fn new(with_worker: bool) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let compose_path = directory.path().join("compose.yml");
        let services = if with_worker {
            "  worker:\n    image: alpine:3.20\n    command: [\"sleep\", \"300\"]\n"
        } else {
            "  app:\n    image: alpine:3.20\n    command: [\"sleep\", \"300\"]\n"
        };
        std::fs::write(&compose_path, format!("services:\n{services}")).unwrap();
        let project_name = format!("colui-it-{}", &Uuid::new_v4().simple().to_string()[..12]);
        let profile = ProjectProfile::from_draft(
            ProfileId::new(Uuid::new_v4()),
            ProfileDraft {
                display_name: "Docker integration fixture".try_into().unwrap(),
                compose_project_name: project_name.clone().try_into().unwrap(),
                working_directory: directory.path().to_path_buf(),
                compose_files: vec![PathBuf::from("compose.yml")],
                environment_files: vec![],
                registration_origin: RegistrationOrigin::Manual,
            },
        )
        .unwrap();
        Self {
            directory,
            profile,
            compose_path,
            project_name,
        }
    }

    async fn apply(
        &self,
        gateway: &RuntimeGateway,
        scale_worker: bool,
    ) -> Result<(), colui_domain::AppError> {
        let operation = if scale_worker {
            ComposeOperation::UpScaled {
                service: "worker".into(),
                replicas: 2,
            }
        } else {
            ComposeOperation::Up
        };
        if scale_worker {
            gateway
                .invoke_backend_for_tests(colui_app::ComposeInvocation {
                    executable: "docker".into(),
                    args: compose_args(&self.profile, operation),
                    working_directory: self.profile.working_directory.clone(),
                    environment: std::env::vars().collect(),
                    deadline: Instant::now() + Duration::from_secs(120),
                })
                .await
                .map(|_| ())
        } else {
            gateway
                .run_profile(self.profile.clone(), LifecycleOperation::Apply)
                .await
                .map(|_| ())
        }
    }

    async fn stop(&self, gateway: &RuntimeGateway) -> Result<(), colui_domain::AppError> {
        gateway
            .run_profile(self.profile.clone(), LifecycleOperation::Stop)
            .await
            .map(|_| ())
    }

    async fn tear_down(&self, gateway: &RuntimeGateway) -> Result<(), colui_domain::AppError> {
        gateway
            .run_profile(self.profile.clone(), LifecycleOperation::TearDown)
            .await
            .map(|_| ())
    }

    async fn assert_containers_present(&self, gateway: &RuntimeGateway) {
        let containers = gateway.list_containers().await.unwrap();
        let matching = self.containers_with_exact_project(&containers);
        assert!(
            !matching.is_empty(),
            "expected disposable project containers"
        );
    }

    async fn assert_containers_stopped(&self, gateway: &RuntimeGateway) {
        let containers = gateway.list_containers().await.unwrap();
        let matching = self.containers_with_exact_project(&containers);
        assert!(!matching.is_empty(), "expected project containers");
        assert!(matching
            .iter()
            .all(|container| { container.state == colui_domain::ContainerState::Stopped }));
    }

    async fn containers_with_service(&self, gateway: &RuntimeGateway, service: &str) -> usize {
        let containers = gateway.list_containers().await.unwrap();
        self.containers_with_exact_project(&containers)
            .into_iter()
            .filter(|container| container.service_name.as_deref() == Some(service))
            .count()
    }

    fn containers_with_exact_project(
        &self,
        containers: &[colui_domain::ContainerObservation],
    ) -> Vec<colui_domain::ContainerInstance> {
        containers
            .iter()
            .filter(|observation| {
                observation.compose.as_ref().map(|compose| &compose.project)
                    == Some(&self.project_name)
            })
            .map(|observation| observation.instance.clone())
            .collect()
    }

    async fn assert_project_resources_absent(&self) {
        for kind in ["container", "network", "volume"] {
            let output = self.run_docker_label_query(kind).await;
            assert!(
                output.trim().is_empty(),
                "disposable project {kind}s remain: {}",
                output.trim()
            );
        }
    }

    async fn run_docker_label_query(&self, kind: &str) -> String {
        let endpoint =
            std::env::var("DOCKER_HOST").unwrap_or_else(|_| "unix:///var/run/docker.sock".into());
        let environment =
            colui_adapters::runtime::build_cli_environment(&endpoint, std::env::vars().collect());
        let mut args = vec![kind.to_owned(), "ls".into(), "-q".into()];
        if kind == "container" {
            args.push("--all".into());
        }
        args.extend([
            "--filter".into(),
            format!("label=com.docker.compose.project={}", self.project_name),
        ]);
        let output = tokio::process::Command::new("docker")
            .args(args)
            .env_clear()
            .envs(environment)
            .output()
            .await
            .unwrap();
        assert!(
            output.status.success(),
            "docker {kind} ls failed: {}",
            bounded_diagnostic(&output.stderr)
        );
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn assert_profile_survives(&self, expected: &ProjectProfile) {
        assert!(self.directory.path().is_dir());
        assert!(self.compose_path.is_file());
        assert_eq!(&self.profile, expected);
    }
}

fn bounded_diagnostic(bytes: &[u8]) -> String {
    const MAX_DIAGNOSTIC_BYTES: usize = 4096;
    let text = String::from_utf8_lossy(bytes);
    let text = text.trim();
    if text.len() <= MAX_DIAGNOSTIC_BYTES {
        return text.to_owned();
    }
    let mut start = text.len() - (MAX_DIAGNOSTIC_BYTES - 3);
    while !text.is_char_boundary(start) {
        start += 1;
    }
    format!("...{}", &text[start..])
}

#[test]
fn bounded_diagnostic_caps_lossy_utf8_rendering() {
    let rendered = bounded_diagnostic(&[0xff; 4096]);
    assert!(rendered.starts_with("..."));
    assert!(rendered.len() <= 4096);
}

struct CleanupGuard<'a> {
    profile: &'a ProjectProfile,
    armed: bool,
}

impl<'a> CleanupGuard<'a> {
    fn new(profile: &'a ProjectProfile) -> Self {
        Self {
            profile,
            armed: true,
        }
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for CleanupGuard<'_> {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let args = compose_args(self.profile, ComposeOperation::Down);
        let endpoint =
            std::env::var("DOCKER_HOST").unwrap_or_else(|_| "unix:///var/run/docker.sock".into());
        let environment =
            colui_adapters::runtime::build_cli_environment(&endpoint, std::env::vars().collect());
        let mut command = std::process::Command::new("docker");
        command
            .args(args)
            .current_dir(&self.profile.working_directory)
            .env_clear()
            .envs(environment)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        #[cfg(unix)]
        {
            use std::os::unix::process::CommandExt;
            unsafe {
                command.pre_exec(|| {
                    if libc::setpgid(0, 0) != 0 {
                        return Err(std::io::Error::last_os_error());
                    }
                    Ok(())
                });
            }
        }
        let mut child = match command.spawn() {
            Ok(child) => child,
            Err(error) => {
                eprintln!("CLEANUP FAILED: could not spawn docker compose down: {error}");
                return;
            }
        };
        let deadline = Instant::now() + Duration::from_secs(30);
        loop {
            match child.try_wait() {
                Ok(Some(status)) if status.success() => return,
                Ok(Some(status)) => {
                    eprintln!("CLEANUP FAILED: docker compose down exited {status}");
                    return;
                }
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(50))
                }
                Ok(None) => {
                    terminate_cleanup_group(&mut child);
                    let reap_deadline = Instant::now() + Duration::from_secs(2);
                    while Instant::now() < reap_deadline {
                        match child.try_wait() {
                            Ok(Some(_)) => {
                                eprintln!(
                                    "CLEANUP FAILED: docker compose down exceeded 30s and was killed"
                                );
                                return;
                            }
                            Ok(None) => std::thread::sleep(Duration::from_millis(50)),
                            Err(error) => {
                                eprintln!(
                                    "CLEANUP FAILED: could not reap docker compose down: {error}"
                                );
                                return;
                            }
                        }
                    }
                    let fallback = child.wait();
                    eprintln!(
                        "CLEANUP FAILED: docker compose down exceeded 30s; kill and bounded reap exhausted; blocking reap fallback: {fallback:?}"
                    );
                    return;
                }
                Err(error) => {
                    terminate_cleanup_group(&mut child);
                    let fallback = child.wait();
                    eprintln!(
                        "CLEANUP FAILED: could not poll docker compose down: {error}; blocking reap fallback: {fallback:?}"
                    );
                    return;
                }
            }
        }
    }
}

#[cfg(unix)]
fn terminate_cleanup_group(child: &mut std::process::Child) {
    let pid = child.id();
    let result = unsafe { libc::kill(-(pid as i32), libc::SIGKILL) };
    if result != 0 {
        eprintln!(
            "CLEANUP FAILED: could not kill docker compose process group: {}",
            std::io::Error::last_os_error()
        );
    }
}

#[cfg(not(unix))]
fn terminate_cleanup_group(child: &mut std::process::Child) {
    eprintln!("CLEANUP WARNING: process groups unsupported; killing direct cleanup child");
    let _ = child.kill();
}
