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
    fixture.assert_no_containers(&gateway).await;
    fixture.assert_profile_survives();
}

#[tokio::test]
async fn disposable_compose_fixture_scales_worker_to_two_containers() {
    if !docker_available().await {
        eprintln!("SKIP: local Docker daemon unavailable");
        return;
    }

    let fixture = TempComposeFixture::new(true);
    let gateway = RuntimeGateway::new(Box::new(ComposeProcessRunner::default()));
    assert!(matches!(
        gateway.connect_runtime(None).await.unwrap(),
        RuntimeSessionState::Ready(_)
    ));

    let mut cleanup = CleanupGuard::new(&fixture.profile);
    fixture.apply(&gateway, true).await.unwrap();
    let containers = gateway.list_containers().await.unwrap();
    let workers = containers
        .iter()
        .filter(|container| container.service_name.as_deref() == Some("worker"))
        .count();
    assert_eq!(workers, 2, "expected two scaled worker containers");
    fixture.tear_down(&gateway).await.unwrap();
    cleanup.disarm();
    fixture.assert_no_containers(&gateway).await;
    fixture.assert_profile_survives();
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
        assert!(containers.iter().any(|container| {
            container.service_name.is_some() && container.name.starts_with(&self.project_name)
        }));
    }

    async fn assert_containers_stopped(&self, gateway: &RuntimeGateway) {
        let containers = gateway.list_containers().await.unwrap();
        assert!(containers.iter().any(|container| {
            container.service_name.is_some()
                && container.name.starts_with(&self.project_name)
                && container.state == colui_domain::ContainerState::Stopped
        }));
    }

    async fn assert_no_containers(&self, gateway: &RuntimeGateway) {
        let containers = gateway.list_containers().await.unwrap();
        assert!(!containers
            .iter()
            .any(|container| { container.name.starts_with(&self.project_name) }));
    }

    fn assert_profile_survives(&self) {
        assert!(self.directory.path().is_dir());
        assert!(self.compose_path.is_file());
        assert_eq!(
            self.profile.compose_files,
            vec![PathBuf::from("compose.yml")]
        );
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
        let _ = std::process::Command::new("docker")
            .args(args)
            .current_dir(&self.profile.working_directory)
            .env_clear()
            .envs(environment)
            .status();
    }
}
