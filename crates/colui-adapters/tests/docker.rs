#![cfg(feature = "docker-tests")]

use colui_adapters::runtime::{
    compose_args, ComposeOperation, ComposeProcessRunner, RuntimeGateway,
};
use colui_app::{ComposeInvocation, ComposeRunner, DockerApi, RuntimeConnector};
use colui_domain::{
    ProfileDraft, ProfileId, ProjectProfile, RegistrationOrigin, RuntimeSessionState,
};
use std::path::PathBuf;
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
    fixture.tear_down(&gateway).await.unwrap();
    cleanup.disarm();
    fixture.assert_project_resources_absent().await;
    fixture.assert_profile_survives(&profile_before);
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
        let mut args = compose_args(&self.profile, ComposeOperation::Up);
        if scale_worker {
            args.extend(["--scale".into(), "worker=2".into()]);
        }
        gateway.invoke(self.invocation(args)).await.map(|_| ())
    }

    async fn stop(&self, gateway: &RuntimeGateway) -> Result<(), colui_domain::AppError> {
        gateway
            .invoke(self.invocation(compose_args(&self.profile, ComposeOperation::Stop)))
            .await
            .map(|_| ())
    }

    async fn tear_down(&self, gateway: &RuntimeGateway) -> Result<(), colui_domain::AppError> {
        gateway
            .invoke(self.invocation(compose_args(&self.profile, ComposeOperation::Down)))
            .await
            .map(|_| ())
    }

    async fn assert_containers_present(&self, gateway: &RuntimeGateway) {
        let containers = gateway.list_containers().await.unwrap();
        let matching = self
            .containers_with_exact_project(gateway, &containers)
            .await;
        assert!(
            !matching.is_empty(),
            "expected disposable project containers"
        );
    }

    async fn assert_containers_stopped(&self, gateway: &RuntimeGateway) {
        let containers = gateway.list_containers().await.unwrap();
        let matching = self
            .containers_with_exact_project(gateway, &containers)
            .await;
        assert!(matching
            .iter()
            .any(|container| { container.state == colui_domain::ContainerState::Stopped }));
    }

    async fn containers_with_service(&self, gateway: &RuntimeGateway, service: &str) -> usize {
        let containers = gateway.list_containers().await.unwrap();
        self.containers_with_exact_project(gateway, &containers)
            .await
            .into_iter()
            .filter(|container| container.service_name.as_deref() == Some(service))
            .count()
    }

    async fn containers_with_exact_project(
        &self,
        gateway: &RuntimeGateway,
        containers: &[colui_domain::ContainerInstance],
    ) -> Vec<colui_domain::ContainerInstance> {
        let mut matching = Vec::new();
        for container in containers {
            let details = gateway.inspect_container(&container.id).await.unwrap();
            if details.labels.get("com.docker.compose.project") == Some(&self.project_name) {
                matching.push(details.instance);
            }
        }
        matching
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
        let output = tokio::process::Command::new("docker")
            .args([
                kind,
                "ls",
                "-q",
                "--filter",
                &format!("label=com.docker.compose.project={}", self.project_name),
            ])
            .env_clear()
            .envs(environment)
            .output()
            .await
            .unwrap();
        assert!(output.status.success(), "docker {kind} ls failed");
        String::from_utf8_lossy(&output.stdout).into_owned()
    }

    fn assert_profile_survives(&self, expected: &ProjectProfile) {
        assert!(self.directory.path().is_dir());
        assert!(self.compose_path.is_file());
        assert_eq!(&self.profile, expected);
    }

    fn invocation(&self, args: Vec<String>) -> ComposeInvocation {
        ComposeInvocation {
            executable: PathBuf::from("docker"),
            args,
            working_directory: self.profile.working_directory.clone(),
            environment: colui_adapters::runtime::build_cli_environment(
                std::env::var("DOCKER_HOST")
                    .ok()
                    .as_deref()
                    .unwrap_or("unix:///var/run/docker.sock"),
                std::env::vars().collect(),
            ),
            deadline: Instant::now() + Duration::from_secs(120),
        }
    }
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
        let mut child = match std::process::Command::new("docker")
            .args(args)
            .current_dir(&self.profile.working_directory)
            .env_clear()
            .envs(environment)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::piped())
            .spawn()
        {
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
                    let stderr = child
                        .stderr
                        .take()
                        .and_then(|mut stream| {
                            use std::io::Read;
                            let mut text = String::new();
                            stream.read_to_string(&mut text).ok().map(|_| text)
                        })
                        .unwrap_or_default();
                    eprintln!("CLEANUP FAILED: docker compose down exited {status}: {stderr}");
                    return;
                }
                Ok(None) if Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(50))
                }
                Ok(None) => {
                    let _ = child.kill();
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
                    eprintln!(
                        "CLEANUP FAILED: docker compose down exceeded 30s; kill reap timed out"
                    );
                    return;
                }
                Err(error) => {
                    let _ = child.kill();
                    eprintln!("CLEANUP FAILED: could not poll docker compose down: {error}");
                    return;
                }
            }
        }
    }
}
