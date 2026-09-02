use crate::runtime::bollard_fingerprint;
use bollard::container::{InspectContainerOptions, ListContainersOptions};
use bollard::models::{ContainerInspectResponse, ContainerSummary};
use bollard::Docker;
use colui_app::RuntimeFuture;
use colui_domain::{
    AppError, AppErrorCode, ContainerDetails, ContainerId, ContainerInstance, ContainerState,
    DaemonFingerprint, PortBinding,
};
use std::collections::BTreeMap;

pub trait DockerControl: Send + Sync {
    fn info(&self) -> RuntimeFuture<'_, DaemonFingerprint>;
    fn list(&self) -> RuntimeFuture<'_, Vec<ContainerInstance>>;
    fn inspect(&self, id: &ContainerId) -> RuntimeFuture<'_, ContainerDetails>;
}

pub struct DockerApiAdapter {
    docker: Docker,
}

impl DockerApiAdapter {
    pub fn new(docker: Docker) -> Self {
        Self { docker }
    }
    pub fn docker(&self) -> &Docker {
        &self.docker
    }
}

impl DockerControl for DockerApiAdapter {
    fn info(&self) -> RuntimeFuture<'_, DaemonFingerprint> {
        Box::pin(async move {
            self.docker
                .info()
                .await
                .map_err(api_error)
                .and_then(|i| bollard_fingerprint(&i))
        })
    }
    fn list(&self) -> RuntimeFuture<'_, Vec<ContainerInstance>> {
        Box::pin(async move {
            self.docker
                .list_containers(Some(ListContainersOptions::<String> {
                    all: true,
                    ..Default::default()
                }))
                .await
                .map_err(api_error)
                .map(|items| items.into_iter().map(summary).collect())
        })
    }
    fn inspect(&self, id: &ContainerId) -> RuntimeFuture<'_, ContainerDetails> {
        let id = id.0.clone();
        Box::pin(async move {
            self.docker
                .inspect_container(&id, Some(InspectContainerOptions { size: false }))
                .await
                .map_err(api_error)
                .map(inspected)
        })
    }
}

fn api_error(error: bollard::errors::Error) -> AppError {
    AppError::new(
        AppErrorCode::RuntimeUnavailable,
        "docker_api",
        None,
        "Docker API request failed",
    )
    .with_details(error.to_string())
}
fn state(value: Option<&str>) -> ContainerState {
    match value {
        Some("running") => ContainerState::Running,
        Some("created") | Some("exited") | Some("dead") | Some("paused") => ContainerState::Stopped,
        _ => ContainerState::Unknown,
    }
}
fn summary(value: ContainerSummary) -> ContainerInstance {
    ContainerInstance {
        id: ContainerId(value.id.unwrap_or_default()),
        name: value
            .names
            .and_then(|mut v| v.pop())
            .unwrap_or_default()
            .trim_start_matches('/')
            .into(),
        image: value.image.unwrap_or_default(),
        state: state(value.state.as_deref()),
        status_text: value.status.unwrap_or_default(),
        service_name: value
            .labels
            .as_ref()
            .and_then(|v| v.get("com.docker.compose.service").cloned()),
        published_ports: value
            .ports
            .unwrap_or_default()
            .into_iter()
            .filter_map(|p| {
                Some(PortBinding {
                    host_ip: p.ip.unwrap_or_default(),
                    host_port: p.public_port?,
                    container_port: p.private_port,
                    protocol: p.typ.map(|value| value.to_string()).unwrap_or_default(),
                })
            })
            .collect(),
    }
}
fn inspected(value: ContainerInspectResponse) -> ContainerDetails {
    let config = value.config.unwrap_or_default();
    let state_value = value
        .state
        .as_ref()
        .and_then(|v| v.status.as_ref())
        .map(|v| v.as_ref());
    ContainerDetails {
        instance: ContainerInstance {
            id: ContainerId(value.id.unwrap_or_default()),
            name: value
                .name
                .unwrap_or_default()
                .trim_start_matches('/')
                .into(),
            image: config.image.unwrap_or_default(),
            state: state(state_value),
            status_text: value
                .state
                .and_then(|v| v.status)
                .map(|v| v.to_string())
                .unwrap_or_default(),
            service_name: config
                .labels
                .as_ref()
                .and_then(|v| v.get("com.docker.compose.service").cloned()),
            published_ports: vec![],
        },
        labels: config
            .labels
            .unwrap_or_default()
            .into_iter()
            .collect::<BTreeMap<_, _>>(),
    }
}
